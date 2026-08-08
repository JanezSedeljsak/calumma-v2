use crate::layer::Layer;
use crate::limits::HISTORY_MEMORY_BUDGET_BYTES;
use crate::tile::{TileCoord, TILE_BYTES};
use std::collections::HashMap;
use std::sync::Arc;

pub type TileSnapshot = HashMap<TileCoord, Option<Arc<Vec<u8>>>>;

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
pub struct HistoryCommand {
    pub diffs: Vec<TileDiff>,
    pub masks: Vec<MaskDiff>,
    pub active_layer_index: Option<usize>,
    pub bytes: usize,
}

pub fn snapshot_bytes(tiles: &TileSnapshot) -> usize {
    tiles.values().filter(|v| v.is_some()).count() * TILE_BYTES
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

    pub fn push(&mut self, command: HistoryCommand) {
        if command.diffs.is_empty() && command.masks.is_empty() {
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

    fn evict(&mut self) {
        while self.memory_used > self.memory_budget && self.undo.len() > 1 {
            let dropped = self.undo.remove(0);
            self.memory_used = self.memory_used.saturating_sub(dropped.bytes);
        }
    }
}

fn apply_command(command: &HistoryCommand, layers: &mut [Layer]) {
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
}

fn invert_command(
    command: &HistoryCommand,
    layers: &[Layer],
    active_layer_index: Option<usize>,
) -> HistoryCommand {
    let mut diffs = Vec::new();
    let mut masks = Vec::new();
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
    HistoryCommand {
        diffs,
        masks,
        active_layer_index,
        bytes,
    }
}
