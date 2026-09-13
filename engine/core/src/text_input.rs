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
}

/// The range an edit is about to replace, with any composition dropped first.
fn pending_range(run: &mut TextRun, range: TextRange) -> (usize, usize) {
    let (start, end) = range.ordered();
    (run.clamp_index(start), run.clamp_index(end))
}
