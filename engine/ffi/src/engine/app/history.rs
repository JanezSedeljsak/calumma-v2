use super::Engine;

impl Engine {
    pub fn undo(&mut self) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.undo();
            inner.dirty_save = true;
            inner.invalidate_renderer();
        }
    }

    pub fn redo(&mut self) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.redo();
            inner.dirty_save = true;
            inner.invalidate_renderer();
        }
    }

    pub fn can_undo(&self) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .is_some_and(|doc| doc.history.can_undo())
    }

    pub fn can_redo(&self) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .is_some_and(|doc| doc.history.can_redo())
    }
}
