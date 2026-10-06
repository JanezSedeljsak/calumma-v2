use super::Engine;

impl Engine {
    pub fn pointer_down(&mut self, x: f32, y: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.pointer_down(x, y);
            inner.dirty_save = true;
            inner.invalidate_renderer();
        }
    }

    pub fn pointer_move(&mut self, x: f32, y: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            let changed_content = doc.pointer_move(x, y);
            if changed_content {
                inner.invalidate_renderer();
            } else {
                inner.invalidate_overlay();
            }
        }
    }

    pub fn pointer_up(&mut self, x: f32, y: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.pointer_up(x, y);
            inner.dirty_save = true;
            inner.invalidate_renderer();
        }
    }

    pub fn set_pointer_hover(&mut self, x: f32, y: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_pointer_hover(x, y);
            inner.invalidate_overlay();
        }
    }

    pub fn clear_pointer_hover(&mut self) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.clear_pointer_hover();
            inner.invalidate_overlay();
        }
    }

    pub fn brush_ring_visible(&self) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .is_some_and(|doc| doc.brush_ring().is_some())
    }

    pub fn screen_on_paper(&self, x: f32, y: f32) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .is_some_and(|doc| doc.screen_on_paper(x, y))
    }

    pub fn set_shift_held(&mut self, held: bool) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_shift_held(held);
        }
    }

    pub fn set_alt_held(&mut self, held: bool) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_alt_held(held);
        }
    }
}
