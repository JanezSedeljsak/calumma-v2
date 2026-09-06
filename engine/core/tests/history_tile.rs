use calumma_core::history::*;
use calumma_core::history_tile::HistoryTile;
use calumma_core::layer::*;
use calumma_core::limits::{HISTORY_COMPACT_TILES_PER_SWEEP, HISTORY_HOT_COMMANDS};
use calumma_core::tile::*;
use std::sync::Arc;

const COORD: TileCoord = TileCoord { x: 0, y: 0 };

struct TileHost<'a> {
    layer: &'a mut Layer,
}

impl HistoryMutator for TileHost<'_> {
    fn apply_command(&mut self, command: &HistoryCommand) {
        for diff in &command.diffs {
            if diff.layer_id == self.layer.id {
                if let Some(tiles) = self.layer.tiles_mut() {
                    tiles.restore_tiles(&diff.tiles);
                }
            }
        }
        for diff in &command.transforms {
            if diff.layer_id == self.layer.id {
                self.layer.transform = diff.transform;
            }
        }
    }

    fn invert_command(&mut self, command: &HistoryCommand) -> HistoryCommand {
        let mut diffs = Vec::new();
        let mut transforms = Vec::new();
        let mut bytes = 0usize;
        for diff in &command.diffs {
            if diff.layer_id != self.layer.id {
                continue;
            }
            let Some(grid) = self.layer.tiles() else {
                continue;
            };
            let coords: Vec<_> = diff.tiles.keys().copied().collect();
            let tiles = grid.snapshot_tiles(&coords);
            bytes += snapshot_bytes(&tiles);
            diffs.push(TileDiff {
                layer_id: diff.layer_id.clone(),
                tiles,
            });
        }
        for diff in &command.transforms {
            if diff.layer_id == self.layer.id {
                transforms.push(TransformDiff {
                    layer_id: diff.layer_id.clone(),
                    transform: self.layer.transform,
                });
                bytes += 32;
            }
        }
        HistoryCommand {
            diffs,
            masks: Vec::new(),
            runs: Vec::new(),
            transforms,
            props: Vec::new(),
            vectors: Vec::new(),
            stack: None,
            active_layer_index: Some(0),
            bytes,
        }
    }

    fn set_active_layer_index(&mut self, _index: usize) {}
}

fn undo(history: &mut History, layer: &mut Layer) -> bool {
    let Some(command) = history.take_undo() else {
        return false;
    };
    let mut host = TileHost { layer };
    let inverse = host.invert_command(&command);
    host.apply_command(&command);
    history.finish_undo(command, inverse);
    true
}

fn noisy_tile() -> Arc<Vec<u8>> {
    let mut px = vec![0u8; TILE_BYTES];
    for (i, b) in px.iter_mut().enumerate() {
        *b = ((i * 31 + i / 7) % 251) as u8;
    }
    Arc::new(px)
}

fn uniform_pixels(rgba: [u8; 4]) -> Arc<Vec<u8>> {
    Arc::new(rgba.repeat(TILE_BYTES / 4))
}

/// The gate the whole design turns on. A snapshot shares its tile with the live document, so
/// compressing it would force the copy that sharing was avoiding — strictly worse memory.
#[test]
fn a_tile_the_document_still_shares_is_never_compacted() {
    let pixels = noisy_tile();
    let live = Arc::clone(&pixels);
    let mut tile = HistoryTile::from_pixels(pixels);
    assert_eq!(tile.compact(), 0);
    assert!(tile.is_compactable());
    assert_eq!(tile.budget_bytes(), TILE_BYTES);
    drop(live);
    assert!(tile.compact() > 0);
    assert!(!tile.is_compactable());
}

#[test]
fn a_flat_tile_collapses_to_its_colour() {
    let mut tile = HistoryTile::from_pixels(uniform_pixels([9, 8, 7, 255]));
    assert!(tile.compact() > 0);
    assert!(matches!(tile, HistoryTile::Uniform([9, 8, 7, 255])));
    assert!(tile.budget_bytes() < 16);
    assert_eq!(
        *tile.materialize(&mut Vec::new()),
        *uniform_pixels([9, 8, 7, 255])
    );
}

