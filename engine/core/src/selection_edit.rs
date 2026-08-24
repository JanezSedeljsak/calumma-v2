//! The selection commands that are not a pointer drag: deselect, select all, invert.
//!
//! The tools build a selection from a gesture (`document.rs`'s `commit_selection_shape`,
//! `commit_lasso_selection`, `commit_magic_wand`). These three answer a menu item instead,
//! and they are the only ones that have to reason about the *document's* extent rather than
//! about where the pointer went.

use crate::document::Document;
use crate::selection::{Selection, SelectionShape};
use crate::selection_mask::SelectionMask;

impl Document {
    pub fn deselect(&mut self) {
        self.exit_transform();
        self.commit_text();
        self.selection = None;
    }

    /// The whole canvas, as one rect — Photoshop's Select All, not the active layer's painted
    /// bounds. It is also exactly what dragging the Rect select tool edge to edge already
    /// produces, so no new shape variant is involved.
    pub fn select_all(&mut self) {
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
}
