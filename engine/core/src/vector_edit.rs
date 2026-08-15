use crate::document::Document;
use crate::layer::Layer;
use crate::limits::{VECTOR_NUDGE_STEP, VECTOR_PICK_SLACK_PX};
use crate::transform::{bounds_center, LayerTransform};
use crate::vector::{items_bounds, VectorItem};

/// Which item, in which layer. A vector layer is a *list*, so an index inside the layer is
/// as much a part of the address as the layer itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VectorPick {
    pub layer: usize,
    pub item: usize,
}

/// A live item drag. The item is captured whole at pointer-down and every frame re-derives
/// it from that capture plus the total pointer delta, so a drag never accumulates rounding
/// the way repeated incremental translation would.
#[derive(Clone, Debug)]
pub struct VectorItemDrag {
    pick: VectorPick,
    start_pointer: (f32, f32),
    start_item: VectorItem,
}

fn layer_pivot(layer: &Layer) -> Option<((f32, f32), LayerTransform)> {
    let t = layer.transform.filter(|t| !t.is_identity())?;
    let raw = items_bounds(layer.content.items()?)?;
    Some((bounds_center(raw), t))
}

fn to_item_space(layer: &Layer, p: (f32, f32)) -> (f32, f32) {
    match layer_pivot(layer) {
        Some((pivot, t)) => t.inverse(pivot, p),
        None => p,
    }
}

fn delta_to_item_space(layer: &Layer, d: (f32, f32)) -> (f32, f32) {
    match layer.transform.filter(|t| !t.is_identity()) {
        Some(t) => t.inverse_delta(d),
        None => d,
    }
}

/// A layer scaled up makes its items look bigger, so the pick slack — a fixed number of
/// screen pixels — has to shrink by the same factor once it is expressed in item space.
fn item_space_slack(layer: &Layer, doc_slack: f32) -> f32 {
    let scale = layer
        .transform
        .filter(|t| !t.is_identity())
        .map(|t| (t.scale_x.abs() + t.scale_y.abs()) * 0.5)
        .unwrap_or(1.0);
    doc_slack / scale.max(1e-6)
}

impl Document {
    /// The topmost visible vector item under a document point, searched the way the eye
    /// reads the board: last layer first, and inside a layer the last item first, because
    /// that is the one drawn on top.
    pub fn vector_item_at(&self, doc_x: f32, doc_y: f32) -> Option<VectorPick> {
        let doc_slack = VECTOR_PICK_SLACK_PX / self.camera.zoom.max(1e-6);
        for (layer_index, layer) in self.layers.iter().enumerate().rev() {
            if !layer.visible {
                continue;
            }
            let Some(items) = layer.content.items() else {
                continue;
            };
            let local = to_item_space(layer, (doc_x, doc_y));
            let slack = item_space_slack(layer, doc_slack);
            for (item_index, item) in items.iter().enumerate().rev() {
                if item.pick_distance(local.0, local.1) <= slack {
                    return Some(VectorPick {
                        layer: layer_index,
                        item: item_index,
                    });
                }
            }
        }
        None
    }

    /// The current pick, re-validated against the layer stack on every read. Layers can be
    /// removed, merged or replaced without any of those paths having to remember that a
    /// selection exists — a stale pick simply stops resolving.
    pub fn selected_vector_item(&self) -> Option<VectorPick> {
        let pick = self.selected_vector?;
        let items = self.layers.get(pick.layer)?.content.items()?;
        (pick.item < items.len()).then_some(pick)
    }

    pub fn clear_vector_selection(&mut self) {
        self.selected_vector = None;
        self.vector_drag = None;
    }

    pub fn select_vector_item_at(&mut self, doc_x: f32, doc_y: f32) -> bool {
        match self.vector_item_at(doc_x, doc_y) {
            Some(pick) => {
                self.selected_vector = Some(pick);
                self.active_layer = pick.layer;
                true
            }
            None => {
                self.clear_vector_selection();
                false
            }
        }
    }

