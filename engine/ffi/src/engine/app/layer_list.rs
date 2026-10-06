use super::Engine;

pub struct LayerSummary {
    pub index: usize,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub active: bool,
    pub is_paper: bool,
    pub clipped: bool,
    pub masked: bool,
    pub clip_base: bool,
}

impl Engine {
    pub fn layer_count(&self) -> u32 {
        self.inner
            .lock()
            .doc
            .as_ref()
            .map(|doc| doc.layers.len() as u32)
            .unwrap_or(0)
    }

    pub fn list_layers(&self) -> Vec<LayerSummary> {
        let inner = self.inner.lock();
        let doc = match inner.doc.as_ref() {
            Some(doc) => doc,
            None => return Vec::new(),
        };
        let active = doc.active_layer;
        doc.layers
            .iter()
            .enumerate()
            .rev()
            .map(|(index, layer)| LayerSummary {
                index,
                name: layer.name.clone(),
                visible: layer.visible,
                locked: layer.locked,
                active: index == active,
                is_paper: layer.is_paper(),
                clipped: layer.clips_to.is_some() && !layer.clip_invert,
                masked: layer.clips_to.is_some() && layer.clip_invert,
                clip_base: doc.is_layer_clip_base(index),
            })
            .collect()
    }

    pub fn set_active_layer(&mut self, index: usize) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_active_layer(index);
            inner.invalidate_renderer();
        }
    }

    pub fn add_layer(&mut self) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            let n = doc.layers.len();
            doc.add_layer(format!("Layer {}", n));
            inner.dirty_save = true;
            inner.invalidate_renderer();
        }
    }
}
