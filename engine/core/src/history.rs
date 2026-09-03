use crate::filters::Adjustments;
use crate::history_tile::HistoryTile;
use crate::layer::{BlendMode, Layer};
use crate::limits::{
    HISTORY_COMPACT_TILES_PER_SWEEP, HISTORY_HOT_COMMANDS, HISTORY_MEMORY_BUDGET_BYTES,
};
use crate::tile::{TileMap, TILE_BYTES};
use crate::transform::LayerTransform;
use crate::vector::VectorItem;
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
pub struct LayerPropDiff {
    pub layer_id: String,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub adjustments: Option<Adjustments>,
    pub transform: Option<LayerTransform>,
}

#[derive(Clone, Debug)]
pub struct VectorDiff {
    pub layer_id: String,
    pub item: Option<VectorItem>,
}

#[derive(Clone, Debug)]
pub struct StackSnapshot {
    pub layers: Vec<Layer>,
    pub width: u32,
    pub height: u32,
    pub active_layer_index: usize,
    pub layer_selection: Vec<usize>,
    pub selected_vector_layer: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct HistoryCommand {
    pub diffs: Vec<TileDiff>,
    pub masks: Vec<MaskDiff>,
    pub runs: Vec<RunDiff>,
    pub transforms: Vec<TransformDiff>,
    pub props: Vec<LayerPropDiff>,
    pub vectors: Vec<VectorDiff>,
    pub stack: Option<StackSnapshot>,
    pub active_layer_index: Option<usize>,
    pub bytes: usize,
}

impl HistoryCommand {
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

pub fn snapshot_bytes(tiles: &TileSnapshot) -> usize {
    tiles
        .values()
        .flatten()
        .map(HistoryTile::budget_bytes)
        .sum()
}

pub fn stack_snapshot_bytes(stack: &StackSnapshot) -> usize {
    stack.layers.iter().map(layer_charge_bytes).sum()
}

fn layer_charge_bytes(layer: &Layer) -> usize {
    let mut bytes = 0usize;
    if let Some(tiles) = layer.tiles() {
        bytes += tiles.len() * TILE_BYTES;
    }
    if let Some(mask) = layer.mask() {
        bytes += mask.len();
    }
    if let Some(run) = layer.run() {
        bytes += run.text.len();
    }
    bytes + 64
}

pub trait HistoryMutator {
    fn apply_command(&mut self, command: &HistoryCommand);
    fn invert_command(&mut self, command: &HistoryCommand) -> HistoryCommand;
    fn set_active_layer_index(&mut self, index: usize);
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
            if let Some(stack) = &command.stack {
                total += stack_snapshot_bytes(stack);
            }
        }
        total
    }

    pub fn push(&mut self, command: HistoryCommand) {
        if command.diffs.is_empty()
            && command.masks.is_empty()
            && command.runs.is_empty()
            && command.transforms.is_empty()
            && command.props.is_empty()
            && command.vectors.is_empty()
            && command.stack.is_none()
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
            props: Vec::new(),
            vectors: Vec::new(),
            stack: None,
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
            props: Vec::new(),
            vectors: Vec::new(),
            stack: None,
            active_layer_index,
            bytes,
        });
    }

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
            props: Vec::new(),
            vectors: Vec::new(),
            stack: None,
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
            props: Vec::new(),
            vectors: Vec::new(),
            stack: None,
            active_layer_index,
            bytes,
        });
    }

    pub fn push_stack(
        &mut self,
        stack: StackSnapshot,
        active_layer_index: Option<usize>,
        bytes: usize,
    ) {
        self.push(HistoryCommand {
            diffs: Vec::new(),
            masks: Vec::new(),
            runs: Vec::new(),
            transforms: Vec::new(),
            props: Vec::new(),
            vectors: Vec::new(),
            stack: Some(stack),
            active_layer_index,
            bytes,
        });
    }

    pub fn push_props(
        &mut self,
        props: Vec<LayerPropDiff>,
        active_layer_index: Option<usize>,
        bytes: usize,
    ) {
        self.push(HistoryCommand {
            diffs: Vec::new(),
            masks: Vec::new(),
            runs: Vec::new(),
            transforms: Vec::new(),
            props,
            vectors: Vec::new(),
            stack: None,
            active_layer_index,
            bytes,
        });
    }

    pub fn push_transforms(
        &mut self,
        transforms: Vec<TransformDiff>,
        active_layer_index: Option<usize>,
        bytes: usize,
    ) {
        self.push(HistoryCommand {
            diffs: Vec::new(),
            masks: Vec::new(),
            runs: Vec::new(),
            transforms,
            props: Vec::new(),
            vectors: Vec::new(),
            stack: None,
            active_layer_index,
            bytes,
        });
    }

    pub fn push_vector(
        &mut self,
        vector: VectorDiff,
        active_layer_index: Option<usize>,
        bytes: usize,
    ) {
        self.push(HistoryCommand {
            diffs: Vec::new(),
            masks: Vec::new(),
            runs: Vec::new(),
            transforms: Vec::new(),
            props: Vec::new(),
            vectors: vec![vector],
            stack: None,
            active_layer_index,
            bytes,
        });
    }

    pub fn take_undo(&mut self) -> Option<HistoryCommand> {
        self.undo.pop()
    }

    pub fn take_redo(&mut self) -> Option<HistoryCommand> {
        self.redo.pop()
    }

    pub fn finish_undo(&mut self, command: HistoryCommand, inverse: HistoryCommand) {
        self.memory_used = self.memory_used.saturating_sub(command.bytes);
        self.memory_used += inverse.bytes;
        self.redo.push(inverse);
    }

    pub fn finish_redo(&mut self, command: HistoryCommand, inverse: HistoryCommand) {
        self.memory_used = self.memory_used.saturating_sub(command.bytes);
        self.memory_used += inverse.bytes;
        self.undo.push(inverse);
    }

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
