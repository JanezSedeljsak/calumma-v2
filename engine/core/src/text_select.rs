//! Selection and caret placement in a live text session, and the geometry the board draws for
//! one.
//!
//! `text_edit.rs` owns the session and the edits that change the string; everything here
//! answers *where* — where the caret is, what is selected, and what rectangles the overlay
//! fills. Every answer comes from the shaped layout (`calumma_text`), never from the string.

use crate::document::Document;
use crate::layer::Layer;
use crate::text_edit::TextRange;
use crate::text_layer;
use calumma_text::{
    caret_rect, index_at_point, paragraph_range, selection_rects, step_index, word_range,
    SelectionRect, Step,
};

impl Document {
    pub fn text_caret(&self) -> Option<usize> {
        self.text_edit.as_ref().map(|e| e.caret)
    }

    pub fn text_range(&self) -> Option<TextRange> {
        self.text_edit.as_ref().map(|e| TextRange {
            caret: e.caret,
            anchor: e.anchor,
        })
    }

    /// The selected byte range, or `None` when the caret stands alone. Every command that has
    /// to ask "is something selected" asks this rather than comparing the pair itself.
    pub fn text_selection(&self) -> Option<(usize, usize)> {
        let range = self.text_range()?;
        let (start, end) = range.ordered();
        (start < end).then_some((start, end))
    }

    /// A caret motion, optionally shift-extended. Without shift a live selection *collapses*
    /// to the end the motion points at rather than stepping on from the caret — the arrow key
    /// that cancels a selection must not also eat a character.
    pub fn text_step_caret(&mut self, step: Step, extend: bool) {
        let Some(edit) = self.text_edit.as_ref() else {
            return;
        };
        let (index, caret, anchor) = (edit.layer, edit.caret, edit.anchor);
        let Some(run) = self.layers.get(index).and_then(Layer::run) else {
            return;
        };
        if !extend && caret != anchor && matches!(step, Step::Left | Step::Right) {
            let (low, high) = (caret.min(anchor), caret.max(anchor));
            self.place_caret(if step == Step::Left { low } else { high }, false);
            return;
        }
        let next = step_index(run, caret, step);
        self.place_caret(next, extend);
    }

    pub fn text_set_caret_at(&mut self, doc_x: f32, doc_y: f32) {
        let Some(run) = self.editing_run() else {
            return;
        };
        let caret = index_at_point(run, doc_x, doc_y);
        self.place_caret(caret, false);
    }

    /// Drag-select: the anchor stays where the press landed and the caret follows the pointer.
    pub fn text_extend_to(&mut self, doc_x: f32, doc_y: f32) {
        let Some(run) = self.editing_run() else {
            return;
        };
        let caret = index_at_point(run, doc_x, doc_y);
        self.place_caret(caret, true);
    }

    /// `⌘A` while typing. `Document::select_all` routes here rather than building a document
    /// selection, so one shortcut means "everything in front of me" in both contexts.
    pub fn text_select_all(&mut self) -> bool {
        let Some(len) = self.editing_run().map(|run| run.text.len()) else {
            return false;
        };
        self.place_caret(0, false);
        self.place_caret(len, true);
        true
    }

    /// Double-click: the word under the pointer, anchored at its far end so a shift-drag from
    /// there keeps extending by the same gesture.
    pub fn text_select_word_at(&mut self, doc_x: f32, doc_y: f32) {
        self.select_span_at(doc_x, doc_y, word_range);
    }

    /// Triple-click: the whole paragraph, wrap or no wrap.
    pub fn text_select_paragraph_at(&mut self, doc_x: f32, doc_y: f32) {
        self.select_span_at(doc_x, doc_y, paragraph_range);
    }

    fn select_span_at(
        &mut self,
        doc_x: f32,
        doc_y: f32,
        range: impl Fn(&calumma_text::TextRun, usize) -> (usize, usize),
    ) {
        let Some(run) = self.editing_run() else {
            return;
        };
        let at = index_at_point(run, doc_x, doc_y);
        let (start, end) = range(run, at);
        self.place_caret(start, false);
        self.place_caret(end, true);
    }

    /// The one place caret and anchor are written. `extend` is the whole difference between a
    /// motion and a selection, and it is stated here rather than at each caller.
    pub(crate) fn place_caret(&mut self, caret: usize, extend: bool) {
        let Some(edit) = self.text_edit.as_ref() else {
            return;
        };
        let index = edit.layer;
        let clamped = self
            .layers
            .get(index)
            .and_then(Layer::run)
            .map(|run| run.clamp_index(caret))
            .unwrap_or(0);
        if let Some(edit) = &mut self.text_edit {
            edit.caret = clamped;
            if !extend {
                edit.anchor = clamped;
            }
        }
    }

    /// Caret as a document-space segment for the board to draw. `None` whenever nothing is
    /// being edited, which is also how the renderer knows to stop blinking.
    pub fn text_caret_segment(&self) -> Option<((f32, f32), (f32, f32))> {
        let edit = self.text_edit.as_ref()?;
        let run = self.layers.get(edit.layer)?.run()?;
        let caret = caret_rect(run, edit.caret);
        Some(((caret.x, caret.y), (caret.x, caret.y + caret.height)))
    }

    pub fn text_box(&self) -> Option<(f32, f32, f32, f32)> {
        let edit = self.text_edit.as_ref()?;
        let run = self.layers.get(edit.layer)?.run()?;
        Some(text_layer::run_box(run))
    }

    /// One quad per visual row of the selection, in document space. Empty whenever nothing is
    /// selected, so the renderer asks unconditionally.
    pub fn text_selection_rows(&self) -> Vec<SelectionRect> {
        let Some((start, end)) = self.text_selection() else {
            return Vec::new();
        };
        let Some(run) = self.editing_run() else {
            return Vec::new();
        };
        selection_rects(run, start, end)
    }

    pub fn text_caret_color(&self) -> [u8; 4] {
        self.editing_run()
            .map(|run| run.color)
            .unwrap_or(self.color)
    }
}
