//! The layer stack: adding, removing, reordering and the per-layer properties the layers
//! panel edits.

use super::*;

impl Document {
    pub fn add_layer(&mut self, name: impl Into<String>) {
        self.record_stack_history();
        self.layers.push(Layer::new(name, self.width, self.height));
        self.active_layer = self.layers.len() - 1;
    }

    pub(crate) fn push_layer(&mut self, name: impl Into<String>) {
        self.layers.push(Layer::new(name, self.width, self.height));
        self.active_layer = self.layers.len() - 1;
    }

    pub fn place_image(&mut self, rgba: &[u8], width: u32, height: u32) -> bool {
        self.place_image_at(rgba, width, height, 0, 0)
    }

    pub fn place_image_at(
        &mut self,
        rgba: &[u8],
        width: u32,
        height: u32,
        ox: i32,
        oy: i32,
    ) -> bool {
        if !self.active_layer_accepts_paint() {
            return false;
        }
        let expected = (width as usize) * (height as usize) * 4;
        if width == 0 || height == 0 || rgba.len() < expected {
            return false;
        }
        let Some(tiles) = self.active_mut().and_then(|layer| layer.tiles_mut()) else {
            return false;
        };
        let placed = DocRect::new(ox, oy, ox + width as i32 - 1, oy + height as i32 - 1);
        tiles.grow_extent(placed);
        tiles.blit_rgba_at(rgba, width, height, ox, oy) > 0
    }

    pub fn remove_layer(&mut self, index: usize) -> bool {
        self.remove_layer_inner(index, true)
    }

    pub(crate) fn remove_layer_inner(&mut self, index: usize, record: bool) -> bool {
        if index >= self.layers.len() {
            return false;
        }
        self.commit_text();
        self.clear_vector_selection();
        if record {
            self.record_stack_history();
        }
        self.layers.remove(index);
        if self.layers.is_empty() {
            self.active_layer = 0;
            self.hover_layer = None;
            return true;
        }
        if self.active_layer > index {
            self.active_layer -= 1;
        } else if self.active_layer >= self.layers.len() {
            self.active_layer = self.layers.len() - 1;
        }
        if let Some(hover) = self.hover_layer {
            if hover == index {
                self.hover_layer = None;
            } else if hover > index {
                self.hover_layer = Some(hover - 1);
            }
        }
        self.layer_selection.retain(|&i| i != index);
        for selected in &mut self.layer_selection {
            if *selected > index {
                *selected -= 1;
            }
        }
        self.validate_clip_links();
        true
    }

    pub fn set_layer_visible(&mut self, index: usize, visible: bool) {
        if let Some(layer) = self.layers.get_mut(index) {
            layer.visible = visible;
        }
        self.schedule_clip_recalc_for_indices(&[index]);
    }

    pub fn set_active_layer(&mut self, index: usize) {
        if index < self.layers.len() {
            if index != self.active_layer {
                self.commit_text();
                self.clear_vector_selection();
            }
            self.active_layer = index;
        }
    }

    /// No `mark_channel_dirty(Render)`: opacity is read by `fs_tile` off the `LayerData` row
    /// (`Renderer::write_layer_data`), not baked into tile bytes, so nothing about the tiles
    /// themselves went stale. The caller still owes the renderer an `invalidate()` — same as a
    /// `⌘T` drag, which rewrites the same row for the same reason.
    pub fn set_layer_opacity(&mut self, index: usize, opacity: f32) {
        let new_opacity = opacity.clamp(0.0, 1.0);
        let Some(layer) = self.layers.get(index) else {
            return;
        };
        if (layer.opacity - new_opacity).abs() < 1e-6 {
            return;
        }
        self.record_layer_props_history(index);
        if let Some(layer) = self.layers.get_mut(index) {
            layer.opacity = new_opacity;
        }
    }

    pub fn set_layer_blend_mode(&mut self, index: usize, mode: crate::layer::BlendMode) {
        let Some(layer) = self.layers.get(index) else {
            return;
        };
        if layer.blend_mode == mode {
            return;
        }
        self.record_layer_props_history(index);
        if let Some(layer) = self.layers.get_mut(index) {
            layer.blend_mode = mode;
        }
    }

    /// No `mark_channel_dirty(Render)`, for the same reason `set_layer_opacity` above dropped
    /// it: the adjustment LUT is evaluated per pixel in `fs_tile` off the `LayerData` row, so a
    /// slider drag never re-walks a single tile.
    pub fn set_layer_adjustments(
        &mut self,
        index: usize,
        adjustments: crate::filters::Adjustments,
    ) {
        let Some(layer) = self.layers.get(index) else {
            return;
        };
        let adjustments = adjustments.clamped();
        let next = if adjustments.is_neutral() {
            None
        } else {
            Some(adjustments)
        };
        if layer.adjustments == next {
            return;
        }
        self.record_layer_props_history(index);
        if let Some(layer) = self.layers.get_mut(index) {
            layer.adjustments = next;
        }
    }

    pub fn nudge_layer_adjustment(
        &mut self,
        index: usize,
        kind: crate::filters::AdjustmentKind,
        steps: f32,
    ) -> bool {
        let Some(layer) = self.layers.get(index) else {
            return false;
        };
        let current = layer.adjustments.unwrap_or_default();
        let next = current.nudged(kind, steps);
        if next == current {
            return false;
        }
        self.set_layer_adjustments(index, next);
        true
    }

    pub fn add_vector_layer(&mut self, name: impl Into<String>, item: vector::VectorItem) -> usize {
        self.layers.push(Layer::vector(name, item));
        self.active_layer = self.layers.len() - 1;
        self.bump_vector_revision();
        self.active_layer
    }