#[test]
fn a_drawn_tile_compresses_and_round_trips_byte_exact() {
    let original = noisy_tile();
    let mut tile = HistoryTile::from_pixels(Arc::new(original.as_ref().clone()));
    assert!(tile.compact() > 0);
    assert!(matches!(tile, HistoryTile::Compressed(_)));
    assert!(tile.budget_bytes() < TILE_BYTES);
    assert_eq!(*tile.materialize(&mut Vec::new()), *original);
}

/// Undoing a flat fill has to rebuild one allocation shared by every tile it covered, the way
/// `TileGrid::fill_uniform` laid it down — otherwise undo silently multiplies Paper's single
/// shared tile into one allocation per tile.
#[test]
fn materialising_the_same_colour_twice_shares_one_allocation() {
    let mut a = HistoryTile::from_pixels(uniform_pixels([255, 255, 255, 255]));
    let mut b = HistoryTile::from_pixels(uniform_pixels([255, 255, 255, 255]));
    a.compact();
    b.compact();
    let mut cache = Vec::new();
    let first = a.materialize(&mut cache);
    let second = b.materialize(&mut cache);
    assert!(Arc::ptr_eq(&first, &second));
}

#[test]
fn the_newest_commands_are_left_hot() {
    let mut layer = Layer::new("L1", 256, 256);
    let mut history = History::default();
    for i in 0..HISTORY_HOT_COMMANDS {
        let before = layer.tiles().unwrap().snapshot_tiles(&[COORD]);
        layer
            .tiles_mut()
            .unwrap()
            .set_pixel(i as i32, 0, [255, 0, 0, 255]);
        history.push_layer_tiles(layer.id.clone(), before, Some(0));
    }
    assert_eq!(history.compact_cold(), 0);
}

/// The point of the feature: compaction has to move `memory_used`, because that is the number
/// `evict()` reads. A saving the budget cannot see buys no undo depth at all.
#[test]
fn compaction_frees_budget_and_undo_still_restores_the_pixels() {
    let mut layer = Layer::new("L1", 2048, 256);
    let mut history = History::default();
    let coords: Vec<TileCoord> = (0..6).map(|x| TileCoord { x, y: 0 }).collect();
    for (step, coord) in coords.iter().enumerate() {
        let (ox, _) = coord.origin();
        layer
            .tiles_mut()
            .unwrap()
            .paint_rect(DocRect::new(ox, 0, ox + 256, 256), |_, _, _| {
                Some([step as u8, 40, 90, 255])
            });
    }
    let first_pixel = layer.tiles().unwrap().get_pixel(4, 4);

    for step in 0..HISTORY_HOT_COMMANDS + 4 {
        let before = layer.tiles().unwrap().snapshot_tiles(&coords);
        layer
            .tiles_mut()
            .unwrap()
            .set_pixel(step as i32, 0, [1, 2, 3, 255]);
        history.push_layer_tiles(layer.id.clone(), before, Some(0));
    }

    let before_sweep = history.memory_used();
    let mut freed = 0;
    for _ in 0..8 {
        freed += history.compact_cold();
    }
    assert!(freed > 0, "cold tiles should have compacted");
    assert_eq!(history.memory_used(), before_sweep - freed);

    while history.can_undo() {
        assert!(undo(&mut history, &mut layer));
    }
    assert_eq!(layer.tiles().unwrap().get_pixel(4, 4), first_pixel);
}

/// A pseudo-random, effectively incompressible fill — unlike `noisy_tile`'s cheap modular
/// formula (which zstd finds plenty of structure in), this is the shape of data `compact`'s
/// "compression didn't help" bailout (history_tile.rs's `frame.len() >= before`) actually needs.
fn incompressible_tile() -> Arc<Vec<u8>> {
    let mut state = 0x9E3779B97F4A7C15u64;
    let mut px = vec![0u8; TILE_BYTES];
    for chunk in px.chunks_mut(8) {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let bytes = state.to_le_bytes();
        chunk.copy_from_slice(&bytes[..chunk.len()]);
    }
    Arc::new(px)
}

