use crate::document::Document;
use crate::history::stack_snapshot_bytes;
use crate::history::{
    snapshot_bytes, HistoryCommand, HistoryMutator, LayerPropDiff, StackSnapshot, TileDiff,
    TransformDiff, VectorDiff,
};
use crate::layer::Layer;
use crate::vector::VectorItem;
use crate::vector_edit::VectorPick;

impl Document {
    pub(crate) fn apply_history_command(&mut self, command: &HistoryCommand) {
        HistoryMutator::apply_command(self, command);
    }

    pub(crate) fn invert_history_command(&mut self, command: &HistoryCommand) -> HistoryCommand {
        HistoryMutator::invert_command(self, command)
    }

    pub(crate) fn set_active_layer_index(&mut self, index: usize) {
        if index < self.layers.len() {
            self.active_layer = index;
        }
    }

    pub(crate) fn snapshot_stack(&self) -> StackSnapshot {
        StackSnapshot {
            layers: self.layers.clone(),
            width: self.width,
            height: self.height,
            active_layer_index: self.active_layer,
            layer_selection: self.layer_selection.clone(),
            selected_vector_layer: self.selected_vector.map(|pick| pick.layer),
        }
    }

    pub(crate) fn restore_stack(&mut self, snap: StackSnapshot) {
        self.commit_text();
        self.transform_drag = None;
        self.vector_drag = None;
        self.layers = snap.layers;
        self.width = snap.width;
        self.height = snap.height;
        self.active_layer = snap
            .active_layer_index
            .min(self.layers.len().saturating_sub(1));
        self.layer_selection = snap.layer_selection;
        self.selected_vector = snap
            .selected_vector_layer
            .filter(|&layer| {
                self.layers
                    .get(layer)
                    .is_some_and(|l| l.content.item().is_some())
            })
            .map(|layer| VectorPick { layer });
        if self.selected_vector.is_none() {
            self.vector_drag = None;
        }
        self.bump_vector_revision();
    }

    pub(crate) fn snapshot_layer_props(&self, index: usize) -> Option<LayerPropDiff> {
        let layer = self.layers.get(index)?;
        Some(LayerPropDiff {
            layer_id: layer.id.clone(),
            opacity: layer.opacity,
            blend_mode: layer.blend_mode,
            adjustments: layer.adjustments,
            transform: layer.transform,
        })
    }

    pub(crate) fn record_stack_history(&mut self) {
        let snap = self.snapshot_stack();
        let bytes = stack_snapshot_bytes(&snap);
        self.history
            .push_stack(snap, Some(self.active_layer), bytes);
    }

    pub(crate) fn record_layer_props_history(&mut self, index: usize) {
        let Some(before) = self.snapshot_layer_props(index) else {
            return;
        };
        let bytes = prop_diff_bytes(&before);
        self.history
            .push_props(vec![before], Some(self.active_layer), bytes);
    }

    pub(crate) fn record_transforms_history(&mut self, transforms: Vec<TransformDiff>) {
        if transforms.is_empty() {
            return;
        }
        let bytes = transforms.len() * 32;
        self.history
            .push_transforms(transforms, Some(self.active_layer), bytes);
    }

    pub(crate) fn record_vector_history(&mut self, layer_id: String, item: Option<VectorItem>) {
        let bytes = 128;
        self.history.push_vector(
            VectorDiff { layer_id, item },
            Some(self.active_layer),
            bytes,
        );
    }
}

fn prop_diff_bytes(prop: &LayerPropDiff) -> usize {
    64 + prop.adjustments.map(|_| 64).unwrap_or(0)
}

fn normalized_transform(
    t: crate::transform::LayerTransform,
) -> Option<crate::transform::LayerTransform> {
    if t.is_identity() {
        None
    } else {
        Some(t)
    }
}

impl HistoryMutator for Document {
    fn apply_command(&mut self, command: &HistoryCommand) {
        if let Some(stack) = &command.stack {
            self.restore_stack(stack.clone());
        }
        for diff in &command.runs {
            if let Some(layer) = self.layers.iter_mut().find(|l| l.id == diff.layer_id) {
                layer.set_run(*diff.run.clone());
            }
        }
        for diff in &command.diffs {
            if let Some(layer) = self.layers.iter_mut().find(|l| l.id == diff.layer_id) {
                if let Some(tiles) = layer.tiles_mut() {
                    tiles.restore_tiles(&diff.tiles);
                }
            }
        }
        for diff in &command.masks {
            if let Some(layer) = self.layers.iter_mut().find(|l| l.id == diff.layer_id) {
                layer.set_mask(diff.mask.clone());
            }
        }
        for diff in &command.props {
            if let Some(layer) = self.layers.iter_mut().find(|l| l.id == diff.layer_id) {
                layer.opacity = diff.opacity;
                layer.blend_mode = diff.blend_mode;
                layer.adjustments = diff.adjustments;
                layer.transform = diff.transform;
            }
        }
        for diff in &command.vectors {
            if let Some(layer) = self.layers.iter_mut().find(|l| l.id == diff.layer_id) {
                match &diff.item {
                    Some(item) => {
                        if let Some(slot) = layer.content.item_mut() {
                            *slot = item.clone();
                        }
                    }
                    None => {}
                }
            }
        }
        for diff in &command.transforms {
            if let Some(layer) = self.layers.iter_mut().find(|l| l.id == diff.layer_id) {
                layer.transform = diff.transform;
            }
        }
        self.bump_vector_revision();
    }

