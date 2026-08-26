use crate::history_tile::HistoryTile;
use crate::layer::Layer;
use crate::limits::{
    HISTORY_COMPACT_TILES_PER_SWEEP, HISTORY_HOT_COMMANDS, HISTORY_MEMORY_BUDGET_BYTES,
};
use crate::tile::{TileMap, TILE_BYTES};
use crate::transform::LayerTransform;
use calumma_text::TextRun;
use std::sync::Arc;

pub type TileSnapshot = TileMap<Option<HistoryTile>>;

#[derive(Clone, Debug)]
pub struct TileDiff {
    pub layer_id: String,
    pub tiles: TileSnapshot,
}

#[derive(Clone, Debug)]
pub struct MaskDiff {
    pub layer_id: String,
    pub mask: Option<Vec<u8>>,
}

/// The run a text layer had before a typing session. Its tiles are already covered by a
/// `TileDiff`, but the run is what the project actually stores — without this, undoing a
/// session would repaint the old glyphs and still save the new string.
#[derive(Clone, Debug)]
pub struct RunDiff {
    pub layer_id: String,
    pub run: Box<TextRun>,
}

#[derive(Clone, Debug)]
pub struct TransformDiff {
    pub layer_id: String,
    pub transform: Option<LayerTransform>,
}

#[derive(Clone, Debug)]
pub struct HistoryCommand {
    pub diffs: Vec<TileDiff>,
    pub masks: Vec<MaskDiff>,
    pub runs: Vec<RunDiff>,
    pub transforms: Vec<TransformDiff>,
    pub active_layer_index: Option<usize>,
    pub bytes: usize,
}

impl HistoryCommand {
    /// Compact this command's tiles, spending at most `budget` of them, and fold the saving
    /// back into `bytes` so the stack's own accounting stays true. Only tiles that still cost
    /// a full tile are counted against the budget — already-compacted ones are free to skip.
    fn compact(&mut self, budget: &mut usize) -> usize {
        let mut freed = 0;
        for diff in &mut self.diffs {
            for tile in diff.tiles.values_mut().flatten() {
                if *budget == 0 {
                    return freed;
                }
                if !tile.is_compactable() {
                    continue;
                }
                *budget -= 1;
                freed += tile.compact();
            }
        }
        self.bytes = self.bytes.saturating_sub(freed);
        freed
    }
}

/// What the budget charges a snapshot. Every tile pays what its current representation costs
/// — a full tile while it is raw pixels, its frame length once compacted — so shrinking a
/// cold command genuinely buys undo depth rather than only looking smaller in the memory
/// panel. That is where this feature's value comes from: `evict()` reads this number.
pub fn snapshot_bytes(tiles: &TileSnapshot) -> usize {
    tiles
        .values()
        .flatten()
        .map(HistoryTile::budget_bytes)
        .sum()
}

#[derive(Clone, Debug)]
pub struct History {
    undo: Vec<HistoryCommand>,
    redo: Vec<HistoryCommand>,
    memory_budget: usize,
    memory_used: usize,
}

impl Default for History {
    fn default() -> Self {
        Self::new(HISTORY_MEMORY_BUDGET_BYTES)
    }
}