#[test]
fn held_bytes_charges_each_variant_its_own_way() {
    let pixels = noisy_tile();
    let pixels_tile = HistoryTile::from_pixels(Arc::clone(&pixels));
    assert_eq!(pixels_tile.held_bytes(|p| p.len()), TILE_BYTES);
    assert_eq!(pixels_tile.held_bytes(|_| 0), 0);

    let mut uniform_tile = HistoryTile::from_pixels(uniform_pixels([1, 2, 3, 255]));
    uniform_tile.compact();
    assert!(matches!(uniform_tile, HistoryTile::Uniform(_)));
    assert_eq!(uniform_tile.held_bytes(|_| panic!("not Pixels")), 4);

    let mut compressed_tile = HistoryTile::from_pixels(noisy_tile());
    compressed_tile.compact();
    assert!(matches!(compressed_tile, HistoryTile::Compressed(_)));
    let held = compressed_tile.held_bytes(|_| panic!("not Pixels"));
    assert_eq!(held, compressed_tile.budget_bytes());
    assert!(held < TILE_BYTES);
}

/// `compact` on a tile that isn't `Pixels` any more is the everyday case in a real sweep: the
/// same tile can come up cold across multiple sweeps after it has already collapsed once.
#[test]
fn compacting_an_already_compacted_tile_is_a_free_no_op() {
    let mut uniform_tile = HistoryTile::from_pixels(uniform_pixels([5, 6, 7, 255]));
    assert!(uniform_tile.compact() > 0);
    assert_eq!(uniform_tile.compact(), 0);
    assert!(matches!(uniform_tile, HistoryTile::Uniform([5, 6, 7, 255])));

    let mut compressed_tile = HistoryTile::from_pixels(noisy_tile());
    assert!(compressed_tile.compact() > 0);
    assert_eq!(compressed_tile.compact(), 0);
    assert!(matches!(compressed_tile, HistoryTile::Compressed(_)));
}

/// Not every drawn tile is worth compressing: truly incompressible pixels would come back out
/// of zstd *larger* than the tile they replaced, so `compact` has to leave them as `Pixels`
/// rather than "shrinking" them into a bigger allocation.
#[test]
fn incompressible_pixels_are_left_uncompacted() {
    let mut tile = HistoryTile::from_pixels(incompressible_tile());
    assert_eq!(tile.compact(), 0);
    assert!(matches!(tile, HistoryTile::Pixels(_)));
    assert_eq!(tile.budget_bytes(), TILE_BYTES);
}

#[test]
fn a_sweep_never_touches_more_than_its_budget() {
    let mut layer = Layer::new("L1", 8192, 256);
    let mut history = History::default();
    let coords: Vec<TileCoord> = (0..32).map(|x| TileCoord { x, y: 0 }).collect();
    for coord in &coords {
        let (ox, _) = coord.origin();
        layer
            .tiles_mut()
            .unwrap()
            .paint_rect(DocRect::new(ox, 0, ox + 256, 256), |x, y, _| {
                Some([(x * 7) as u8, (y * 13) as u8, 5, 255])
            });
    }
    for step in 0..HISTORY_HOT_COMMANDS + 1 {
        let before = layer.tiles().unwrap().snapshot_tiles(&coords);
        layer
            .tiles_mut()
            .unwrap()
            .set_pixel(step as i32, 0, [1, 2, 3, 255]);
        history.push_layer_tiles(layer.id.clone(), before, Some(0));
    }
    let freed = history.compact_cold();
    assert!(freed > 0);
    assert!(
        freed <= HISTORY_COMPACT_TILES_PER_SWEEP * TILE_BYTES,
        "one sweep compacted more than {HISTORY_COMPACT_TILES_PER_SWEEP} tiles"
    );
}
