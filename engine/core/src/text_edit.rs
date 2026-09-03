use crate::document::Document;
use crate::history::TileSnapshot;
use crate::layer::Layer;
use crate::text_layer;
use crate::tile::TileCoord;
use calumma_text::{caret_rect, index_at_point, step_index, Step, TextRun};

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
    layer_id: String,
    created: bool,
    before: TileSnapshot,
    before_run: Box<TextRun>,
}

impl Document {
    pub fn text_editing(&self) -> bool {
        self.text_edit.is_some()
    }

    pub fn text_edit_layer(&self) -> Option<usize> {
        self.text_edit.as_ref().map(|e| e.layer)
    }

    pub fn text_caret(&self) -> Option<usize> {
        self.text_edit.as_ref().map(|e| e.caret)
    }

    /// The run being typed into, or the active layer's run when nothing is being edited —
    /// so the shell can show the font and size of a selected text layer without entering it.
    pub fn active_text_run(&self) -> Option<&TextRun> {
        let index = self.text_edit_layer().unwrap_or(self.active_layer);
        self.layers.get(index)?.run()
    }

    fn editing_run(&self) -> Option<&TextRun> {
        self.layers.get(self.text_edit.as_ref()?.layer)?.run()
    }

    /// Click with the Text tool: re-enter the topmost text layer under the pointer, or start
    /// a new one there. Either way the caret lands where the click did, like Photoshop.
    pub fn begin_text_at(&mut self, doc_x: f32, doc_y: f32) {
        self.commit_text();
        if let Some(index) = self.text_layer_at(doc_x, doc_y) {
            self.enter_text(index, false);
            if let Some(run) = self.editing_run() {
                let caret = index_at_point(run, doc_x, doc_y);
                if let Some(edit) = &mut self.text_edit {
                    edit.caret = caret;
                }
            }
            return;
        }
        let run = TextRun {
            origin: (doc_x, doc_y - self.text_style.size * 0.5),
            color: self.color,
            ..self.text_style.clone()
        }
        .clamped();
        let n = self.layers.iter().filter(|l| l.is_text()).count() + 1;
        let layer = Layer::text(
            crate::names::numbered_text_layer(n),
            run,
            self.width,
            self.height,
        );
        self.layers.push(layer);
        self.active_layer = self.layers.len() - 1;
        let index = self.active_layer;
        self.enter_text(index, true);
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
            layer_id,
            created,
            before,
            before_run,
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

    pub(crate) fn with_run<R>(&mut self, f: impl FnOnce(&mut TextRun, usize) -> R) -> Option<R> {
        let edit = self.text_edit.as_ref()?;
        let index = self.text_edit_index(edit)?;
        let caret = edit.caret;
        let run = self.layers.get_mut(index)?.content.run_mut()?;
        let out = f(run, caret);
        self.resync_text(index);
        Some(out)
    }

    pub fn text_insert(&mut self, insert: &str) {
        if insert.is_empty() {
            return;
        }
        let Some(caret) = self.with_run(|run, caret| {
            let at = run.clamp_index(caret);
            let at = clear_marked(run, at);
            run.text.insert_str(at, insert);
            at + insert.len()
        }) else {
            return;
        };
        self.set_caret(caret);
    }

    /// An in-flight IME or dead-key composition. It is stored on the run so the board shows
    /// it exactly where it will land, and replaced wholesale on every update — the platform
    /// always sends the full composition, never a delta.
    pub fn text_set_marked(&mut self, marked: &str) {
        self.with_run(|run, caret| {
            let at = run.clamp_index(caret);
            let at = if run.marked.is_empty() {
                at
            } else {
                run.marked_at
            };
            run.marked_at = at;
            run.marked = marked.to_string();
        });
    }

    pub fn text_backspace(&mut self) {
        let Some(caret) = self.with_run(|run, caret| {
            let at = run.clamp_index(caret);
            if !run.marked.is_empty() {
                return clear_marked(run, at);
            }
            if at == 0 {
                return 0;
            }
            let prev = step_index(run, at, Step::Left);
            run.text.replace_range(prev..at, "");
            prev
        }) else {
            return;
        };
        self.set_caret(caret);
    }

    pub fn text_delete_forward(&mut self) {
        let Some(caret) = self.with_run(|run, caret| {
            let at = run.clamp_index(caret);
            if !run.marked.is_empty() {
                return clear_marked(run, at);
            }
            let next = step_index(run, at, Step::Right);
            if next > at {
                run.text.replace_range(at..next, "");
            }
            at
        }) else {
            return;
        };
        self.set_caret(caret);
    }

    pub fn text_step_caret(&mut self, step: Step) {
        let Some(edit) = self.text_edit.as_ref() else {
            return;
        };
        let (index, caret) = (edit.layer, edit.caret);
        let Some(run) = self.layers.get(index).and_then(Layer::run) else {
            return;
        };
        let next = step_index(run, caret, step);
        self.set_caret(next);
    }

    pub fn text_set_caret_at(&mut self, doc_x: f32, doc_y: f32) {
        let Some(run) = self.editing_run() else {
            return;
        };
        let caret = index_at_point(run, doc_x, doc_y);
        self.set_caret(caret);
    }

    fn set_caret(&mut self, caret: usize) {
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

    pub fn text_caret_color(&self) -> [u8; 4] {
        self.editing_run()
            .map(|run| run.color)
            .unwrap_or(self.color)
    }
}

/// Drops any composition in progress and reports where the caret should sit afterwards.
fn clear_marked(run: &mut TextRun, caret: usize) -> usize {
    if run.marked.is_empty() {
        return caret;
    }
    let at = run.clamp_index(run.marked_at);
    run.marked.clear();
    run.marked_at = at;
    at
}