impl History {
    pub fn new(memory_budget: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            memory_budget: memory_budget.max(TILE_BYTES),
            memory_used: 0,
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn memory_used(&self) -> usize {
        self.memory_used
    }

    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// What the undo/redo stacks are really holding. `memory_used` is the *budget* estimate —
    /// it charges a full tile for every snapshot entry, including the ones still shared with
    /// the live document — so accounting asks here instead and passes a counter that has
    /// already seen the layers' tiles.
    pub fn held_bytes(&self, mut tile_bytes: impl FnMut(&Arc<Vec<u8>>) -> usize) -> usize {
        let mut total = 0;
        for command in self.undo.iter().chain(&self.redo) {
            for diff in &command.diffs {
                for tile in diff.tiles.values().flatten() {
                    total += tile.held_bytes(&mut tile_bytes);
                }
            }
            total += command
                .masks
                .iter()
                .filter_map(|m| m.mask.as_ref())
                .map(Vec::len)
                .sum::<usize>();
            total += command.runs.iter().map(|r| r.run.text.len()).sum::<usize>();
        }
        total
    }

    pub fn push(&mut self, command: HistoryCommand) {
        if command.diffs.is_empty()
            && command.masks.is_empty()
            && command.runs.is_empty()
            && command.transforms.is_empty()
        {
            return;
        }
        self.clear_redo();
        self.memory_used += command.bytes;
        self.undo.push(command);
        self.evict();
    }

    fn clear_redo(&mut self) {
        for dropped in self.redo.drain(..) {
            self.memory_used = self.memory_used.saturating_sub(dropped.bytes);
        }
    }

    pub fn push_layer_tiles(
        &mut self,
        layer_id: String,
        tiles: TileSnapshot,
        active_layer_index: Option<usize>,
    ) {
        let bytes = snapshot_bytes(&tiles);
        self.push(HistoryCommand {
            diffs: vec![TileDiff { layer_id, tiles }],
            masks: Vec::new(),
            runs: Vec::new(),
            transforms: Vec::new(),
            active_layer_index,
            bytes,
        });
    }

    pub fn push_remove_background(
        &mut self,
        layer_id: String,
        tiles: TileSnapshot,
        transform: Option<LayerTransform>,
        active_layer_index: Option<usize>,
    ) {
        let bytes = snapshot_bytes(&tiles);
        self.push(HistoryCommand {
            diffs: vec![TileDiff {
                layer_id: layer_id.clone(),
                tiles,
            }],
            masks: Vec::new(),
            runs: Vec::new(),
            transforms: vec![TransformDiff {
                layer_id,
                transform,
            }],
            active_layer_index,
            bytes,
        });
    }

    /// One typing session: the tiles it repainted and the run it started from, restored
    /// together so undo takes back what was typed and not only what was drawn.
    pub fn push_layer_text(
        &mut self,
        layer_id: String,
        tiles: TileSnapshot,
        run: Box<TextRun>,
        active_layer_index: Option<usize>,
    ) {
        let bytes = snapshot_bytes(&tiles) + run.text.len();
        self.push(HistoryCommand {
            diffs: vec![TileDiff {
                layer_id: layer_id.clone(),
                tiles,
            }],
            masks: Vec::new(),
            runs: vec![RunDiff { layer_id, run }],
            transforms: Vec::new(),
            active_layer_index,
            bytes,
        });
    }

    pub fn push_layer_mask(
        &mut self,
        layer_id: String,
        before: Option<Vec<u8>>,
        active_layer_index: Option<usize>,
    ) {
        let bytes = before.as_ref().map(|m| m.len()).unwrap_or(0);
        self.push(HistoryCommand {
            diffs: Vec::new(),
            masks: vec![MaskDiff {
                layer_id,
                mask: before,
            }],
            runs: Vec::new(),
            transforms: Vec::new(),
            active_layer_index,
            bytes,
        });
    }

    pub fn undo(&mut self, layers: &mut [Layer], active_layer: &mut usize) -> bool {
        let Some(command) = self.undo.pop() else {
            return false;
        };
        let inverse = self.step(&command, layers, active_layer);
        self.redo.push(inverse);
        true
    }

    pub fn redo(&mut self, layers: &mut [Layer], active_layer: &mut usize) -> bool {
        let Some(command) = self.redo.pop() else {
            return false;
        };
        let inverse = self.step(&command, layers, active_layer);
        self.undo.push(inverse);
        true
    }

    fn step(
        &mut self,
        command: &HistoryCommand,
        layers: &mut [Layer],
        active_layer: &mut usize,
    ) -> HistoryCommand {
        let inverse = invert_command(command, layers, Some(*active_layer));
        apply_command(command, layers);
        if let Some(index) = command.active_layer_index {
            if index < layers.len() {
                *active_layer = index;
            }
        }
        self.memory_used = self.memory_used.saturating_sub(command.bytes);
        self.memory_used += inverse.bytes;
        inverse
    }

    /// Shrink cold tiles until the per-sweep budget runs out, returning the bytes reclaimed.
    ///
    /// Age picks the *candidates* and unique ownership decides which of them are actually
    /// worth touching (`HistoryTile::compact`). The commands either side of the history
    /// cursor — the last `HISTORY_HOT_COMMANDS` of each stack — are left alone so an
    /// immediate undo/redo never pays decompression.
    ///
    /// Bounded per call because it runs under the engine lock: whatever is left stays cold
    /// and is picked up on the next tick. A stack with nothing cold queues no work at all.
    pub fn compact_cold(&mut self) -> usize {
        let mut budget = HISTORY_COMPACT_TILES_PER_SWEEP;
        let mut freed = 0;
        let undo_cold = self.undo.len().saturating_sub(HISTORY_HOT_COMMANDS);
        let redo_cold = self.redo.len().saturating_sub(HISTORY_HOT_COMMANDS);
        for command in self.undo[..undo_cold]
            .iter_mut()
            .chain(self.redo[..redo_cold].iter_mut())
        {
            if budget == 0 {
                break;
            }
            freed += command.compact(&mut budget);
        }
        self.memory_used = self.memory_used.saturating_sub(freed);
        freed
    }

    fn evict(&mut self) {
        while self.memory_used > self.memory_budget && self.undo.len() > 1 {
            let dropped = self.undo.remove(0);
            self.memory_used = self.memory_used.saturating_sub(dropped.bytes);
        }
    }
}

fn apply_command(command: &HistoryCommand, layers: &mut [Layer]) {
    for diff in &command.runs {
        if let Some(layer) = layers.iter_mut().find(|l| l.id == diff.layer_id) {
            layer.set_run(*diff.run.clone());
        }
    }
    for diff in &command.diffs {
        if let Some(layer) = layers.iter_mut().find(|l| l.id == diff.layer_id) {
            if let Some(tiles) = layer.tiles_mut() {
                tiles.restore_tiles(&diff.tiles);
            }
        }
    }
    for diff in &command.masks {
        if let Some(layer) = layers.iter_mut().find(|l| l.id == diff.layer_id) {
            layer.set_mask(diff.mask.clone());
        }
    }
    for diff in &command.transforms {
        if let Some(layer) = layers.iter_mut().find(|l| l.id == diff.layer_id) {
            layer.transform = diff.transform;
        }
    }
}

fn invert_command(
    command: &HistoryCommand,
    layers: &[Layer],
    active_layer_index: Option<usize>,
) -> HistoryCommand {
    let mut diffs = Vec::new();
    let mut masks = Vec::new();
    let mut runs = Vec::new();
    let mut transforms = Vec::new();
    let mut bytes = 0;
    for diff in &command.diffs {
        if let Some(layer) = layers.iter().find(|l| l.id == diff.layer_id) {
            let Some(grid) = layer.tiles() else {
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
    }
    for diff in &command.runs {
        if let Some(run) = layers
            .iter()
            .find(|l| l.id == diff.layer_id)
            .and_then(Layer::run)
        {
            bytes += run.text.len();
            runs.push(RunDiff {
                layer_id: diff.layer_id.clone(),
                run: Box::new(run.clone()),
            });
        }
    }
    for diff in &command.masks {
        if let Some(layer) = layers.iter().find(|l| l.id == diff.layer_id) {
            let mask = layer.mask_owned();
            bytes += mask.as_ref().map(|m| m.len()).unwrap_or(0);
            masks.push(MaskDiff {
                layer_id: diff.layer_id.clone(),
                mask,
            });
        }
    }
    for diff in &command.transforms {
        if let Some(layer) = layers.iter().find(|l| l.id == diff.layer_id) {
            transforms.push(TransformDiff {
                layer_id: diff.layer_id.clone(),
                transform: layer.transform,
            });
        }
    }
    HistoryCommand {
        diffs,
        masks,
        runs,
        transforms,
        active_layer_index,
        bytes,
    }
}
