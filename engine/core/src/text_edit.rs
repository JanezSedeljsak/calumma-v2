//! The text session: opening one, placing a block, and closing it into a single undo step.
//!
//! Two neighbours split the rest off. `text_input.rs` holds the edits that change the string,
//! and `text_select.rs` holds where the caret and its anchor are and what the board draws for
//! them. What stays here is the session's own lifetime, which is the part every structural
//! layer edit has to reason about: a session indexes a layer by position, so it must not
//! outlive a stack that moved.

use crate::document::Document;
use crate::history::TileSnapshot;
use crate::layer::Layer;
use crate::text_layer;
use crate::tile::TileCoord;
use calumma_text::{index_at_point, TextRun, TEXT_WRAP_MIN_WIDTH};

/// A live typing session on one text layer.
///
/// The caret lives here rather than on the run because it is not content — reopening a
/// project restores the text, not where someone's cursor happened to be. `before` is the
/// layer's tiles as they stood when editing began, which is what makes a whole editing
/// session one undo step instead of one per keystroke.
#[derive(Clone, Debug)]
pub struct TextEdit {
    pub layer: usize,
    pub caret: usize,
    /// The other end of the selection, equal to `caret` whenever nothing is selected. Every
    /// caret mutation resets it unless the motion was shift-extended — that one rule is what
    /// keeps selection from leaking into every call site.
    pub anchor: usize,
    layer_id: String,
    created: bool,
    before: TileSnapshot,
    before_run: Box<TextRun>,
    pub(crate) press: Option<TextPress>,
}

/// What the pointer is doing between a press and its release, which decides what a drag means.
/// A press that made a new layer draws its wrap box; a press into text that was already there
/// sweeps a selection, because resizing somebody's paragraph by dragging inside it would be a
/// trap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TextPress {
    Box { start: (f32, f32) },
    Select,
}

/// A caret and its anchor. `(min, max)` of the pair is the selected range, and the pair being
/// equal is the ordinary no-selection case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextRange {
    pub caret: usize,
    pub anchor: usize,
}

impl TextRange {
    pub fn ordered(self) -> (usize, usize) {
        (self.caret.min(self.anchor), self.caret.max(self.anchor))
    }

    pub fn is_empty(self) -> bool {
        self.caret == self.anchor
    }
}

impl Document {
    pub fn text_editing(&self) -> bool {
        self.text_edit.is_some()
    }

    pub fn text_edit_layer(&self) -> Option<usize> {
        self.text_edit.as_ref().map(|e| e.layer)
    }

    /// The run being typed into, or the active layer's run when nothing is being edited —
    /// so the shell can show the font and size of a selected text layer without entering it.
    pub fn active_text_run(&self) -> Option<&TextRun> {
        let index = self.text_edit_layer().unwrap_or(self.active_layer);
        self.layers.get(index)?.run()
    }

    pub(crate) fn editing_run(&self) -> Option<&TextRun> {
        self.layers.get(self.text_edit.as_ref()?.layer)?.run()
    }

    /// Click with the Text tool: re-enter the topmost text layer under the pointer, or start
    /// a new one there. Either way the caret lands where the click did, like Photoshop.
    pub fn begin_text_at(&mut self, doc_x: f32, doc_y: f32) {
        // Shift-click extends the range in the session that is already open, so this one press
        // must not close the session it is extending — every other press does.
        if self.shift_held && self.text_editing() {
            let hit = self.text_layer_at(doc_x, doc_y);
            if hit.is_some() && hit == self.text_edit_layer() {
                self.text_extend_to(doc_x, doc_y);
                if let Some(edit) = &mut self.text_edit {
                    edit.press = Some(TextPress::Select);
                }
                return;
            }
        }
        self.commit_text();
        if let Some(index) = self.text_layer_at(doc_x, doc_y) {
            self.enter_text(index, false);
            if let Some(run) = self.editing_run() {
                let caret = index_at_point(run, doc_x, doc_y);
                if let Some(edit) = &mut self.text_edit {
                    edit.caret = caret;
                    edit.anchor = caret;
                    edit.press = Some(TextPress::Select);
                }
            }
            return;
        }
        self.push_text_layer(TextRun {
            origin: (doc_x, doc_y - self.text_style.size * 0.5),
            color: self.color,
            ..self.text_style.clone()
        });
        if let Some(edit) = &mut self.text_edit {
            edit.press = Some(TextPress::Box {
                start: (doc_x, doc_y),
            });
        }
    }

    /// A wrapped text box, from the rectangle a drag swept. The engine has honoured
    /// `wrap_width` since text existed — this is the gesture that finally reaches it, and the
    /// only difference from a click-placed run is that the origin is the box's corner rather
    /// than a baseline guess.
    pub fn begin_text_box(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.commit_text();
        let (min_x, max_x) = (x0.min(x1), x0.max(x1));
        let min_y = y0.min(y1);
        self.push_text_layer(TextRun {
            origin: (min_x, min_y),
            color: self.color,
            wrap_width: Some(max_x - min_x),
            ..self.text_style.clone()
        });
    }

    fn push_text_layer(&mut self, run: TextRun) {
        let n = self.layers.iter().filter(|l| l.is_text()).count() + 1;
        let layer = Layer::text(
            crate::names::numbered_text_layer(n),
            run.clamped(),
            self.width,
            self.height,
        );
        self.layers.push(layer);
        self.active_layer = self.layers.len() - 1;
        let index = self.active_layer;
        self.enter_text(index, true);
    }

