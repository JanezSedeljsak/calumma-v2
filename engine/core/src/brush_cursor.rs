//! Where the brush actually is, so the board can draw it.
//!
//! Brush size is a slider number until you can see it on the paper. The ring is drawn from the
//! same two values the stamp is — `brush_size` and the pointer — so it cannot disagree with
//! what the next stroke will lay down, and it is refused in exactly the cases a stroke would
//! be: `Document::tool_block` says no, or `⌘T` owns the pointer.

use crate::document::Document;
use crate::shape::Tool;

impl Document {
    /// The pointer, in screen coordinates, the same units `pointer_down` takes — the shell
    /// never converts anything into document space itself.
    pub fn set_pointer_hover(&mut self, screen_x: f32, screen_y: f32) {
        self.pointer_hover = Some(self.camera.to_doc(screen_x, screen_y));
    }

    pub fn clear_pointer_hover(&mut self) {
        self.pointer_hover = None;
    }

    /// Centre and radius of the brush cursor in **document** units, or `None` when nothing
    /// should be drawn. Document units because the ring has to scale with the zoom exactly as
    /// the stamp does: a 200px brush at 10% zoom is a small circle, not a large one.
    pub fn brush_ring(&self) -> Option<((f32, f32), f32)> {
        if !matches!(self.tool, Tool::Pen | Tool::Eraser | Tool::Blur) {
            return None;
        }
        if self.transform_active {
            return None;
        }
        // The same predicate the press itself will run, so the ring never promises a stroke
        // the engine then refuses. A vector-mode pen draws into a layer of its own, which is
        // why `tool_block` — not the active layer's content — is what decides.
        if self.tool_blocked(self.tool) {
            return None;
        }
        let centre = self.pointer_hover?;
        let radius = self.brush_size * 0.5;
        (radius > 0.0).then_some((centre, radius))
    }
}
