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

    /// An in-flight IME or dead-key composition. It is stored on the run so the board shows
    /// it exactly where it will land, and replaced wholesale on every update — the platform
    /// always sends the full composition, never a delta.
    pub fn text_set_marked(&mut self, marked: &str) {
        let Some(caret) = self.with_run(|run, range| {
            if run.marked.is_empty() {
                // A composition replaces the selection, exactly as typed text does — the
                // range has to go before the first reading is spliced in, or the two would
                // both be on screen.
                let (start, end) = range.ordered();
                let (start, end) = (run.clamp_index(start), run.clamp_index(end));
                if end > start {
                    run.replace_range(start, end, "");
                }
                run.marked_at = start;
            }
            run.marked = marked.to_string();
            run.marked_at
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
}

/// The range an edit is about to replace, with any composition dropped first.
///
/// A composition in flight owns the keystroke — it is discarded and the caret parks where it
/// sat, with nothing selected — so an IME cancel never also deletes a selection that was made
/// before the composition started.
fn pending_range(run: &mut TextRun, range: TextRange) -> (usize, usize) {
    if !run.marked.is_empty() {
        let at = run.clamp_index(run.marked_at);
        run.marked.clear();
        run.marked_at = at;
        return (at, at);
    }
    let (start, end) = range.ordered();
    (run.clamp_index(start), run.clamp_index(end))
}
