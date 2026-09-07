use super::Engine;
use calumma_core::limits::{
    BLUR_STRENGTH_MAX, BLUR_STRENGTH_MIN, ERASER_HARDNESS_MAX, ERASER_HARDNESS_MIN,
    EYEDROPPER_RADIUS_MAX, EYEDROPPER_RADIUS_MIN, TOLERANCE_MAX, TOLERANCE_MIN,
};
use calumma_core::{Brush, Tool};

impl Engine {
    pub fn shape_fill(&self) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .map(|doc| doc.fill)
            .unwrap_or(false)
    }

    pub fn set_shape_fill(&mut self, on: bool) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.fill = on;
            inner.invalidate_overlay();
        }
    }

    pub fn shape_stroke(&self) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .map(|doc| doc.stroke)
            .unwrap_or(true)
    }

    pub fn set_shape_stroke(&mut self, on: bool) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.stroke = on;
            inner.invalidate_overlay();
        }
    }

    pub fn brush(&self) -> Brush {
        self.inner
            .lock()
            .doc
            .as_ref()
            .map(|doc| doc.brush)
            .unwrap_or_default()
    }

    pub fn set_brush(&mut self, brush: Brush) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_brush(brush);
            inner.invalidate_overlay();
        }
    }

    pub fn blur_strength(&self) -> f32 {
        self.inner
            .lock()
            .doc
            .as_ref()
            .map(|doc| doc.blur_strength)
            .unwrap_or(0.5)
    }

    pub fn set_blur_strength(&mut self, strength: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_blur_strength(strength.clamp(BLUR_STRENGTH_MIN, BLUR_STRENGTH_MAX));
            inner.invalidate_overlay();
        }
    }

    pub fn clone_aligned(&self) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .is_some_and(|doc| doc.clone_aligned)
    }

    pub fn set_clone_aligned(&mut self, aligned: bool) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_clone_aligned(aligned);
        }
    }

    pub fn vector_mode(&self) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .is_some_and(|doc| doc.effective_vector_mode())
    }

    pub fn vector_mode_locked(&self) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .is_some_and(|doc| doc.vector_mode_locked())
    }

    pub fn set_vector_mode(&mut self, on: bool) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            if !doc.vector_mode_locked() {
                doc.set_vector_mode(on);
                inner.invalidate_overlay();
            }
        }
    }

    pub fn eraser_hardness(&self) -> f32 {
        self.inner
            .lock()
            .doc
            .as_ref()
            .map(|doc| doc.eraser_hardness)
            .unwrap_or(1.0)
    }

    pub fn set_eraser_hardness(&mut self, hardness: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_eraser_hardness(hardness.clamp(ERASER_HARDNESS_MIN, ERASER_HARDNESS_MAX));
            inner.invalidate_overlay();
        }
    }

    pub fn tolerance(&self) -> u8 {
        self.inner
            .lock()
            .doc
            .as_ref()
            .map(|doc| doc.tolerance)
            .unwrap_or(0)
    }

    pub fn set_tolerance(&mut self, tolerance: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            let value = tolerance
                .round()
                .clamp(f32::from(TOLERANCE_MIN), f32::from(TOLERANCE_MAX));
            doc.set_tolerance(value as u8);
        }
    }

    pub fn eyedropper_radius(&self) -> u32 {
        self.inner
            .lock()
            .doc
            .as_ref()
            .map(|doc| doc.eyedropper_radius)
            .unwrap_or(1)
    }

    pub fn set_eyedropper_radius(&mut self, radius: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            let value = radius
                .round()
                .clamp(EYEDROPPER_RADIUS_MIN as f32, EYEDROPPER_RADIUS_MAX as f32)
                as u32;
            doc.set_eyedropper_radius(value);
            inner.invalidate_overlay();
        }
    }

    pub fn transform_active(&self) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .is_some_and(|doc| doc.transform_active)
    }

    pub fn set_move_transform(&mut self, on: bool) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            if on {
                doc.set_tool(Tool::Move);
                doc.enter_transform();
            } else {
                doc.exit_transform();
            }
            inner.invalidate_renderer();
        }
    }
}
