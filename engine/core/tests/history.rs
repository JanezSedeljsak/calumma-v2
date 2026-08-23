use calumma_core::history::*;
use calumma_core::layer::*;
use calumma_core::tile::*;

fn undo(history: &mut History, layer: &mut Layer) -> bool {
    let mut active = 0;
    history.undo(std::slice::from_mut(layer), &mut active)
}

fn redo(history: &mut History, layer: &mut Layer) -> bool {
    let mut active = 0;
    history.redo(std::slice::from_mut(layer), &mut active)
}

fn pixel(layer: &Layer, x: i32, y: i32) -> [u8; 4] {
    layer.tiles().unwrap().get_pixel(x, y)
}

#[test]
fn undo_redo_pixel() {
    let mut layer = Layer::new("L1", 256, 256);
    let mut history = History::default();
    let coord = TileCoord { x: 0, y: 0 };
    let before = layer.tiles().unwrap().snapshot_tiles(&[coord]);
    layer.tiles_mut().unwrap().set_pixel(5, 5, [255, 0, 0, 255]);
    history.push_layer_tiles(layer.id.clone(), before, Some(0));
    assert_eq!(pixel(&layer, 5, 5), [255, 0, 0, 255]);
    assert!(undo(&mut history, &mut layer));
    assert_eq!(pixel(&layer, 5, 5), [0, 0, 0, 0]);
    assert!(redo(&mut history, &mut layer));
    assert_eq!(pixel(&layer, 5, 5), [255, 0, 0, 255]);
}

#[test]
fn absent_tiles_cost_nothing() {
    let mut layer = Layer::new("L1", 4096, 4096);
    let mut history = History::default();
    let coords: Vec<TileCoord> = (0..100).map(|i| TileCoord { x: i, y: 0 }).collect();
    let snap = layer.tiles().unwrap().snapshot_tiles(&coords);
    assert!(snap.values().all(|v| v.is_none()));
    layer.tiles_mut().unwrap().set_pixel(0, 0, [1, 2, 3, 4]);
    history.push_layer_tiles(layer.id.clone(), snap, Some(0));
    assert_eq!(history.memory_used(), 0);
}

#[test]
fn present_tiles_are_charged_once_each() {
    let mut layer = Layer::new("L1", 1024, 1024);
    let mut history = History::default();
    layer.tiles_mut().unwrap().set_pixel(10, 10, [1, 1, 1, 255]);
    layer
        .tiles_mut()
        .unwrap()
        .set_pixel(600, 600, [1, 1, 1, 255]);
    let coords: Vec<TileCoord> = layer.tiles().unwrap().coords().collect();
    let snap = layer.tiles().unwrap().snapshot_tiles(&coords);
    history.push_layer_tiles(layer.id.clone(), snap, Some(0));
    assert_eq!(history.memory_used(), 2 * TILE_BYTES);
}

#[test]
fn discarded_redo_releases_budget() {
    let mut layer = Layer::new("L1", 256, 256);
    let mut history = History::default();
    let coord = TileCoord { x: 0, y: 0 };
    for i in 0..3 {
        let before = layer.tiles().unwrap().snapshot_tiles(&[coord]);
        layer.tiles_mut().unwrap().set_pixel(i, i, [255, 0, 0, 255]);
        history.push_layer_tiles(layer.id.clone(), before, Some(0));
    }
    assert!(undo(&mut history, &mut layer));
    assert!(undo(&mut history, &mut layer));
    assert!(history.can_redo());

    let before = layer.tiles().unwrap().snapshot_tiles(&[coord]);
    layer.tiles_mut().unwrap().set_pixel(9, 9, [1, 2, 3, 4]);
    history.push_layer_tiles(layer.id.clone(), before, Some(0));

    assert!(!history.can_redo());
    assert_eq!(history.undo_depth(), 2);
}

#[test]
fn undo_redo_mask() {
    let mut layer = Layer::new("L1", 4, 4);
    let mut history = History::default();
    let before = layer.mask_owned();
    layer.set_mask(Some(vec![255; 16]));
    history.push_layer_mask(layer.id.clone(), before, Some(0));
    assert!(layer.mask().is_some());
    assert!(undo(&mut history, &mut layer));
    assert!(layer.mask().is_none());
    assert!(redo(&mut history, &mut layer));
    assert_eq!(layer.mask().map(|m| m.len()), Some(16));
}

