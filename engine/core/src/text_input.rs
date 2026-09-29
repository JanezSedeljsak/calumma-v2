//! The edits that change a text layer's string: typed characters, an in-flight composition,
//! and the two deletes.
//!
//! Every one of them goes through `Document::with_run`, so each lands on the session's layer
//! and re-rasterizes the tile cache in one place, and through `TextRun::replace_range`, so each
//! moves the run's style spans by the same rule. Where the caret and its anchor *are* is
//! `text_select.rs`; opening and closing the session is `text_edit.rs`.

use crate::document::Document;
use crate::text_edit::TextRange;
use calumma_text::{step_index, Step, TextRun};

impl Document {
    pub fn text_insert(&mut self, insert: &str) {
        if insert.is_empty() {
            return;
        }
        self.drop_text_composition();
        let Some(caret) = self.with_run(|run, range| {
            let (start, end) = pending_range(run, range);
            run.replace_range(start, end, insert);
            start + insert.len()
        }) else {
            return;
        };
        self.place_caret(caret, false);
    }

    pub fn text_backspace(&mut self) {
        self.drop_text_composition();
        let Some(caret) = self.with_run(|run, range| {
            let (start, end) = pending_range(run, range);
            if end > start {
                run.replace_range(start, end, "");
                return start;
            }
            if start == 0 {
                return 0;
            }
            let prev = step_index(run, start, Step::Left);
            run.replace_range(prev, start, "");
            prev
        }) else {
            return;
        };
        self.place_caret(caret, false);
    }

    pub fn text_delete_forward(&mut self) {
        self.drop_text_composition();
        let Some(caret) = self.with_run(|run, range| {
            let (start, end) = pending_range(run, range);
            if end > start {
                run.replace_range(start, end, "");
                return start;
            }
            let next = step_index(run, start, Step::Right);
            if next > start {
                run.replace_range(start, next, "");
            }
            start
        }) else {
            return;
        };
        self.place_caret(caret, false);
    }

    /// `⌥⌫` / `⌥⌦`: widen an empty range to the word on that side, then delete it the way a
    /// selection is deleted. A live selection is deleted as it stands, like any other delete.
    pub fn text_delete_word(&mut self, forward: bool) {
        self.drop_text_composition();
        if self.text_range().is_some_and(|range| range.is_empty()) {
            let step = if forward {
                Step::WordRight
            } else {
                Step::WordLeft
            };
            self.text_step_caret(step, true);
        }
        if forward {
            self.text_delete_forward();
        } else {
            self.text_backspace();
        }
    }

    /// An input method's in-flight text: replaces the previous composition, or the selection
    /// when a composition is just starting, and parks the caret at `cursor` bytes into it (its
    /// end when the input method does not say). An empty `text` ends the composition and puts
    /// the run back as it was. Committing is an ordinary `text_insert`, which drops the
    /// composition before it types.
    pub fn text_set_composition(&mut self, text: &str, cursor: Option<usize>) {
        let Some(replacing) = self.text_edit.as_ref().map(|edit| edit.composition) else {
            return;
        };
        if text.is_empty() && replacing.is_none() {
            return;
        }
        let Some(start) = self.with_run(|run, range| {
            let (start, end) = replacing.unwrap_or_else(|| range.ordered());
            let (start, end) = (run.clamp_index(start), run.clamp_index(end));
            run.replace_range(start, end, text);
            start
        }) else {
            return;
        };
        let offset = cursor
            .filter(|&at| text.is_char_boundary(at))
            .unwrap_or(text.len());
        self.place_caret(start + offset, false);
        if let Some(edit) = &mut self.text_edit {
            edit.composition = (!text.is_empty()).then_some((start, start + text.len()));
        }
    }

    pub fn text_composition(&self) -> Option<(usize, usize)> {
        self.text_edit.as_ref()?.composition
    }

    pub(crate) fn drop_text_composition(&mut self) {
        if self.text_composition().is_some() {
            self.text_set_composition("", None);
        }
    }
}

/// The range an edit is about to replace.
fn pending_range(run: &mut TextRun, range: TextRange) -> (usize, usize) {
    let (start, end) = range.ordered();
    (run.clamp_index(start), run.clamp_index(end))
}