    /// Selecting and grabbing are the same gesture: whatever the click lands on becomes the
    /// selection *and* starts moving, so an item never needs two clicks to be dragged.
    pub fn begin_vector_item_drag(&mut self, doc_x: f32, doc_y: f32) -> bool {
        let Some(pick) = self.vector_item_at(doc_x, doc_y) else {
            return false;
        };
        let Some(item) = self
            .layers
            .get(pick.layer)
            .and_then(|l| l.content.items())
            .and_then(|items| items.get(pick.item))
            .cloned()
        else {
            return false;
        };
        self.active_layer = pick.layer;
        self.selected_vector = Some(pick);
        self.vector_drag = Some(VectorItemDrag {
            pick,
            start_pointer: (doc_x, doc_y),
            start_item: item,
        });
        true
    }

    pub fn update_vector_item_drag(&mut self, doc_x: f32, doc_y: f32) -> bool {
        let Some(drag) = self.vector_drag.take() else {
            return false;
        };
        let doc_delta = (doc_x - drag.start_pointer.0, doc_y - drag.start_pointer.1);
        let moved = self.move_item_by(drag.pick, doc_delta, |slot, (dx, dy)| {
            slot.set_translated(&drag.start_item, dx, dy)
        });
        self.vector_drag = Some(drag);
        moved
    }

    pub fn end_vector_item_drag(&mut self) -> bool {
        self.vector_drag.take().is_some()
    }

    pub fn is_dragging_vector_item(&self) -> bool {
        self.vector_drag.is_some()
    }

    /// Keyboard move of the selection, in document pixels — the shell sends a direction, the
    /// step is core's.
    pub fn nudge_selected_vector_item(&mut self, steps_x: f32, steps_y: f32) -> bool {
        let Some(pick) = self.selected_vector_item() else {
            return false;
        };
        let doc_delta = (steps_x * VECTOR_NUDGE_STEP, steps_y * VECTOR_NUDGE_STEP);
        self.move_item_by(pick, doc_delta, |slot, (dx, dy)| slot.translate(dx, dy))
    }

    pub fn delete_selected_vector_item(&mut self) -> bool {
        let Some(pick) = self.selected_vector_item() else {
            return false;
        };
        let Some(items) = self
            .layers
            .get_mut(pick.layer)
            .and_then(|l| l.content.items_mut())
        else {
            return false;
        };
        items.remove(pick.item);
        self.clear_vector_selection();
        self.bump_vector_revision();
        true
    }

    /// The selection box in **document** space: the item's own bounds carried through the
    /// layer transform, so the overlay sits on the item wherever the layer has been moved,
    /// scaled or rotated to.
    pub fn selected_vector_item_corners(&self) -> Option<[(f32, f32); 4]> {
        let pick = self.selected_vector_item()?;
        let layer = self.layers.get(pick.layer)?;
        let bounds = layer.content.items()?.get(pick.item)?.bounds()?;
        let Some((pivot, t)) = layer_pivot(layer) else {
            let (x0, y0, x1, y1) = bounds;
            return Some([(x0, y0), (x1, y0), (x1, y1), (x0, y1)]);
        };
        Some(t.transformed_corners(pivot, bounds))
    }

    pub fn vector_item_count(&self, layer_index: usize) -> usize {
        self.layers
            .get(layer_index)
            .and_then(|l| l.content.items())
            .map_or(0, <[VectorItem]>::len)
    }

    /// The one place an item is moved: it maps the document-space delta into the layer's own
    /// space — the only difference between a pointer drag and an arrow-key nudge — and marks
    /// the draw list stale. `edit` writes the new geometry into the item in place.
    fn move_item_by(
        &mut self,
        pick: VectorPick,
        doc_delta: (f32, f32),
        edit: impl FnOnce(&mut VectorItem, (f32, f32)),
    ) -> bool {
        let Some(layer) = self.layers.get_mut(pick.layer) else {
            return false;
        };
        let delta = delta_to_item_space(layer, doc_delta);
        let Some(slot) = layer
            .content
            .items_mut()
            .and_then(|items| items.get_mut(pick.item))
        else {
            return false;
        };
        edit(slot, delta);
        self.bump_vector_revision();
        true
    }
}
