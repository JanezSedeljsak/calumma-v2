//! Ruler guides: dragging one out of a ruler, and the guides card that lists and edits them.

use super::*;

impl AppController {
    pub fn begin_guide_drag(&mut self, horizontal: bool, x: f32, y: f32, shift: bool) {
        let axis = if horizontal {
            GuideAxis::Horizontal
        } else {
            GuideAxis::Vertical
        };
        let mut engine = self.engine.borrow_mut();
        engine.set_shift_held(shift);
        engine.begin_guide_drag_from_ruler(axis, x, y);
        self.guide_drag_pos = Some((x, y));
    }

    pub fn update_guide_drag(&mut self, x: f32, y: f32, shift: bool) {
        let mut engine = self.engine.borrow_mut();
        engine.set_shift_held(shift);
        engine.update_guide_drag(x, y);
        self.guide_drag_pos = Some((x, y));
    }

    pub fn end_guide_drag(&mut self) {
        self.engine.borrow_mut().end_guide_drag();
        self.guide_drag_pos = None;
    }

    pub fn refresh_guide_shift(&mut self, shift: bool) {
        let mut engine = self.engine.borrow_mut();
        engine.set_shift_held(shift);
        if let Some((x, y)) = self.guide_drag_pos {
            engine.update_guide_drag(x, y);
        }
    }

    pub fn open_guides(&mut self) {
        self.guides_open = true;
    }

    pub fn add_guide_from_card(&mut self, horizontal: bool, text: &str) {
        let Ok(position) = text.trim().parse::<f32>() else {
            return;
        };
        self.engine.borrow_mut().add_guide(horizontal, position);
    }

    pub fn remove_guide(&mut self, index: usize) {
        self.engine.borrow_mut().remove_guide(index);
    }

    pub fn clear_guides(&mut self) {
        self.engine.borrow_mut().clear_guides();
    }

    pub fn set_guide_axis(&mut self, index: usize, horizontal: bool) {
        self.engine.borrow_mut().set_guide_axis(index, horizontal);
    }

    pub fn set_guide_offset(&mut self, index: usize, text: &str) {
        let Ok(position) = text.trim().parse::<f32>() else {
            return;
        };
        self.engine.borrow_mut().set_guide_position(index, position);
    }

    pub fn set_guide_color(&mut self, index: usize, palette_index: usize) {
        self.engine
            .borrow_mut()
            .set_guide_color(index, palette_index);
    }
}
