use super::Engine;
use calumma_core::ruler::RulerTick;
use calumma_core::Camera;

impl Engine {
    pub fn pan(&mut self, dx: f32, dy: f32) {
        let mut inner = self.inner.lock();
        inner.pending_pan_dx += dx;
        inner.pending_pan_dy += dy;
        inner.invalidate_camera();
    }

    pub fn pan_scroll(&mut self, dx: f32, dy: f32, precise: bool) {
        let mut inner = self.inner.lock();
        inner.pending_scroll_dx += dx;
        inner.pending_scroll_dy += dy;
        if precise {
            inner.pending_scroll_precise = true;
        }
        inner.invalidate_camera();
    }

    pub fn zoom_scroll(&mut self, x: f32, y: f32, delta: f32, precise: bool) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            let (w, h) = (doc.width as f32, doc.height as f32);
            doc.camera.zoom_by_scroll(x, y, delta, precise, w, h);
            inner.invalidate_camera();
        }
    }

    pub fn zoom(&mut self, x: f32, y: f32, factor: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            let next = doc.camera.zoom * factor;
            let (w, h) = (doc.width as f32, doc.height as f32);
            doc.camera.zoom_at(x, y, next, w, h);
            inner.invalidate_camera();
        }
    }

    pub fn zoom_to(&mut self, x: f32, y: f32, zoom: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            let (w, h) = (doc.width as f32, doc.height as f32);
            doc.camera.zoom_at(x, y, zoom, w, h);
            inner.invalidate_camera();
        }
    }

    pub fn end_camera_motion(&mut self) {
        let mut inner = self.inner.lock();
        if let Some(renderer) = &mut inner.renderer {
            renderer.end_camera_motion();
        }
    }

    pub fn camera_pan(&self) -> (f32, f32) {
        let inner = self.inner.lock();
        inner
            .doc
            .as_ref()
            .map(|doc| (doc.camera.pan_x, doc.camera.pan_y))
            .unwrap_or((0.0, 0.0))
    }

    pub fn ruler_ticks_x(&self) -> Vec<RulerTick> {
        let inner = self.inner.lock();
        inner
            .doc
            .as_ref()
            .map(|doc| doc.camera.ruler_ticks_x())
            .unwrap_or_default()
    }

    pub fn ruler_ticks_y(&self) -> Vec<RulerTick> {
        let inner = self.inner.lock();
        inner
            .doc
            .as_ref()
            .map(|doc| doc.camera.ruler_ticks_y())
            .unwrap_or_default()
    }

    pub fn zoom_unit(&self) -> f32 {
        let inner = self.inner.lock();
        inner
            .doc
            .as_ref()
            .map(|doc| {
                let (w, h) = (doc.width as f32, doc.height as f32);
                doc.camera.zoom_unit(w, h)
            })
            .unwrap_or(0.0)
    }

    pub fn zoom_factor(&self) -> f32 {
        let inner = self.inner.lock();
        inner.doc.as_ref().map(|doc| doc.camera.zoom).unwrap_or(1.0)
    }

    pub fn is_fit(&self) -> bool {
        let inner = self.inner.lock();
        inner
            .doc
            .as_ref()
            .map(|doc| doc.camera.is_fit(doc.width as f32, doc.height as f32))
            .unwrap_or(false)
    }

    pub fn fit_preview(
        viewport_width: f32,
        viewport_height: f32,
        doc_width: u32,
        doc_height: u32,
    ) -> Camera {
        let camera = Camera {
            viewport_width: viewport_width.max(1.0),
            viewport_height: viewport_height.max(1.0),
            ..Camera::default()
        };
        camera.fitted(doc_width as f32, doc_height as f32)
    }

    pub fn set_zoom_unit(&mut self, unit: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            let (w, h) = (doc.width as f32, doc.height as f32);
            let zoom = doc.camera.zoom_from_unit(unit, w, h);
            doc.camera.zoom_to_center(zoom, w, h);
            inner.invalidate_camera();
        }
    }

    pub fn step_zoom(&mut self, zoom_in: bool) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            let (w, h) = (doc.width as f32, doc.height as f32);
            doc.camera.step_zoom(zoom_in, w, h);
            inner.invalidate_camera();
        }
    }

    pub fn fit_to_view(&mut self) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.fit_to_view();
            inner.invalidate_camera();
        }
    }
}
