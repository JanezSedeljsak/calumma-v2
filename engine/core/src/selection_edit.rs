//! The selection commands that are not a pointer drag: deselect, select all, invert, and the
//! colour-range re-run a knob change asks for.
//!
//! The tools build a selection from a gesture (`document.rs`'s `commit_selection_shape`,
//! `commit_lasso_selection`, `commit_magic_wand`). These three answer a menu item instead,
//! and they are the only ones that have to reason about the *document's* extent rather than
//! about where the pointer went.

use crate::document::Document;
use crate::selection::{Selection, SelectionShape};
use crate::selection_mask::SelectionMask;
use crate::shape::Tool;

impl Document {
    pub fn deselect(&mut self) {
        self.exit_transform();
        self.commit_text();
        self.selection = None;
    }

    /// The whole canvas, as one rect — Photoshop's Select All, not the active layer's painted
    /// bounds. It is also exactly what dragging the Rect select tool edge to edge already
    /// produces, so no new shape variant is involved.
    /// While a text session is open this means the *text*, not the canvas — one shortcut, and
    /// it always selects whatever is in front of you (`Document::text_select_all`).
    pub fn select_all(&mut self) {
        if self.text_select_all() {
            return;
        }
        self.commit_text();
        self.selection = Some(Selection {
            shape: SelectionShape::Rect {
                start: (0.0, 0.0),
                end: (self.width as f32, self.height as f32),
            },
        });
    }

    /// Everything the current selection leaves out, clipped to the canvas.
    ///
    /// Rect, Ellipse and Lasso answer `contains` from a formula, so there is no buffer to
    /// flip: the inverse of any of them is a `Mask`, the same shape the magic wand already
    /// produces, filled wherever `contains` was false and cropped at `finish`. Inverting
    /// nothing selects everything, which is what Photoshop does with an empty selection.
    ///
    /// Inverting a full-canvas selection reaches no pixel at all, and `finish` answers that
    /// with `None` — the same deliberate rule the wand follows for a click that reaches
    /// nothing, because an empty-but-present selection would silently clip every later
    /// stroke to nothing.
    pub fn invert_selection(&mut self) {
        self.commit_text();
        let Some(current) = self.selection.take() else {
            self.select_all();
            return;
        };
        let bounds = self.bounds();
        let mask = SelectionMask::from_predicate(
            (bounds.min_x, bounds.min_y),
            self.width,
            self.height,
            |x, y| !current.contains(x as f32 + 0.5, y as f32 + 0.5),
        );
        self.selection = mask.finish().map(|mask| Selection {
            shape: SelectionShape::Mask(mask),
        });
    }

    /// Select-by-colour against the match swatch, over the active layer's painted box.
    ///
    /// One walk, `SelectionMask::from_predicate`, which is a rayon task per row — the same
    /// traversal Invert uses, and for the same reason: unlike a flood, this asks about every
    /// pixel rather than following a blob. The scope is the layer's ink, not the canvas, so
    /// Paper white never floods through the empty tiles of the layer above it.
    pub(crate) fn apply_color_range(&mut self) -> bool {
        let doc_bounds = self.bounds();
        let target = self.select_color;
        let tolerance = self.tolerance;
        let Some(layer) = self.layers.get(self.active_layer) else {
            return false;
        };
        let Some(sample) = crate::select_sample::LayerSelectSample::new(layer, doc_bounds) else {
            return false;
        };
        let scope = sample.scope;
        self.selection =
            crate::fill::color_range_pixels(scope, target, tolerance, |x, y| sample.pixel(x, y))
                .map(|mask| Selection {
                    shape: SelectionShape::Mask(mask),
                });
        true
    }

    /// Re-runs the colour range after the match swatch or the tolerance moved. Scoped to the
    /// tool actually being in hand, so turning the same tolerance knob for the bucket or the
    /// wand never rebuilds a selection nobody asked about.
    pub(crate) fn reselect_color(&mut self) {
        if self.tool != Tool::SelectColor || self.tool_blocked(Tool::SelectColor) {
            return;
        }
        self.apply_color_range();
    }
}