#[test]
fn undo_restores_active_layer() {
    let mut layers = vec![Layer::new("A", 64, 64), Layer::new("B", 64, 64)];
    let mut history = History::default();
    let coord = TileCoord { x: 0, y: 0 };
    let before = layers[1].tiles().unwrap().snapshot_tiles(&[coord]);
    layers[1]
        .tiles_mut()
        .unwrap()
        .set_pixel(1, 1, [9, 9, 9, 255]);
    history.push_layer_tiles(layers[1].id.clone(), before, Some(1));

    let mut active = 0;
    assert!(history.undo(&mut layers, &mut active));
    assert_eq!(active, 1);
    assert!(history.redo(&mut layers, &mut active));
    assert_eq!(active, 0);
}

/// A command carrying no diff of any kind must not leave an undo step behind — otherwise
/// `⌘Z` would silently consume presses on gestures that changed nothing while the real edit
/// stayed put one press further back.
#[test]
fn a_command_with_no_diffs_at_all_is_not_pushed() {
    let mut layer = Layer::new("L1", 64, 64);
    let mut history = History::default();
    history.push(HistoryCommand {
        diffs: Vec::new(),
        masks: Vec::new(),
        runs: Vec::new(),
        active_layer_index: Some(0),
        bytes: 0,
    });
    assert!(!history.can_undo());
    assert_eq!(history.undo_depth(), 0);
    assert_eq!(history.memory_used(), 0);
    assert!(!undo(&mut history, &mut layer));
}

/// A snapshot of tiles that were never allocated costs no memory but is still a real step:
/// it is how a stroke onto empty space is taken back. Keeping the callers responsible for not
/// pushing a no-op is why `push`'s own guard only catches a wholly empty command.
#[test]
fn a_snapshot_of_absent_tiles_is_still_an_undoable_step() {
    let mut layer = Layer::new("L1", 64, 64);
    let mut history = History::default();
    let coord = TileCoord { x: 0, y: 0 };
    let before = layer.tiles().unwrap().snapshot_tiles(&[coord]);
    assert!(before.values().all(Option::is_none), "nothing painted yet");
    layer.tiles_mut().unwrap().set_pixel(4, 4, [255, 0, 0, 255]);
    history.push_layer_tiles(layer.id.clone(), before, Some(0));

    assert!(history.can_undo());
    assert_eq!(history.memory_used(), 0, "an absent tile costs nothing");
    assert!(undo(&mut history, &mut layer));
    assert_eq!(pixel(&layer, 4, 4), [0, 0, 0, 0]);
}

#[test]
fn undo_and_redo_on_an_empty_stack_report_that_nothing_happened() {
    let mut layer = Layer::new("L1", 64, 64);
    let mut history = History::default();
    assert!(!history.can_undo());
    assert!(!history.can_redo());
    assert!(!undo(&mut history, &mut layer));
    assert!(!redo(&mut history, &mut layer));
}

/// Redo is only reachable straight after an undo — reaching the bottom of the undo stack and
/// asking again has to stop rather than wrap around into the redo side.
#[test]
fn undoing_past_the_bottom_stops_rather_than_wrapping() {
    let mut layer = Layer::new("L1", 64, 64);
    let mut history = History::default();
    let coord = TileCoord { x: 0, y: 0 };
    let before = layer.tiles().unwrap().snapshot_tiles(&[coord]);
    layer.tiles_mut().unwrap().set_pixel(1, 1, [255, 0, 0, 255]);
    history.push_layer_tiles(layer.id.clone(), before, Some(0));

    assert!(undo(&mut history, &mut layer));
    assert!(!undo(&mut history, &mut layer), "nothing left to undo");
    assert_eq!(pixel(&layer, 1, 1), [0, 0, 0, 0], "and nothing moved");

    assert!(redo(&mut history, &mut layer));
    assert!(!redo(&mut history, &mut layer), "nothing left to redo");
    assert_eq!(pixel(&layer, 1, 1), [255, 0, 0, 255]);
}

