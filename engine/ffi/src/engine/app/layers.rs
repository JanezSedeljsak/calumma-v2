use super::Engine;
use calumma_core::{Adjustments, AlignEdge, BlendMode, DistributeAxis, Document};

impl Engine {
    pub fn layer_selection(&self) -> Vec<usize> {
        self.inner
            .lock()
            .doc
            .as_ref()
            .map(|doc| doc.layer_selection().to_vec())
            .unwrap_or_default()
    }

    pub fn set_layer_selection(&mut self, indices: &[usize]) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_layer_selection(indices);
            inner.invalidate_overlay();
        }
    }

    pub fn align_selected_layers(&mut self, edge: AlignEdge) -> bool {
        self.edit_selected_layers(|doc, indices| doc.align_layers(indices, edge))
    }

    pub fn distribute_selected_layers(&mut self, axis: DistributeAxis) -> bool {
        self.edit_selected_layers(|doc, indices| doc.distribute_layers(indices, axis))
    }

    fn edit_selected_layers(&mut self, edit: impl FnOnce(&mut Document, &[usize]) -> bool) -> bool {
        let mut inner = self.inner.lock();
        let Some(doc) = inner.doc.as_mut() else {
            return false;
        };
        let indices = doc.layer_selection().to_vec();
        if !edit(doc, &indices) {
            return false;
        }
        inner.dirty_save = true;
        inner.invalidate_renderer();
        true
    }

    pub fn layer_blend_mode(&self, index: usize) -> BlendMode {
        self.inner
            .lock()
            .doc
            .as_ref()
            .and_then(|doc| doc.layers.get(index))
            .map(|layer| layer.blend_mode)
            .unwrap_or_default()
    }

    pub fn set_layer_blend_mode(&mut self, index: usize, mode: BlendMode) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_layer_blend_mode(index, mode);
            inner.dirty_save = true;
            inner.invalidate_renderer();
        }
    }

    pub fn layer_adjustments(&self, index: usize) -> Adjustments {
        self.inner
            .lock()
            .doc
            .as_ref()
            .and_then(|doc| doc.layers.get(index))
            .and_then(|layer| layer.adjustments)
            .unwrap_or_default()
    }

    pub fn set_layer_adjustments(&mut self, index: usize, adjustments: Adjustments) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_layer_adjustments(index, adjustments);
            inner.dirty_save = true;
            inner.invalidate_renderer();
        }
    }

    pub fn reset_layer_adjustments(&mut self, index: usize) {
        self.set_layer_adjustments(index, Adjustments::default());
    }

    pub fn is_layer_clipped(&self, index: usize) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .is_some_and(|doc| doc.is_layer_clipped(index))
    }

    pub fn is_layer_masked(&self, index: usize) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .is_some_and(|doc| doc.is_layer_masked(index))
    }

    pub fn can_create_clipping_mask(&self, index: usize) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .is_some_and(|doc| doc.can_create_clipping_mask(index))
    }

    pub fn can_release_clipping_mask(&self, index: usize) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .is_some_and(|doc| doc.can_release_clipping_mask(index))
    }

    pub fn create_clipping_mask(&mut self, index: usize) -> bool {
        let mut inner = self.inner.lock();
        let Some(doc) = inner.doc.as_mut() else {
            return false;
        };
        if !doc.create_clipping_mask(index) {
            return false;
        }
        inner.dirty_save = true;
        inner.invalidate_renderer();
        true
    }

    pub fn release_clipping_mask(&mut self, index: usize) -> bool {
        let mut inner = self.inner.lock();
        let Some(doc) = inner.doc.as_mut() else {
            return false;
        };
        if !doc.release_clipping_mask(index) {
            return false;
        }
        inner.dirty_save = true;
        inner.invalidate_renderer();
        true
    }

    pub fn can_create_layer_mask(&self, index: usize) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .is_some_and(|doc| doc.can_create_layer_mask(index))
    }

    pub fn can_release_layer_mask(&self, index: usize) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .is_some_and(|doc| doc.can_release_layer_mask(index))
    }

    pub fn create_layer_mask(&mut self, index: usize) -> bool {
        let mut inner = self.inner.lock();
        let Some(doc) = inner.doc.as_mut() else {
            return false;
        };
        if !doc.create_layer_mask(index) {
            return false;
        }
        inner.dirty_save = true;
        inner.invalidate_renderer();
        true
    }

    pub fn release_layer_mask(&mut self, index: usize) -> bool {
        let mut inner = self.inner.lock();
        let Some(doc) = inner.doc.as_mut() else {
            return false;
        };
        if !doc.release_layer_mask(index) {
            return false;
        }
        inner.dirty_save = true;
        inner.invalidate_renderer();
        true
    }

    pub fn can_flatten_clip(&self, index: usize) -> bool {
        self.inner.lock().doc.as_ref().is_some_and(|doc| {
            if doc.is_layer_masked(index) {
                doc.can_apply_layer_mask(index)
            } else {
                doc.is_layer_clipped(index) && doc.can_clip_layer_down(index)
            }
        })
    }

    pub fn flatten_clip(&mut self, index: usize) -> bool {
        let mut inner = self.inner.lock();
        let Some(doc) = inner.doc.as_mut() else {
            return false;
        };
        let ok = if doc.is_layer_masked(index) {
            doc.apply_layer_mask(index)
        } else {
            doc.clip_layer_down(index)
        };
        if !ok {
            return false;
        }
        inner.dirty_save = true;
        inner.invalidate_renderer();
        true
    }

    pub fn can_merge_layer_down(&self, index: usize) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .is_some_and(|doc| doc.can_merge_layer_down(index))
    }

    pub fn merge_layer_down(&mut self, index: usize) -> bool {
        let mut inner = self.inner.lock();
        let Some(doc) = inner.doc.as_mut() else {
            return false;
        };
        if !doc.merge_layer_down(index) {
            return false;
        }
        inner.dirty_save = true;
        inner.invalidate_renderer();
        true
    }

    pub fn layer_has_transform(&self, index: usize) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .and_then(|doc| doc.layers.get(index))
            .is_some_and(|layer| layer.transform.is_some_and(|t| !t.is_identity()))
    }

    pub fn reset_layer_transform(&mut self, index: usize) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.reset_layer_transform(index);
            inner.dirty_save = true;
            inner.invalidate_renderer();
        }
    }

    pub fn can_move_layer_up(&self, index: usize) -> bool {
        self.inner.lock().doc.as_ref().is_some_and(|doc| {
            index + 1 < doc.layers.len() && !doc.layers.get(index).is_some_and(|l| l.is_paper())
        })
    }

    pub fn can_move_layer_down(&self, index: usize) -> bool {
        self.inner.lock().doc.as_ref().is_some_and(|doc| {
            if index == 0 || doc.layers.get(index).is_some_and(|l| l.is_paper()) {
                return false;
            }
            !doc.layers.get(index - 1).is_some_and(|l| l.is_paper())
        })
    }

    pub fn move_layer_up(&mut self, index: usize) -> bool {
        let mut inner = self.inner.lock();
        let Some(doc) = inner.doc.as_mut() else {
            return false;
        };
        if !doc.move_layer_up(index) {
            return false;
        }
        inner.dirty_save = true;
        inner.invalidate_renderer();
        true
    }

    pub fn move_layer_down(&mut self, index: usize) -> bool {
        let mut inner = self.inner.lock();
        let Some(doc) = inner.doc.as_mut() else {
            return false;
        };
        if !doc.move_layer_down(index) {
            return false;
        }
        inner.dirty_save = true;
        inner.invalidate_renderer();
        true
    }

    pub fn move_layer_row(&mut self, from_row: usize, to_row: usize) -> bool {
        let mut inner = self.inner.lock();
        let Some(doc) = inner.doc.as_mut() else {
            return false;
        };
        if !doc.move_layer_row(from_row, to_row) {
            return false;
        }
        inner.dirty_save = true;
        inner.invalidate_renderer();
        true
    }

    pub fn can_rename_layer(&self, index: usize) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .and_then(|doc| doc.layers.get(index))
            .is_some_and(|layer| !layer.is_paper())
    }

    pub fn set_layer_name(&mut self, index: usize, name: &str) -> bool {
        let mut inner = self.inner.lock();
        let Some(doc) = inner.doc.as_mut() else {
            return false;
        };
        if !doc.set_layer_name(index, name) {
            return false;
        }
        inner.dirty_save = true;
        inner.invalidate_renderer();
        true
    }

    pub fn layer_is_rasterizable(&self, index: usize) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .is_some_and(|doc| doc.layer_is_rasterizable(index))
    }

    pub fn rasterize_layer(&mut self, index: usize) -> bool {
        let mut inner = self.inner.lock();
        let Some(doc) = inner.doc.as_mut() else {
            return false;
        };
        if !doc.rasterize_layer(index) {
            return false;
        }
        inner.dirty_save = true;
        inner.invalidate_renderer();
        true
    }
}
