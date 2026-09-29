//! The layers panel and the layer settings card: picking, visibility, ordering, clipping and
//! masks, per-layer properties, and the bounds readout.

use super::*;

impl AppController {
    pub fn pick_layer(&mut self, index: usize) {
        let mut engine = self.engine.borrow_mut();
        engine.set_active_layer(index);
        engine.set_layer_selection(&[]);
        drop(engine);
        self.retarget_layer_settings(index);
    }

    pub fn toggle_layer_selected(&mut self, index: usize) {
        let mut engine = self.engine.borrow_mut();
        let mut selection = engine.layer_selection();
        if selection.is_empty() {
            selection.extend(engine.active_layer_index());
        }
        let added = match selection.iter().position(|&i| i == index) {
            Some(at) => {
                selection.remove(at);
                false
            }
            None => {
                selection.push(index);
                engine.set_active_layer(index);
                true
            }
        };
        engine.set_layer_selection(&selection);
        drop(engine);
        if added {
            self.retarget_layer_settings(index);
        }
    }

    pub fn retarget_layer_settings_to_active(&mut self) {
        if !self.layer_settings_open {
            return;
        }
        if let Some(index) = self.engine.borrow().active_layer_index() {
            self.layer_settings_index = index;
        }
    }

    pub(super) fn retarget_layer_settings(&mut self, index: usize) {
        if self.layer_settings_open {
            self.layer_settings_index = index;
        }
    }

    pub fn add_layer(&mut self) {
        self.engine.borrow_mut().add_layer();
    }

    pub fn layer_settings_summary(&self) -> Option<LayerSummary> {
        let engine = self.engine.borrow();
        engine
            .list_layers()
            .into_iter()
            .find(|layer| layer.index == self.layer_settings_index)
    }

    pub fn open_layer_settings(&mut self, index: usize, _anchor_x: f32, _anchor_y: f32) {
        self.layer_settings_index = index;
        self.layer_settings_open = true;
    }

    pub fn toggle_layer_visible(&mut self, index: usize) {
        let visible = self
            .engine
            .borrow()
            .list_layers()
            .into_iter()
            .find(|layer| layer.index == index)
            .map(|layer| layer.visible)
            .unwrap_or(true);
        self.engine.borrow_mut().set_layer_visible(index, !visible);
    }

    pub fn set_layer_visible(&mut self, index: usize, visible: bool) {
        self.engine.borrow_mut().set_layer_visible(index, visible);
    }

    pub fn set_layer_locked(&mut self, index: usize, locked: bool) {
        self.engine.borrow_mut().set_layer_locked(index, locked);
    }

    pub fn set_layer_opacity(&mut self, index: usize, opacity: f32) {
        self.engine.borrow_mut().set_layer_opacity(index, opacity);
    }

    pub fn set_layer_blend_mode(&mut self, index: usize, mode: i32) {
        if let Some(mode) = BlendMode::from_u32(mode as u32) {
            self.engine.borrow_mut().set_layer_blend_mode(index, mode);
        }
    }

    pub fn set_layer_filter(&mut self, index: usize, kind: i32, value: f32) {
        let mut adjustments = self.engine.borrow().layer_adjustments(index);
        match kind {
            0 => adjustments.brightness = value,
            1 => adjustments.contrast = value,
            2 => adjustments.vibrance = value,
            3 => adjustments.saturation = value,
            4 => adjustments.levels_gamma = value,
            5 => adjustments.hue = value,
            _ => return,
        }
        self.engine
            .borrow_mut()
            .set_layer_adjustments(index, adjustments);
    }

    pub fn reset_layer_filters(&mut self, index: usize) {
        self.engine.borrow_mut().reset_layer_adjustments(index);
    }

    pub fn rename_layer(&mut self, index: usize, name: &str) -> bool {
        self.engine.borrow_mut().set_layer_name(index, name)
    }

    pub fn toggle_layer_clip(&mut self, index: usize) -> bool {
        let mut engine = self.engine.borrow_mut();
        if engine.is_layer_clipped(index) {
            engine.release_clipping_mask(index)
        } else {
            engine.create_clipping_mask(index)
        }
    }

    pub fn toggle_layer_mask(&mut self, index: usize) -> bool {
        let mut engine = self.engine.borrow_mut();
        if engine.is_layer_masked(index) {
            let ok = engine.release_layer_mask(index);
            drop(engine);
            if ok {
                self.layer_settings_index = index.saturating_sub(1);
            }
            ok
        } else {
            let ok = engine.create_layer_mask(index);
            drop(engine);
            if ok {
                self.layer_settings_index = index + 1;
            }
            ok
        }
    }

    pub fn flatten_layer_clip(&mut self, index: usize) -> bool {
        self.engine.borrow_mut().flatten_clip(index)
    }

    pub fn merge_layer_down(&mut self, index: usize) -> bool {
        self.engine.borrow_mut().merge_layer_down(index)
    }

    pub fn reset_layer_transform(&mut self, index: usize) {
        self.engine.borrow_mut().reset_layer_transform(index);
    }

    pub fn move_layer_up(&mut self, index: usize) -> bool {
        if !self.engine.borrow_mut().move_layer_up(index) {
            return false;
        }
        self.layer_settings_index = index + 1;
        true
    }

    pub fn move_layer_down(&mut self, index: usize) -> bool {
        if !self.engine.borrow_mut().move_layer_down(index) {
            return false;
        }
        self.layer_settings_index = index.saturating_sub(1);
        true
    }

    pub fn move_layer_row(&mut self, from_row: usize, to_row: usize) -> bool {
        let count = self.engine.borrow().layer_count() as usize;
        if !self.engine.borrow_mut().move_layer_row(from_row, to_row) {
            return false;
        }
        let from = count - 1 - from_row;
        let to = count - 1 - to_row;
        let index = self.layer_settings_index;
        self.layer_settings_index = if index == from {
            to
        } else if from < to && index > from && index <= to {
            index - 1
        } else if from > to && index >= to && index < from {
            index + 1
        } else {
            index
        };
        self.thumb_cache.invalidate();
        true
    }

    pub fn rasterize_layer(&mut self, index: usize) -> bool {
        self.engine.borrow_mut().rasterize_layer(index)
    }

    pub fn duplicate_layer(&mut self, index: usize) -> bool {
        self.engine.borrow_mut().duplicate_layer(index)
    }

    pub fn remove_layer(&mut self, index: usize) -> bool {
        self.engine.borrow_mut().remove_layer(index)
    }

    pub fn set_layer_hover(&mut self, index: usize) {
        self.layer_hover_index = Some(index);
        self.engine.borrow_mut().set_hover_layer(Some(index));
    }

    pub fn clear_layer_hover(&mut self) {
        self.layer_hover_index = None;
        self.engine.borrow_mut().set_hover_layer(None);
    }

    pub fn commit_layer_bounds(&mut self, x: &str, y: &str, w: &str, h: &str) {
        let parse = |text: &str| text.trim().parse::<f32>().ok();
        let (Some(x), Some(y), Some(w), Some(h)) = (parse(x), parse(y), parse(w), parse(h)) else {
            return;
        };
        let Some(index) = self.engine.borrow().active_layer_index() else {
            return;
        };
        self.engine.borrow_mut().set_layer_bounds(index, x, y, w, h);
    }
}