/// A step names its layer by id, so a diff whose layer is gone — merged away, deleted — is
/// skipped rather than applied to whatever now sits at that position.
#[test]
fn a_step_whose_layer_is_gone_is_skipped_not_misapplied() {
    let mut layers = vec![Layer::new("A", 64, 64), Layer::new("B", 64, 64)];
    let mut history = History::default();
    let coord = TileCoord { x: 0, y: 0 };
    let before = layers[0].tiles().unwrap().snapshot_tiles(&[coord]);
    layers[0]
        .tiles_mut()
        .unwrap()
        .set_pixel(2, 2, [255, 0, 0, 255]);
    history.push_layer_tiles(layers[0].id.clone(), before, Some(0));

    layers.remove(0);
    let untouched = layers[0].tiles().unwrap().get_pixel(2, 2);
    let mut active = 0;
    history.undo(&mut layers, &mut active);
    assert_eq!(
        layers[0].tiles().unwrap().get_pixel(2, 2),
        untouched,
        "the surviving layer was not painted with the dead one's undo"
    );
}

/// A tile diff naming a layer that has no tile grid — a vector layer took its place — is
/// skipped, since there is nothing to write the pixels into.
#[test]
fn a_tile_diff_against_a_layer_with_no_pixels_is_skipped() {
    let mut raster = Layer::new("A", 64, 64);
    let mut history = History::default();
    let coord = TileCoord { x: 0, y: 0 };
    let before = raster.tiles().unwrap().snapshot_tiles(&[coord]);
    raster
        .tiles_mut()
        .unwrap()
        .set_pixel(3, 3, [255, 0, 0, 255]);
    history.push_layer_tiles(raster.id.clone(), before, Some(0));

    let mut vector = Layer::vector("V", Vec::new());
    vector.id = raster.id.clone();
    let mut layers = vec![vector];
    let mut active = 0;
    assert!(history.undo(&mut layers, &mut active));
    assert!(layers[0].tiles().is_none(), "still a vector layer");
}

/// Every push after an undo drops the redo branch — the standard linear-history rule — and
/// the budget it was holding has to come back with it.
#[test]
fn a_new_edit_after_an_undo_drops_the_redo_branch_and_its_budget() {
    let mut layer = Layer::new("L1", 512, 512);
    let mut history = History::default();
    let coord = TileCoord { x: 0, y: 0 };
    for i in 1..4 {
        let before = layer.tiles().unwrap().snapshot_tiles(&[coord]);
        layer.tiles_mut().unwrap().set_pixel(i, i, [255, 0, 0, 255]);
        history.push_layer_tiles(layer.id.clone(), before, Some(0));
    }
    assert!(undo(&mut history, &mut layer));
    let with_redo = history.memory_used();
    assert!(history.can_redo());

    let before = layer.tiles().unwrap().snapshot_tiles(&[coord]);
    layer.tiles_mut().unwrap().set_pixel(20, 20, [1, 2, 3, 4]);
    history.push_layer_tiles(layer.id.clone(), before, Some(0));

    assert!(!history.can_redo());
    assert!(
        history.memory_used() <= with_redo,
        "the dropped branch gave its bytes back"
    );
}

/// A mask diff and a tile diff for the same layer travel in separate commands, so undoing one
/// must not disturb the other.
#[test]
fn a_mask_undo_leaves_the_pixels_alone() {
    let mut layer = Layer::new("L1", 4, 4);
    let mut history = History::default();
    layer.tiles_mut().unwrap().set_pixel(1, 1, [9, 9, 9, 255]);

    let before = layer.mask_owned();
    layer.set_mask(Some(vec![128; 16]));
    history.push_layer_mask(layer.id.clone(), before, Some(0));

    assert!(undo(&mut history, &mut layer));
    assert!(layer.mask().is_none());
    assert_eq!(
        pixel(&layer, 1, 1),
        [9, 9, 9, 255],
        "the mask step did not touch the tiles"
    );
}
