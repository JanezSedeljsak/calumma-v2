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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::Layer;

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
}