    fn invert_command(&mut self, command: &HistoryCommand) -> HistoryCommand {
        let mut diffs = Vec::new();
        let mut masks = Vec::new();
        let mut runs = Vec::new();
        let mut transforms = Vec::new();
        let mut props = Vec::new();
        let mut vectors = Vec::new();
        let mut bytes = 0usize;

        if command.stack.is_some() {
            let stack = self.snapshot_stack();
            bytes += stack_snapshot_bytes(&stack);
            return HistoryCommand {
                diffs,
                masks,
                runs,
                transforms,
                props,
                vectors,
                stack: Some(stack),
                active_layer_index: Some(self.active_layer),
                bytes,
            };
        }

        for diff in &command.diffs {
            if let Some(layer) = self.layers.iter().find(|l| l.id == diff.layer_id) {
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
            if let Some(run) = self
                .layers
                .iter()
                .find(|l| l.id == diff.layer_id)
                .and_then(Layer::run)
            {
                bytes += run.text.len();
                runs.push(crate::history::RunDiff {
                    layer_id: diff.layer_id.clone(),
                    run: Box::new(run.clone()),
                });
            }
        }
        for diff in &command.masks {
            if let Some(layer) = self.layers.iter().find(|l| l.id == diff.layer_id) {
                let mask = layer.mask_owned();
                bytes += mask.as_ref().map(|m| m.len()).unwrap_or(0);
                masks.push(crate::history::MaskDiff {
                    layer_id: diff.layer_id.clone(),
                    mask,
                });
            }
        }
        for diff in &command.props {
            if let Some(layer) = self.layers.iter().find(|l| l.id == diff.layer_id) {
                let prop = LayerPropDiff {
                    layer_id: diff.layer_id.clone(),
                    opacity: layer.opacity,
                    blend_mode: layer.blend_mode,
                    adjustments: layer.adjustments,
                    transform: layer.transform,
                };
                bytes += prop_diff_bytes(&prop);
                props.push(prop);
            }
        }
        for diff in &command.vectors {
            if let Some(layer) = self.layers.iter().find(|l| l.id == diff.layer_id) {
                let item = layer.content.item().cloned();
                bytes += 128;
                vectors.push(VectorDiff {
                    layer_id: diff.layer_id.clone(),
                    item,
                });
            }
        }
        for diff in &command.transforms {
            if let Some(layer) = self.layers.iter().find(|l| l.id == diff.layer_id) {
                transforms.push(TransformDiff {
                    layer_id: diff.layer_id.clone(),
                    transform: layer.transform,
                });
                bytes += 32;
            }
        }
        HistoryCommand {
            diffs,
            masks,
            runs,
            transforms,
            props,
            vectors,
            stack: None,
            active_layer_index: Some(self.active_layer),
            bytes,
        }
    }

    fn set_active_layer_index(&mut self, index: usize) {
        if index < self.layers.len() {
            self.active_layer = index;
        }
    }
}

impl Document {
    pub(crate) fn commit_transform_drag_history(&mut self) {
        let Some(drag) = self.transform_drag.take() else {
            return;
        };
        let mut transforms = Vec::new();
        for target in &drag.targets {
            let Some(layer) = self.layers.get(target.layer_index) else {
                continue;
            };
            let current = layer.transform.unwrap_or_default();
            if current == target.start_transform {
                continue;
            }
            transforms.push(TransformDiff {
                layer_id: layer.id.clone(),
                transform: normalized_transform(target.start_transform),
            });
        }
        self.record_transforms_history(transforms);
    }

    pub(crate) fn commit_vector_drag_history(&mut self) {
        let Some(drag) = self.vector_drag.take() else {
            return;
        };
        let Some(layer) = self.layers.get(drag.pick.layer) else {
            return;
        };
        let Some(current) = layer.content.item() else {
            return;
        };
        if *current == drag.start_item {
            return;
        }
        self.record_vector_history(layer.id.clone(), Some(drag.start_item));
    }

    pub(crate) fn record_transforms_for_indices(&mut self, indices: &[usize]) {
        let mut transforms = Vec::new();
        for &index in indices {
            let Some(layer) = self.layers.get(index) else {
                continue;
            };
            transforms.push(TransformDiff {
                layer_id: layer.id.clone(),
                transform: layer.transform,
            });
        }
        self.record_transforms_history(transforms);
    }
}
