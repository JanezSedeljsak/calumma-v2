use crate::document::layer_alpha_at;
use crate::document::{point_dist, Document, TransformHandle, HANDLE_HIT_RADIUS_PX};
use crate::layer::Layer;
use crate::limits::{VECTOR_NUDGE_STEP, VECTOR_PICK_SLACK_PX};
use crate::transform::{bounds_center, corner_scale, LayerTransform};
use crate::vector::VectorItem;

/// A vector layer is the item. Clicking it selects that layer; there is no second index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VectorPick {
    pub layer: usize,
}

/// A live item drag. The item is captured whole at pointer-down and every frame re-derives
/// it from that capture plus where the pointer is now, so a drag never accumulates rounding
/// the way repeated incremental edits would.
///
/// `handle` is which grip started it: `Move` for the item's body, a corner for a resize. One
/// drag type covers both because the pointer routing — Move tool and `⌘T` alike — only ever
/// asks whether *an item* is being dragged.
#[derive(Clone, Debug)]
pub struct VectorItemDrag {
    pick: VectorPick,
    handle: TransformHandle,
    start_pointer: (f32, f32),
    start_item: VectorItem,
    /// The item's document-space box at pointer-down. Moving an item by a document delta moves
    /// this box by exactly the same delta — `move_item_by` un-rotates the delta only so the
    /// layer transform can put it back — so the box the guides snap against is this one shifted,
    /// with no per-frame transform work.
    start_aabb: Option<(f32, f32, f32, f32)>,
    /// Where the layer sat when the drag began. Captured rather than read per frame because a
    /// vector layer's pivot is the centre of its *content*: resizing an item moves that centre,
    /// which would move the mapping the resize is being measured through and let the drag chase
    /// its own tail.
    start_placement: Placement,
}

/// A layer transform and the pivot it turns about, or `None` for the common untransformed
/// layer, which then costs nothing to map through.
type Placement = Option<((f32, f32), LayerTransform)>;

fn layer_pivot(layer: &Layer) -> Placement {
    let t = layer.transform.filter(|t| !t.is_identity())?;
    let raw = layer.content.item()?.bounds()?;
    Some((bounds_center(raw), t))
}

fn placed_to_item_space(placement: Placement, p: (f32, f32)) -> (f32, f32) {
    match placement {
        Some((pivot, t)) => t.inverse(pivot, p),
        None => p,
    }
}

