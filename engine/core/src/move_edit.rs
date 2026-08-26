use crate::document::{Document, TransformDrag};
use crate::limits::LAYER_NUDGE_STEP;
use crate::shape::Tool;
use crate::transform::bounds_center;

impl Document {
    pub fn set_tool(&mut self, next: Tool) -> bool {
        if next != Tool::Text {
            self.commit_text();
        }
        if next == Tool::Transform {
            return self.toggle_transform();
        }
        if next != Tool::Move {
            self.exit_transform();
        }
        if next.is_shape() {
            self.last_shape_tool = next;
        }
        if next.is_selection() {
            self.last_select_tool = next;
        }
        self.tool = next;
        true
    }

    pub fn begin_move_at(&mut self, doc_x: f32, doc_y: f32) -> bool {
        if self.begin_vector_item_drag(doc_x, doc_y) {
            return true;
        }
        self.clear_vector_selection();
        let Some(index) = self.layer_at_for_move(doc_x, doc_y) else {
            self.note_locked_pick_for_move(doc_x, doc_y);
            return false;
        };
        self.active_layer = index;
        self.begin_layer_move(index, doc_x, doc_y)
    }

    pub fn update_move_drag(&mut self, doc_x: f32, doc_y: f32) -> bool {
        if self.update_vector_item_drag(doc_x, doc_y) {
            return true;
        }
        if self.transform_drag.is_none() {
            return false;
        }
        self.update_transform_drag(doc_x, doc_y);
        true
    }

    pub fn end_move_drag(&mut self) -> bool {
        let vector = self.end_vector_item_drag();
        let layer = self.transform_drag.take().is_some();
        vector || layer
    }

    pub fn nudge_move_target(&mut self, steps_x: f32, steps_y: f32) -> bool {
        if self.nudge_selected_vector_item(steps_x, steps_y) {
            return true;
        }
        if self.tool != Tool::Move && !self.transform_active {
            return false;
        }
        self.nudge_active_layer(steps_x, steps_y)
    }

    fn begin_layer_move(&mut self, index: usize, doc_x: f32, doc_y: f32) -> bool {
        let Some(layer) = self.layers.get(index) else {
            return false;
        };
        if layer.is_paper() || layer.locked {
            return false;
        }
        let Some(raw_bounds) = layer.content_bounds() else {
            return false;
        };
        let pivot = bounds_center(raw_bounds);
        let t = layer.transform.unwrap_or_default();
        self.transform_drag = Some(TransformDrag::layer_move(
            index,
            pivot,
            raw_bounds,
            t,
            (doc_x, doc_y),
        ));
        true
    }

    fn nudge_active_layer(&mut self, steps_x: f32, steps_y: f32) -> bool {
        let index = self.active_layer;
        let Some(layer) = self.layers.get_mut(index) else {
            return false;
        };
        if layer.is_paper() || layer.locked {
            return false;
        }
        if layer.content_bounds().is_none() {
            return false;
        }
        let mut t = layer.transform.unwrap_or_default();
        t.offset_x += steps_x * LAYER_NUDGE_STEP;
        t.offset_y += steps_y * LAYER_NUDGE_STEP;
        layer.transform = Some(t.clamped());
        true
    }
}
