//! Where the brush actually is, so the board can draw it.
//!
//! Brush size is a slider number until you can see it on the paper. The ring is drawn from the
//! same two values the stamp is — `brush_size` and the pointer — so it cannot disagree with
//! what the next stroke will lay down, and it is refused in exactly the cases a stroke would
//! be: `Document::tool_block` says no, or `⌘T` owns the pointer.

use crate::document::Document;
use crate::limits::{BRUSH_MIN_SCREEN_PX, BRUSH_SIZE_MAX};
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
        if !matches!(
            self.tool,
            Tool::Pen | Tool::Eraser | Tool::Blur | Tool::Clone | Tool::Heal
        ) {
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
        // A ring is a promise that a stamp lands here, so it stops where the stamps do. Off the
        // end of the layer there is nothing to paint, and the shell — which hides its own cursor
        // for exactly as long as there is a ring — needs to be told so, or the pointer vanishes
        // over the desk.
        if !self.brush_reaches(centre) {
            return None;
        }
        let radius = self.effective_brush_size() * 0.5;
        (radius > 0.0).then_some((centre, radius))
    }

    /// Whether a stamp at this document point would land on the active layer at all. That is the
    /// layer's own **extent**, not the paper: a pasted image reaches past the document, and the
    /// part hanging off it takes paint like the rest of it. Mapped through the layer's transform
    /// for the same reason the commit is — the extent is measured in the layer's grid.
    pub(crate) fn brush_reaches(&self, doc_point: (f32, f32)) -> bool {
        // A vector stroke commits into a layer of its own and has no tile grid to fall off, so
        // there is no extent to be outside of.
        if self.effective_vector_mode() {
            return true;
        }
        let Some(layer) = self.layers.get(self.active_layer) else {
            return false;
        };
        let Some(grid) = layer.tiles() else {
            return false;
        };
        let (gx, gy) = layer.doc_point_to_grid(doc_point);
        grid.extent().contains(gx.floor() as i32, gy.floor() as i32)
    }

    /// The brush as it is actually laid down, which is not always the number on the slider.
    ///
    /// A size is chosen in document pixels, and at a low enough zoom every document size
    /// disappears — an 8px brush on a 4096px board fitted to a laptop is about one screen pixel,
    /// too fine to see and so too fine to aim. So the brush carries a second floor measured in
    /// *screen* pixels (`BRUSH_MIN_SCREEN_PX`), which costs more document pixels the further out
    /// the board is zoomed and nothing at all once it is zoomed in. Zooming in never takes the
    /// brush below `BRUSH_SIZE_MIN`: the floor is on what can be seen, and zoomed in it can be.
    ///
    /// Everything that draws the brush reads this and not `brush_size` — the ring, the GPU
    /// preview and the commit alike — because the moment two of them disagree the stroke moves
    /// when the preview hands over.
    ///
    /// Vector mode is exempt: its width is stored in the item and redrawn at every zoom, so
    /// folding today's camera into it would bake the zoom into the document.
    pub fn effective_brush_size(&self) -> f32 {
        // A brush with no size is not a small brush, it is no brush — the floor lifts a chosen
        // size into view, it does not invent one. `brush_ring` reads the `> 0.0` that follows
        // from this, and the size sliders never offer it.
        if self.brush_size <= 0.0 {
            return self.brush_size;
        }
        if self.effective_vector_mode() {
            return self.brush_size;
        }
        let screen_floor = BRUSH_MIN_SCREEN_PX / self.camera.zoom.max(f32::MIN_POSITIVE);
        self.brush_size.max(screen_floor).min(BRUSH_SIZE_MAX)
    }
}