fn to_item_space(layer: &Layer, p: (f32, f32)) -> (f32, f32) {
    placed_to_item_space(layer_pivot(layer), p)
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
    /// The topmost visible vector layer whose item covers a document point, searched last
    /// layer first because that is the one drawn on top. A hit is the layer — there is only
    /// one item in it.
    pub fn vector_item_at(&self, doc_x: f32, doc_y: f32) -> Option<VectorPick> {
        let doc_slack = VECTOR_PICK_SLACK_PX / self.camera.zoom.max(1e-6);
        for (layer_index, layer) in self.layers.iter().enumerate().rev() {
            if !layer.visible || layer.is_paper() || layer.opacity <= 0.0 {
                continue;
            }
            if let Some(item) = layer.content.item() {
                let local = to_item_space(layer, (doc_x, doc_y));
                let slack = item_space_slack(layer, doc_slack);
                if item.pick_distance(local.0, local.1) <= slack {
                    return Some(VectorPick { layer: layer_index });
                }
                continue;
            }
            if layer_alpha_at(layer, doc_x, doc_y, self.width, self.height) != 0 {
                return None;
            }
        }
        None
    }

    /// The current pick, re-validated against the layer stack on every read. Layers can be
    /// removed, merged or replaced without any of those paths having to remember that a
    /// selection exists — a stale pick simply stops resolving.
    pub fn selected_vector_item(&self) -> Option<VectorPick> {
        let pick = self.selected_vector?;
        self.layers.get(pick.layer)?.content.item().map(|_| pick)
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
    ///
    /// A corner handle of the item already selected is checked first. Handles outrank content
    /// the same way the layer frame's do — otherwise a handle sitting over another item would
    /// select that one instead of resizing this one.
    pub fn begin_vector_item_drag(&mut self, doc_x: f32, doc_y: f32) -> bool {
        if let Some((pick, handle)) = self.selected_vector_handle_at(doc_x, doc_y) {
            return self.begin_item_drag(pick, handle, (doc_x, doc_y));
        }
        let Some(pick) = self.vector_item_at(doc_x, doc_y) else {
            return false;
        };
        self.begin_item_drag(pick, TransformHandle::Move, (doc_x, doc_y))
    }

    fn begin_item_drag(
        &mut self,
        pick: VectorPick,
        handle: TransformHandle,
        pointer: (f32, f32),
    ) -> bool {
        if self.layer_locked(pick.layer) {
            return false;
        }
        let Some(item) = self.item_for_pick(pick).cloned() else {
            return false;
        };
        self.active_layer = pick.layer;
        self.selected_vector = Some(pick);
        let start_aabb = self.vector_item_doc_aabb(pick);
        let start_placement = self.layers.get(pick.layer).and_then(layer_pivot);
        self.vector_drag = Some(VectorItemDrag {
            pick,
            handle,
            start_pointer: pointer,
            start_item: item,
            start_aabb,
            start_placement,
        });
        true
    }

    pub fn update_vector_item_drag(&mut self, doc_x: f32, doc_y: f32) -> bool {
        let Some(drag) = self.vector_drag.take() else {
            return false;
        };
        let changed = match drag.handle.corner_signs() {
            Some(signs) => self.scale_dragged_item(&drag, signs, (doc_x, doc_y)),
            None => self.move_dragged_item(&drag, (doc_x, doc_y)),
        };
        self.vector_drag = Some(drag);
        changed
    }

    fn move_dragged_item(&mut self, drag: &VectorItemDrag, pointer: (f32, f32)) -> bool {
        let mut doc_delta = (
            pointer.0 - drag.start_pointer.0,
            pointer.1 - drag.start_pointer.1,
        );
        if let Some(aabb) = drag.start_aabb {
            let moved_aabb = (
                aabb.0 + doc_delta.0,
                aabb.1 + doc_delta.1,
                aabb.2 + doc_delta.0,
                aabb.3 + doc_delta.1,
            );
            let (snap_x, snap_y) = self.snap_box_offset(moved_aabb);
            doc_delta = (doc_delta.0 + snap_x, doc_delta.1 + snap_y);
        }
        self.move_item_by(drag.pick, doc_delta, |slot, (dx, dy)| {
            slot.set_translated(&drag.start_item, dx, dy)
        })
    }

    /// Resize the item under a corner handle. The handle *is* the pointer, so it snaps to a
    /// guide directly like a `⌘T` corner does, then crosses into the item's own space where
    /// its box is axis-aligned whatever the layer transform is doing.
    ///
    /// The ink pad comes off both the box and the pointer's reach: a resize does not change
    /// stroke width, so that padding is a constant the ratio must not see, and taking it off
    /// both sides is what puts the dragged corner exactly where the pointer is — an arrow,
    /// whose head can pad its box by tens of pixels, would otherwise lag badly behind.
    fn scale_dragged_item(
        &mut self,
        drag: &VectorItemDrag,
        signs: (f32, f32),
        pointer: (f32, f32),
    ) -> bool {
        let Some(raw) = drag.start_item.geometry_bounds() else {
            return false;
        };
        let local = placed_to_item_space(drag.start_placement, self.snap_doc_point(pointer));
        let pivot = bounds_center(raw);
        let pad = drag.start_item.ink_pad();
        let half = ((raw.2 - raw.0) * 0.5, (raw.3 - raw.1) * 0.5);
        let reach = (
            local.0 - pivot.0 - pad * signs.0,
            local.1 - pivot.1 - pad * signs.1,
        );
        let scale = corner_scale(half, signs, reach, !self.shift_held);
        self.edit_item(drag.pick, |slot| {
            slot.set_scaled(&drag.start_item, pivot, scale)
        })
    }

    /// The corner of the selected item's box under a point, if any. These are the same four
    /// corners `selected_vector_item_corners` draws, so what is on screen and what answers a
    /// click cannot drift apart.
    fn selected_vector_handle_at(
        &self,
        doc_x: f32,
        doc_y: f32,
    ) -> Option<(VectorPick, TransformHandle)> {
        let pick = self.selected_vector_item()?;
        let corners = self.selected_vector_item_corners()?;
        let hit_r = HANDLE_HIT_RADIUS_PX / self.camera.zoom.max(1e-6);
        corners
            .iter()
            .zip(TransformHandle::CORNERS)
            .find(|(corner, _)| point_dist(**corner, (doc_x, doc_y)) <= hit_r)
            .map(|(_, handle)| (pick, handle))
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
        self.clear_vector_selection();
        self.remove_layer(pick.layer)
    }

    fn item_for_pick(&self, pick: VectorPick) -> Option<&VectorItem> {
        self.layers.get(pick.layer)?.content.item()
    }

    /// The selection box in **document** space: the item's own bounds carried through the
    /// layer transform, so the overlay sits on the item wherever the layer has been moved,
    /// scaled or rotated to. Corner order matches `TransformHandle::CORNERS`.
    pub fn selected_vector_item_corners(&self) -> Option<[(f32, f32); 4]> {
        let pick = self.selected_vector_item()?;
        let layer = self.layers.get(pick.layer)?;
        let bounds = self.item_for_pick(pick)?.bounds()?;
        let Some((pivot, t)) = layer_pivot(layer) else {
            let (x0, y0, x1, y1) = bounds;
            return Some([(x0, y0), (x1, y0), (x1, y1), (x0, y1)]);
        };
        Some(t.transformed_corners(pivot, bounds))
    }

    /// One item's box in **document** space — its own bounds carried through the layer
    /// transform, the same mapping `selected_vector_item_corners` draws the selection box with.
    fn vector_item_doc_aabb(&self, pick: VectorPick) -> Option<(f32, f32, f32, f32)> {
        let layer = self.layers.get(pick.layer)?;
        let bounds = self.item_for_pick(pick)?.bounds()?;
        let Some((pivot, t)) = layer_pivot(layer) else {
            return Some(bounds);
        };
        let corners = t.transformed_corners(pivot, bounds);
        let mut min = corners[0];
        let mut max = corners[0];
        for &(x, y) in &corners[1..] {
            min.0 = min.0.min(x);
            min.1 = min.1.min(y);
            max.0 = max.0.max(x);
            max.1 = max.1.max(y);
        }
        Some((min.0, min.1, max.0, max.1))
    }

    pub fn vector_item_count(&self, layer_index: usize) -> usize {
        usize::from(
            self.layers
                .get(layer_index)
                .is_some_and(|l| l.content.item().is_some()),
        )
    }

    /// The one place an item is moved: it maps the document-space delta into the layer's own
    /// space — the only difference between a pointer drag and an arrow-key nudge — before
    /// handing it to `edit`, which writes the new geometry into the item in place.
    fn move_item_by(
        &mut self,
        pick: VectorPick,
        doc_delta: (f32, f32),
        edit: impl FnOnce(&mut VectorItem, (f32, f32)),
    ) -> bool {
        let Some(layer) = self.layers.get(pick.layer) else {
            return false;
        };
        let delta = delta_to_item_space(layer, doc_delta);
        self.edit_item(pick, |slot| edit(slot, delta))
    }

    /// The one place an item's geometry is rewritten, and so the one place that marks the
    /// draw list stale. Everything above hands it geometry already expressed in item space.
    fn edit_item(&mut self, pick: VectorPick, edit: impl FnOnce(&mut VectorItem)) -> bool {
        let Some(slot) = self
            .layers
            .get_mut(pick.layer)
            .and_then(|l| l.content.item_mut())
        else {
            return false;
        };
        edit(slot);
        self.bump_vector_revision();
        true
    }
}