    /// Dragging with the Text tool. A press that opened a fresh layer is still deciding how
    /// wide the block is; a press into existing text is sweeping a selection. Nothing else can
    /// be dragged with this tool, so there is no third case.
    ///
    /// Returns whether the layer's *pixels* moved, which is the answer `pointer_move` turns
    /// into a content invalidate rather than an overlay one. Re-wrapping re-rasterizes; sweeping
    /// a selection only moves furniture the overlay pass draws every frame anyway.
    pub fn text_pointer_move(&mut self, doc_x: f32, doc_y: f32) -> bool {
        let Some(press) = self.text_edit.as_ref().and_then(|e| e.press) else {
            return false;
        };
        match press {
            TextPress::Select => {
                self.text_extend_to(doc_x, doc_y);
                false
            }
            TextPress::Box { start } => {
                let width = (doc_x - start.0).abs();
                let wrap = (width >= TEXT_WRAP_MIN_WIDTH).then_some(width);
                let size = self.text_style.size;
                self.with_run(|run, _| {
                    run.wrap_width = wrap;
                    run.origin = match wrap {
                        Some(_) => (start.0.min(doc_x), start.1.min(doc_y)),
                        None => (start.0, start.1 - size * 0.5),
                    };
                    *run = std::mem::take(run).clamped();
                })
                .is_some()
            }
        }
    }

    pub fn text_pointer_up(&mut self) {
        if let Some(edit) = &mut self.text_edit {
            edit.press = None;
        }
    }

    pub fn text_layer_at(&self, doc_x: f32, doc_y: f32) -> Option<usize> {
        self.layers
            .iter()
            .enumerate()
            .rev()
            .find(|(_, layer)| {
                layer.visible
                    && layer
                        .run()
                        .is_some_and(|run| text_layer::hits_run(run, doc_x, doc_y))
            })
            .map(|(index, _)| index)
    }

    /// Enters an existing text layer with the caret at the end — what a double-click on a
    /// text layer in the layers panel should do.
    pub fn edit_text_layer(&mut self, index: usize) -> bool {
        if self.layers.get(index).and_then(Layer::run).is_none() {
            return false;
        }
        self.commit_text();
        self.enter_text(index, false);
        if let Some(len) = self.editing_run().map(|r| r.text.len()) {
            if let Some(edit) = &mut self.text_edit {
                edit.caret = len;
                edit.anchor = len;
            }
        }
        true
    }

    fn enter_text(&mut self, index: usize, created: bool) {
        let Some(layer) = self.layers.get(index) else {
            return;
        };
        let layer_id = layer.id.clone();
        let before_run = Box::new(layer.run().cloned().unwrap_or_default());
        let before = layer
            .tiles()
            .map(|grid| {
                let coords: Vec<TileCoord> = grid.coords().collect();
                grid.snapshot_tiles(&coords)
            })
            .unwrap_or_default();
        self.exit_transform();
        self.active_layer = index;
        self.text_edit = Some(TextEdit {
            layer: index,
            caret: 0,
            anchor: 0,
            layer_id,
            created,
            before,
            before_run,
            press: None,
        });
    }

    /// Ends the session. Anything typed becomes a single undo step covering every tile the
    /// run touched from start to finish; a layer that was created this session and left
    /// empty leaves nothing behind, while emptying a layer that already existed is an edit
    /// like any other and stays undoable.
    /// Where the edited layer sits *now*. `TextEdit.layer` is a position, and positions move
    /// when the stack is reordered or a layer is inserted beneath this one; the layer's id does
    /// not. `commit_text` can remove a layer, so it resolves through this rather than trusting
    /// a stored index — getting that wrong deletes somebody else's work.
    fn text_edit_index(&self, edit: &TextEdit) -> Option<usize> {
        if self
            .layers
            .get(edit.layer)
            .is_some_and(|l| l.id == edit.layer_id)
        {
            return Some(edit.layer);
        }
        self.layers.iter().position(|l| l.id == edit.layer_id)
    }

    pub fn commit_text(&mut self) {
        let Some(edit) = self.text_edit.take() else {
            return;
        };
        // The layer can be gone entirely, in which case there is nothing left to commit.
        let Some(index) = self.text_edit_index(&edit) else {
            return;
        };
        if let Some(run) = self.layers.get_mut(index).and_then(|l| l.content.run_mut()) {
            if !run.marked.is_empty() {
                run.marked.clear();
                self.resync_text(index);
            }
        }
        let empty = self
            .layers
            .get(index)
            .and_then(Layer::run)
            .is_some_and(TextRun::is_empty);
        if empty && edit.created {
            self.remove_layer_inner(index, false);
            return;
        }
        let mut before = edit.before;
        if let Some(grid) = self.layers.get(index).and_then(Layer::tiles) {
            for coord in grid.coords() {
                before.entry(coord).or_insert(None);
            }
        }
        let unchanged = self
            .layers
            .get(index)
            .and_then(Layer::run)
            .is_some_and(|run| *run == *edit.before_run);
        if before.is_empty() && unchanged {
            return;
        }
        self.history
            .push_layer_text(edit.layer_id, before, edit.before_run, Some(index));
    }

    fn resync_text(&mut self, index: usize) {
        if let Some(layer) = self.layers.get_mut(index) {
            text_layer::resync(layer);
        }
    }

    pub(crate) fn with_run<R>(
        &mut self,
        f: impl FnOnce(&mut TextRun, TextRange) -> R,
    ) -> Option<R> {
        let edit = self.text_edit.as_ref()?;
        let index = self.text_edit_index(edit)?;
        let range = TextRange {
            caret: edit.caret,
            anchor: edit.anchor,
        };
        let run = self.layers.get_mut(index)?.content.run_mut()?;
        let out = f(run, range);
        self.resync_text(index);
        Some(out)
    }
}
