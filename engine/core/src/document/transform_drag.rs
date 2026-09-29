//! `⌘T` and Move: the transform handles, the drag that edits a layer's `LayerTransform`, and
//! the outlines and bounds readout the board shows for the layers being moved.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TransformHandle {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
    Rotate,
    Move,
}

impl TransformHandle {
    /// The four scale handles in the order `LayerTransform::transformed_corners` emits them,
    /// so a corner index and a handle are the same fact read two ways. Both the layer frame
    /// and a vector item's frame zip against this.
    pub(crate) const CORNERS: [Self; 4] = [
        Self::TopLeft,
        Self::TopRight,
        Self::BottomRight,
        Self::BottomLeft,
    ];

    /// Which way this corner points from the box centre, or `None` for the two handles that
    /// do not scale anything.
    pub(crate) fn corner_signs(self) -> Option<(f32, f32)> {
        Some(match self {
            Self::TopLeft => (-1.0, -1.0),
            Self::TopRight => (1.0, -1.0),
            Self::BottomRight => (1.0, 1.0),
            Self::BottomLeft => (-1.0, 1.0),
            Self::Rotate | Self::Move => return None,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TransformTarget {
    pub(crate) layer_index: usize,
    pub(crate) pivot: (f32, f32),
    pub(crate) raw_bounds: (f32, f32, f32, f32),
    pub(crate) start_transform: LayerTransform,
}

#[derive(Clone, Debug)]
pub(crate) struct TransformDrag {
    pub(crate) targets: Vec<TransformTarget>,
    pub(crate) handle: TransformHandle,
    pub(crate) start_pointer: (f32, f32),
}

impl TransformDrag {
    pub(crate) fn single(
        layer_index: usize,
        handle: TransformHandle,
        pivot: (f32, f32),
        raw_bounds: (f32, f32, f32, f32),
        start_transform: LayerTransform,
        start_pointer: (f32, f32),
    ) -> Self {
        Self {
            targets: vec![TransformTarget {
                layer_index,
                pivot,
                raw_bounds,
                start_transform,
            }],
            handle,
            start_pointer,
        }
    }

    pub(crate) fn layer_move(targets: Vec<TransformTarget>, start_pointer: (f32, f32)) -> Self {
        Self {
            targets,
            handle: TransformHandle::Move,
            start_pointer,
        }
    }

    pub(crate) fn layer_index(&self) -> usize {
        self.targets[0].layer_index
    }

    pub(crate) fn primary(&self) -> &TransformTarget {
        &self.targets[0]
    }

    /// The frame's centre *on the board*. `pivot` is the raw content centre, which `forward`
    /// rotates and scales about before it translates — so once a layer has been moved, the
    /// box the user is dragging is no longer centred there. Rotation and corner scale both
    /// have to measure from what is on screen, or a moved layer turns about a point off in
    /// space and its corners jump the moment they are grabbed.
    pub(crate) fn center(&self) -> (f32, f32) {
        let target = self.primary();
        (
            target.pivot.0 + target.start_transform.offset_x,
            target.pivot.1 + target.start_transform.offset_y,
        )
    }
}

pub type TransformHandles = (usize, [(f32, f32); 4], (f32, f32));

pub(crate) const HANDLE_HIT_RADIUS_PX: f32 = 10.0;

pub(super) const ROTATE_HANDLE_OFFSET_PX: f32 = 24.0;

pub(super) const ROTATE_SNAP_STEP: f32 = std::f32::consts::FRAC_PI_4;

pub(crate) fn point_dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// A direction of length one, or `None` when there is no direction to take — a degenerate
/// edge has to be caught by the caller, not normalised into a random bearing.
pub(super) fn unit(v: (f32, f32)) -> Option<(f32, f32)> {
    let len = (v.0 * v.0 + v.1 * v.1).sqrt();
    (len > 1e-6).then(|| (v.0 / len, v.1 / len))
}

pub(super) fn angle_from(pivot: (f32, f32), p: (f32, f32)) -> f32 {
    (p.1 - pivot.1).atan2(p.0 - pivot.0)
}

pub(super) fn point_in_quad(p: (f32, f32), quad: [(f32, f32); 4]) -> bool {
    let mut sign = 0.0f32;
    for i in 0..4 {
        let a = quad[i];
        let b = quad[(i + 1) % 4];
        let edge = (b.0 - a.0, b.1 - a.1);
        let to_p = (p.0 - a.0, p.1 - a.1);
        let cross = edge.0 * to_p.1 - edge.1 * to_p.0;
        if cross.abs() < 1e-6 {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    true
}

pub(super) fn union_aabb_after_delta(
    targets: &[TransformTarget],
    dx: f32,
    dy: f32,
) -> (f32, f32, f32, f32) {
    let mut union = (
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    );
    for target in targets {
        let mut next = target.start_transform;
        next.offset_x += dx;
        next.offset_y += dy;
        let aabb = next.transformed_aabb(target.raw_bounds);
        union.0 = union.0.min(aabb.0);
        union.1 = union.1.min(aabb.1);
        union.2 = union.2.max(aabb.2);
        union.3 = union.3.max(aabb.3);
    }
    union
}

impl Document {
    pub fn reset_layer_transform(&mut self, index: usize) {
        let Some(layer) = self.layers.get(index) else {
            return;
        };
        if self.clip_pair_locked(index) || layer.transform.is_none() {
            return;
        }
        self.record_layer_props_history(index);
        if let Some(layer) = self.layers.get_mut(index) {
            layer.transform = None;
        }
        self.schedule_clip_recalc_for_indices(&[index]);
    }

    pub fn layer_transform(&self, index: usize) -> LayerTransform {
        self.layers
            .get(index)
            .and_then(|l| l.transform)
            .unwrap_or_default()
    }

    pub fn enter_transform(&mut self) -> bool {
        if self.tool_blocked(Tool::Transform) {
            return false;
        }
        self.transform_active = true;
        true
    }

    pub fn exit_transform(&mut self) {
        self.transform_active = false;
        self.transform_drag = None;
        self.clear_vector_selection();
    }

    /// The rotate grip: always `ROTATE_HANDLE_OFFSET_PX` clear of the middle of the frame's
    /// top edge and square to it, at every rotation, scale and flip.
    ///
    /// It reads the drawn corners rather than the transform, because the transform's own
    /// centre is `pivot` *before* translation. Measuring the stalk from there made a moved
    /// layer's grip lean off along the top edge by the offset — the further the layer was
    /// dragged, the further the grip slid sideways and, at a large enough offset, right off
    /// the corner. `forward` scales on the box's own axes before it rotates, so the frame is
    /// always a rectangle and its top edge has a true normal; there is no shear to correct.
    pub(super) fn rotate_handle_position(corners: [(f32, f32); 4], zoom: f32) -> (f32, f32) {
        let [tl, tr, br, bl] = corners;
        let top_mid = ((tl.0 + tr.0) * 0.5, (tl.1 + tr.1) * 0.5);
        let center = ((tl.0 + br.0) * 0.5, (tl.1 + br.1) * 0.5);
        // A zero-width box has no top edge to stand square to, so fall back to the side it
        // does have; a box with neither keeps the grip above it rather than nowhere.
        let mut dir = unit((tr.1 - tl.1, tl.0 - tr.0))
            .or_else(|| unit((tl.0 - bl.0, tl.1 - bl.1)))
            .unwrap_or((0.0, -1.0));
        // Outward, never into the box: a vertical flip turns the local top edge into the
        // lower one on screen, and the normal has to turn with it.
        if dir.0 * (top_mid.0 - center.0) + dir.1 * (top_mid.1 - center.1) < 0.0 {
            dir = (-dir.0, -dir.1);
        }
        let reach = ROTATE_HANDLE_OFFSET_PX / zoom.max(1e-6);
        (top_mid.0 + dir.0 * reach, top_mid.1 + dir.1 * reach)
    }

    /// The whole-layer transform frame, or `None` when there is nothing to show one for.
    ///
    /// A selected vector item takes the frame over: its own corners are drawn and hit-tested
    /// in place of the layer's, so both cannot be on screen at once. Clicking off the item
    /// drops the selection and hands the frame back to the layer.
    pub fn transform_handles(&self) -> Option<TransformHandles> {
        if !self.transform_active || self.selected_vector_item().is_some() {
            return None;
        }
        let index = self.active_layer;
        let layer = self.layers.get(index)?;
        let raw_bounds = layer.content_bounds()?;
        let pivot = bounds_center(raw_bounds);
        let t = layer.transform.unwrap_or_default();
        let corners = t.transformed_corners(pivot, raw_bounds);
        let rotate_handle = Self::rotate_handle_position(corners, self.camera.zoom);
        Some((index, corners, rotate_handle))
    }

    pub(super) fn transform_handle_at(&self, doc_x: f32, doc_y: f32) -> Option<TransformDrag> {
        let index = self.active_layer;
        let layer = self.layers.get(index)?;
        let raw_bounds = layer.content_bounds()?;
        let pivot = bounds_center(raw_bounds);
        let t = layer.transform.unwrap_or_default();
        let corners = t.transformed_corners(pivot, raw_bounds);
        let zoom = self.camera.zoom.max(1e-6);
        let hit_r = HANDLE_HIT_RADIUS_PX / zoom;
        let point = (doc_x, doc_y);
        // Scale and rotate belong to whatever the frame is around, and while an item is
        // selected that is the item — so only the Move quad answers here, which is what still
        // lets a click inside the box drop the item selection and take the layer.
        if self.selected_vector_item().is_none() {
            for (corner, handle) in corners.iter().zip(TransformHandle::CORNERS) {
                if point_dist(*corner, point) <= hit_r {
                    return Some(TransformDrag::single(
                        index, handle, pivot, raw_bounds, t, point,
                    ));
                }
            }
            let rotate_handle = Self::rotate_handle_position(corners, zoom);
            if point_dist(rotate_handle, point) <= hit_r {
                return Some(TransformDrag::single(
                    index,
                    TransformHandle::Rotate,
                    pivot,
                    raw_bounds,
                    t,
                    point,
                ));
            }
        }
        if point_in_quad(point, corners) {
            return Some(TransformDrag::single(
                index,
                TransformHandle::Move,
                pivot,
                raw_bounds,
                t,
                point,
            ));
        }
        None
    }

    pub(super) fn begin_transform_drag(&mut self, doc_x: f32, doc_y: f32) -> bool {
        self.transform_drag = self.transform_handle_at(doc_x, doc_y);
        self.transform_drag.is_some()
    }

    pub(super) fn retarget_transform(&mut self, doc_x: f32, doc_y: f32) -> bool {
        if self.pick_layer_for_move(doc_x, doc_y).is_none() {
            self.note_locked_pick_for_move(doc_x, doc_y);
            return false;
        }
        self.begin_transform_drag(doc_x, doc_y);
        true
    }

    /// The transform box is `content_bounds()` — tight to what the layer actually shows, mask
    /// included. Inside that box a click always keeps the active layer and may start a move
    /// drag even on transparent pixels; outside it the stack is offered the click first.
    ///
    /// A vector item under the cursor outranks the whole-layer Move handle — the layer is
    /// the item, so a click on it starts an item drag, and the corner and rotate handles
    /// (checked first) are still how the whole layer is scaled or turned.
    pub(super) fn transform_pointer_down(&mut self, doc_x: f32, doc_y: f32) {
        let handle = self.transform_handle_at(doc_x, doc_y);
        if let Some(drag) = handle
            .as_ref()
            .filter(|d| d.handle != TransformHandle::Move)
            .cloned()
        {
            self.clear_vector_selection();
            self.transform_drag = Some(drag);
            return;
        }
        if self.begin_vector_item_drag(doc_x, doc_y) {
            return;
        }
        self.clear_vector_selection();
        if let Some(drag) = handle {
            if self
                .layers
                .get(self.active_layer)
                .is_some_and(|layer| layer.visible)
            {
                self.transform_drag = Some(drag);
                return;
            }
        }
        if self.retarget_transform(doc_x, doc_y) {
            return;
        }
        if self.tool == Tool::Move {
            self.note_locked_pick_for_move(doc_x, doc_y);
            return;
        }
        self.exit_transform();
    }

    pub(crate) fn update_transform_drag(&mut self, doc_x: f32, doc_y: f32) {
        let Some(drag) = self.transform_drag.clone() else {
            return;
        };
        let (doc_x, doc_y) = match drag.handle {
            TransformHandle::Move | TransformHandle::Rotate => (doc_x, doc_y),
            _ => self.snap_doc_point((doc_x, doc_y)),
        };
        match drag.handle {
            TransformHandle::Move => {
                let dx = doc_x - drag.start_pointer.0;
                let dy = doc_y - drag.start_pointer.1;
                let union = union_aabb_after_delta(&drag.targets, dx, dy);
                let (snap_x, snap_y) = self.snap_box_offset(union);
                for target in &drag.targets {
                    let mut next = target.start_transform;
                    next.offset_x += dx + snap_x;
                    next.offset_y += dy + snap_y;
                    let next = next.clamped();
                    if let Some(layer) = self.layers.get_mut(target.layer_index) {
                        layer.transform = Some(next);
                    }
                }
            }
            TransformHandle::Rotate => {
                let target = drag.primary();
                let mut next = target.start_transform;
                let center = drag.center();
                let start_angle = angle_from(center, drag.start_pointer);
                let now_angle = angle_from(center, (doc_x, doc_y));
                let rotation = target.start_transform.rotation + (now_angle - start_angle);
                next.rotation = if self.shift_held {
                    (rotation / ROTATE_SNAP_STEP).round() * ROTATE_SNAP_STEP
                } else {
                    rotation
                };
                let next = next.clamped();
                if let Some(layer) = self.layers.get_mut(target.layer_index) {
                    layer.transform = Some(next);
                }
            }
            corner => {
                let target = drag.primary();
                let Some(signs) = corner.corner_signs() else {
                    return;
                };
                let half = (
                    (target.raw_bounds.2 - target.raw_bounds.0) * 0.5,
                    (target.raw_bounds.3 - target.raw_bounds.1) * 0.5,
                );
                let reach = target
                    .start_transform
                    .to_local(drag.center(), (doc_x, doc_y));
                let (scale_x, scale_y) = corner_scale(half, signs, reach, !self.shift_held);
                let mut next = target.start_transform;
                next.scale_x = scale_x;
                next.scale_y = scale_y;
                let next = next.clamped();
                if let Some(layer) = self.layers.get_mut(target.layer_index) {
                    layer.transform = Some(next);
                }
            }
        }
        let indices: Vec<usize> = drag.targets.iter().map(|t| t.layer_index).collect();
        self.schedule_clip_recalc_for_indices(&indices);
    }

    /// A layer's box in document space, tight to what is actually painted — the same rectangle
    /// the frame on the board draws, because both read `content_bounds`.
    pub fn layer_bounds(&self, index: usize) -> Option<(f32, f32, f32, f32)> {
        let layer = self.layers.get(index)?;
        let raw = layer.content_bounds()?;
        let t = layer.transform.unwrap_or_default();
        Some(t.transformed_aabb(raw))
    }

    /// Moves a layer so its box starts at `(x, y)`, and crops it to `width` × `height`.
    ///
    /// Size only ever **crops**. A size larger than the layer already is gets clamped rather
    /// than scaling the content up: there are no pixels to invent, and a number field that
    /// quietly resampled a layer would destroy detail on a typo. Scaling up is what the
    /// Transform tool is for.
    ///
    /// Position is a transform offset, so moving is non-destructive and undoes cleanly — the
    /// same thing the Move tool writes. Cropping is not: it discards pixels outside the box.
    /// A layer carrying a scale or rotation is moved but **not** cropped, since the crop
    /// rectangle would have to be resolved in the layer's own frame rather than the
    /// document's; the caller can see that from the bounds it reads back.
    pub fn set_layer_bounds(&mut self, index: usize, x: f32, y: f32, w: f32, h: f32) -> bool {
        if self.clip_pair_locked(index) {
            return false;
        }
        let Some((cur_x, cur_y, cur_x1, cur_y1)) = self.layer_bounds(index) else {
            return false;
        };
        let Some(layer) = self.layers.get_mut(index) else {
            return false;
        };
        let mut t = layer.transform.unwrap_or_default();
        t.offset_x += x - cur_x;
        t.offset_y += y - cur_y;
        layer.transform = (!t.is_identity()).then_some(t);

        let crop_w = w.max(1.0).min(cur_x1 - cur_x);
        let crop_h = h.max(1.0).min(cur_y1 - cur_y);
        let shrinks = crop_w < cur_x1 - cur_x || crop_h < cur_y1 - cur_y;
        let square = t.scale_x == 1.0 && t.scale_y == 1.0 && t.rotation == 0.0;
        if shrinks && square {
            let keep = DocRect::from_floats(
                x - t.offset_x,
                y - t.offset_y,
                x - t.offset_x + crop_w - 1.0,
                y - t.offset_y + crop_h - 1.0,
            );
            if let Some(grid) = layer.tiles_mut() {
                for band in outside_bands(grid.bounds(), keep) {
                    grid.paint_rect(band, |_, _, _| Some([0, 0, 0, 0]));
                }
            }
        }
        self.schedule_clip_recalc_for_indices(&[index]);
        true
    }

    pub fn layer_highlight(&self) -> Option<(usize, [(f32, f32); 4])> {
        if let Some(drag) = &self.transform_drag {
            if drag.handle != TransformHandle::Move {
                return None;
            }
            let corners = self.layer_outline_corners(drag.layer_index())?;
            return Some((drag.layer_index(), corners));
        }
        let index = self.hover_layer?;
        let corners = self.layer_outline_corners(index)?;
        Some((index, corners))
    }

    pub fn layer_highlights(&self) -> Vec<(usize, [(f32, f32); 4])> {
        if let Some(drag) = &self.transform_drag {
            if drag.handle == TransformHandle::Move {
                return drag
                    .targets
                    .iter()
                    .filter_map(|target| {
                        self.layer_outline_corners(target.layer_index)
                            .map(|corners| (target.layer_index, corners))
                    })
                    .collect();
            }
            if let Some(corners) = self.layer_outline_corners(drag.layer_index()) {
                return vec![(drag.layer_index(), corners)];
            }
            return Vec::new();
        }
        let mut out: Vec<_> = self
            .dragged_vector_layer()
            .into_iter()
            .filter_map(|index| {
                self.layer_outline_corners(index)
                    .map(|corners| (index, corners))
            })
            .collect();
        if let Some((index, corners)) = self.layer_highlight() {
            if !out.iter().any(|&(outlined, _)| outlined == index) {
                out.push((index, corners));
            }
        }
        out
    }

    /// The vector layer a plain Move drag is carrying. A layer drag already answers through
    /// `transform_drag` above; a vector drags its item instead, so it needs this to get the same
    /// dashed outline. Only while the pointer is down — at rest Move outlines nothing, so letting
    /// go never leaves a frame behind that reads as waiting to be committed. `⌘T` draws its own
    /// frame, and every other tool leaves the stack alone, so both answer nothing.
    pub(super) fn dragged_vector_layer(&self) -> Option<usize> {
        if self.tool != Tool::Move || self.transform_active {
            return None;
        }
        self.vector_drag.as_ref().map(|drag| drag.pick.layer)
    }

    pub(super) fn layer_outline_corners(&self, index: usize) -> Option<[(f32, f32); 4]> {
        let layer = self.layers.get(index)?;
        if layer.is_paper() {
            return None;
        }
        let raw_bounds = layer.content_bounds()?;
        let pivot = bounds_center(raw_bounds);
        let t = layer.transform.unwrap_or_default();
        Some(t.transformed_corners(pivot, raw_bounds))
    }
}
