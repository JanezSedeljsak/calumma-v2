use super::Document;

impl Document {
    pub fn begin_background_removal(&mut self, layer_id: String) {
        self.background_removal = Some(layer_id);
    }

    pub fn clear_background_removal(&mut self) {
        self.background_removal = None;
    }

    pub fn clear_background_removal_for(&mut self, layer_id: &str) {
        if self.background_removal.as_deref() == Some(layer_id) {
            self.background_removal = None;
        }
    }

    pub fn background_removal_animating(&self) -> bool {
        self.background_removal.is_some()
    }

    /// The layer the sweep should cross, and the same corners the hover outline uses, so the
    /// band and the dashed frame describe one box.
    pub fn background_removal_target(&self) -> Option<(usize, [(f32, f32); 4])> {
        let id = self.background_removal.as_deref()?;
        let index = self.layers.iter().position(|layer| layer.id == id)?;
        let corners = self.layer_outline_corners(index)?;
        Some((index, corners))
    }
}
