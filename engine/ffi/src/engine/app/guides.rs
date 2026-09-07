use super::Engine;
use calumma_core::guide::GuideAxis;
use calumma_core::limits::GUIDES_LIMIT;
use calumma_core::{project_color, Guide};

pub struct GuideInfo {
    pub axis: GuideAxis,
    pub position: f32,
    pub color: [u8; 3],
}

impl From<&Guide> for GuideInfo {
    fn from(guide: &Guide) -> Self {
        Self {
            axis: guide.axis,
            position: guide.position,
            color: guide.color,
        }
    }
}

fn axis(horizontal: bool) -> GuideAxis {
    if horizontal {
        GuideAxis::Horizontal
    } else {
        GuideAxis::Vertical
    }
}

impl Engine {
    pub fn guides(&self) -> Vec<GuideInfo> {
        self.inner
            .lock()
            .doc
            .as_ref()
            .map(|doc| doc.guides().iter().map(GuideInfo::from).collect())
            .unwrap_or_default()
    }

    pub fn guides_limit(&self) -> usize {
        GUIDES_LIMIT
    }

    pub fn add_guide(&mut self, horizontal: bool, position: f32) -> bool {
        let mut inner = self.inner.lock();
        let Some(doc) = inner.doc.as_mut() else {
            return false;
        };
        let chosen = axis(horizontal);
        let extent = match chosen {
            GuideAxis::Horizontal => doc.height as f32,
            GuideAxis::Vertical => doc.width as f32,
        };
        let clamped = position.clamp(0.0, extent);
        let before = doc.guides().len();
        let Some(_) = doc.add_guide(chosen, clamped) else {
            return false;
        };
        if doc.guides().len() == before {
            return false;
        }
        inner.dirty_save = true;
        inner.invalidate_overlay();
        true
    }

    pub fn remove_guide(&mut self, index: usize) {
        let mut inner = self.inner.lock();
        if let Some(doc) = inner.doc.as_mut() {
            if doc.remove_guide(index) {
                inner.dirty_save = true;
                inner.invalidate_overlay();
            }
        }
    }

    pub fn clear_guides(&mut self) {
        let mut inner = self.inner.lock();
        if let Some(doc) = inner.doc.as_mut() {
            if doc.clear_guides() {
                inner.dirty_save = true;
                inner.invalidate_overlay();
            }
        }
    }

    pub fn set_guide_position(&mut self, index: usize, position: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = inner.doc.as_mut() {
            if doc.set_guide_position(index, position) {
                inner.dirty_save = true;
                inner.invalidate_overlay();
            }
        }
    }

    pub fn set_guide_axis(&mut self, index: usize, horizontal: bool) {
        let mut inner = self.inner.lock();
        if let Some(doc) = inner.doc.as_mut() {
            if doc.set_guide_axis(index, axis(horizontal)) {
                inner.dirty_save = true;
                inner.invalidate_overlay();
            }
        }
    }

    pub fn set_guide_color(&mut self, index: usize, palette_index: usize) {
        let mut inner = self.inner.lock();
        if let Some(doc) = inner.doc.as_mut() {
            if doc.set_guide_color(index, project_color(palette_index)) {
                inner.dirty_save = true;
                inner.invalidate_overlay();
            }
        }
    }

    pub fn dragged_guide_readout(&self) -> Option<(bool, f32, f32)> {
        self.inner.lock().doc.as_ref().and_then(|doc| {
            let (guide_axis, position, screen) = doc.dragged_guide_readout()?;
            Some((guide_axis == GuideAxis::Horizontal, position, screen))
        })
    }

    pub fn is_dragging_guide(&self) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .is_some_and(|doc| doc.is_dragging_guide())
    }
}