    pub(super) fn push_vector_item(&mut self, item: vector::VectorItem) {
        self.record_stack_history();
        let n = self.layers.iter().filter(|l| l.content.is_vector()).count() + 1;
        self.add_vector_layer(crate::names::numbered_vector_layer(n), item);
    }

    pub fn duplicate_layer(&mut self, index: usize) -> bool {
        if index >= self.layers.len() {
            return false;
        }
        self.commit_text();
        self.record_stack_history();
        let Some(source) = self.layers.get(index).cloned() else {
            return false;
        };
        let base_name = source.name.clone();
        let mut copy = source;
        copy.id = uuid::Uuid::new_v4().to_string();
        copy.name = crate::names::duplicate_layer_name(&base_name);
        copy.clips_to = None;
        copy.clip_invert = false;
        self.layers.insert(index + 1, copy);
        self.active_layer = index + 1;
        self.validate_clip_links();
        true
    }

    pub fn move_layer_up(&mut self, index: usize) -> bool {
        self.move_layer_by(index, 1)
    }

    pub fn move_layer_down(&mut self, index: usize) -> bool {
        self.move_layer_by(index, -1)
    }

    /// Move a layer to an arbitrary position in the stack, the drag-reorder counterpart to the
    /// single-step `move_layer_up` / `move_layer_down`. `to` is where the layer ends up in the
    /// finished stack, not an insertion point measured against the old one.
    ///
    /// Paper is pinned: it cannot be dragged, and nothing can be dropped beneath it. It is the
    /// board's backing sheet rather than a layer in the composition, and a stack with paint
    /// hidden under it would look like the paint had vanished.
    pub fn move_layer(&mut self, from: usize, to: usize) -> bool {
        let count = self.layers.len();
        if from >= count || to >= count || from == to {
            return false;
        }
        if let Some(paper) = self.layers.iter().position(Layer::is_paper) {
            if from == paper || to <= paper {
                return false;
            }
        }
        self.commit_text();
        self.record_stack_history();
        let layer = self.layers.remove(from);
        self.layers.insert(to, layer);
        let remap = |i: usize| {
            if i == from {
                to
            } else if from < to && i > from && i <= to {
                i - 1
            } else if from > to && i >= to && i < from {
                i + 1
            } else {
                i
            }
        };
        self.remap_layer_indices(remap);
        self.validate_clip_links();
        true
    }

    /// The same move stated in panel rows. The layers panel draws the stack top-first while
    /// the document stores it bottom-first, so the two orders are mirror images — the engine
    /// owns that flip so the shell can hand over the row it dragged and the row it dropped on
    /// without ever computing a stack index.
    pub fn move_layer_row(&mut self, from_row: usize, to_row: usize) -> bool {
        let count = self.layers.len();
        if from_row >= count || to_row >= count {
            return false;
        }
        self.move_layer(count - 1 - from_row, count - 1 - to_row)
    }

    /// Rename a layer, or refuse.
    ///
    /// Paper is name-matched (`Layer::is_paper`), so its name is load-bearing: merge-down and
    /// click-to-pick both key off it. Renaming Paper would quietly break both, and renaming *another* layer to `Paper` would quietly turn it into one — so
    /// both directions are refused. An all-whitespace name is refused too, since a row with no
    /// label is unusable.
    pub fn set_layer_name(&mut self, index: usize, name: &str) -> bool {
        let trimmed = name.trim();
        if trimmed.is_empty() || trimmed == crate::names::PAPER {
            return false;
        }
        let Some(layer) = self.layers.get_mut(index) else {
            return false;
        };
        if layer.is_paper() || layer.name == trimmed {
            return false;
        }
        layer.name = trimmed.to_string();
        true
    }

    pub fn set_layer_locked(&mut self, index: usize, locked: bool) -> bool {
        {
            let Some(layer) = self.layers.get_mut(index) else {
                return false;
            };
            if layer.locked == locked {
                return false;
            }
            layer.locked = locked;
        }
        if locked && self.clip_pair_locked(self.active_layer) {
            self.exit_transform();
        }
        true
    }

    /// Every index the document keeps into `layers`, moved through one mapping. Any stack
    /// mutation has to run all of them or something ends up pointing at the wrong layer.
    pub(super) fn remap_layer_indices(&mut self, remap: impl Fn(usize) -> usize + Copy) {
        self.active_layer = remap(self.active_layer);
        self.hover_layer = self.hover_layer.map(remap);
        if let Some(drag) = &mut self.transform_drag {
            for target in &mut drag.targets {
                target.layer_index = remap(target.layer_index);
            }
        }
        self.layer_selection = self
            .layer_selection
            .iter()
            .map(|&index| remap(index))
            .collect();
        if let Some(pick) = &mut self.selected_vector {
            pick.layer = remap(pick.layer);
        }
        if let Some(edit) = &mut self.text_edit {
            edit.layer = remap(edit.layer);
        }
    }

    pub(super) fn move_layer_by(&mut self, index: usize, delta: isize) -> bool {
        if index >= self.layers.len() {
            return false;
        }
        if self.layers[index].is_paper() {
            return false;
        }
        let other = match index.checked_add_signed(delta) {
            Some(other) if other < self.layers.len() => other,
            _ => return false,
        };
        if delta < 0 && self.layers[other].is_paper() {
            return false;
        }
        self.commit_text();
        self.record_stack_history();
        self.layers.swap(index, other);
        self.remap_layer_indices(|i| {
            if i == index {
                other
            } else if i == other {
                index
            } else {
                i
            }
        });
        true
    }
}
