use super::Engine;

impl Engine {
    pub fn ink_color(&self) -> [u8; 4] {
        self.inner
            .lock()
            .doc
            .as_ref()
            .map(|doc| doc.color)
            .unwrap_or([0, 0, 0, 255])
    }

    pub fn stroke_color(&self) -> [u8; 4] {
        self.inner
            .lock()
            .doc
            .as_ref()
            .map(|doc| doc.stroke_color)
            .unwrap_or([0, 0, 0, 255])
    }

    pub fn shape_fill_color(&self) -> [u8; 4] {
        self.inner
            .lock()
            .doc
            .as_ref()
            .map(|doc| doc.shape_fill_color)
            .unwrap_or([255, 255, 255, 255])
    }

    pub fn select_color(&self) -> [u8; 4] {
        self.inner
            .lock()
            .doc
            .as_ref()
            .map(|doc| doc.select_color())
            .unwrap_or([0, 0, 0, 255])
    }

    pub fn set_ink_color(&mut self, color: [u8; 4]) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_color(color);
            inner.invalidate_renderer();
        }
    }

    pub fn set_stroke_color(&mut self, color: [u8; 4]) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.stroke_color = color;
            inner.invalidate_renderer();
        }
    }

    pub fn set_shape_fill_color(&mut self, color: [u8; 4]) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.shape_fill_color = color;
            inner.invalidate_renderer();
        }
    }

    pub fn set_select_color(&mut self, color: [u8; 4]) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_select_color(color);
            inner.invalidate_renderer();
        }
    }

    pub fn push_quick_colors(
        &mut self,
        ink: [u8; 4],
        stroke: [u8; 4],
        fill: [u8; 4],
        select: [u8; 4],
    ) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_color(ink);
            doc.stroke_color = stroke;
            doc.shape_fill_color = fill;
            doc.set_select_color(select);
            inner.invalidate_renderer();
        }
    }
}
