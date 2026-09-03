use crate::document::Document;

impl Document {
    pub fn set_layer_selection(&mut self, indices: &[usize]) {
        self.layer_selection.clear();
        for &index in indices {
            if self.layer_movable(index) && !self.layer_selection.contains(&index) {
                self.layer_selection.push(index);
            }
        }
    }

    pub fn offset_layers(&mut self, indices: &[usize], dx: f32, dy: f32) -> bool {
        let mut moved = false;
        for &index in indices {
            let Some(layer) = self.layers.get_mut(index) else {
                continue;
            };
            if layer.is_paper() || layer.locked {
                continue;
            }
            if layer.content_bounds().is_none() {
                continue;
            }
            let mut t = layer.transform.unwrap_or_default();
            t.offset_x += dx;
            t.offset_y += dy;
            layer.transform = Some(t.clamped());
            moved = true;
        }
        moved
    }

    pub(crate) fn layer_movable(&self, index: usize) -> bool {
        self.layers.get(index).is_some_and(|layer| {
            !layer.is_paper() && !layer.locked && layer.content_bounds().is_some()
        })
    }

    pub(crate) fn movable_selection_for_click(&self, clicked: usize) -> Vec<usize> {
        let candidates =
            if self.layer_selection.len() > 1 && self.layer_selection.contains(&clicked) {
                self.layer_selection.clone()
            } else {
                vec![clicked]
            };
        candidates
            .into_iter()
            .filter(|&index| self.layer_movable(index))
            .collect()
    }

    pub(crate) fn nudge_layer_indices(&self) -> Vec<usize> {
        if self.layer_selection.len() > 1 {
            self.layer_selection
                .iter()
                .copied()
                .filter(|&index| self.layer_movable(index))
                .collect()
        } else if self.layer_movable(self.active_layer) {
            vec![self.active_layer]
        } else {
            Vec::new()
        }
    }
}
