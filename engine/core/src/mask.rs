use crate::clip::{apply_clip_alpha, clip_base_alpha_for_layer_pixel};
use crate::document::Document;
use crate::layer::Layer;
use crate::names;
use crate::tile::{DirtyChannel, TILE_SIZE};

impl Document {
    pub fn can_create_layer_mask(&self, index: usize) -> bool {
        let Some(layer) = self.layers.get(index) else {
            return false;
        };
        if layer.is_paper() || layer.locked || !layer.is_raster() {
            return false;
        }
        if layer.clips_to.is_some() || self.is_layer_mask_base(index) {
            return false;
        }
        true
    }

    pub fn can_release_layer_mask(&self, index: usize) -> bool {
        index > 0 && self.is_layer_masked(index) && !self.clip_pair_locked(index)
    }

    pub fn can_apply_layer_mask(&self, index: usize) -> bool {
        if !self.can_release_layer_mask(index) {
            return false;
        }
        let mask = &self.layers[index - 1];
        let painted = &self.layers[index];
        mask.is_raster() && painted.is_raster()
    }

    pub fn create_layer_mask(&mut self, index: usize) -> bool {
        if !self.can_create_layer_mask(index) {
            return false;
        }
        self.commit_text();
        self.record_stack_history();
        let mask = Layer::new(next_mask_name(&self.layers), self.width, self.height);
        let mask_id = mask.id.clone();
        self.layers.insert(index, mask);
        self.layers[index + 1].clips_to = Some(mask_id);
        self.layers[index + 1].clip_invert = true;
        self.active_layer = index + 1;
        self.mark_layer_render_dirty_for_mask(index + 1);
        true
    }

    pub fn release_layer_mask(&mut self, index: usize) -> bool {
        if !self.can_release_layer_mask(index) {
            return false;
        }
        self.record_stack_history();
        self.layers[index].clips_to = None;
        self.layers[index].clip_invert = false;
        self.mark_layer_render_dirty_for_mask(index);
        self.remove_layer_inner(index - 1, false);
        true
    }

    pub fn apply_layer_mask(&mut self, index: usize) -> bool {
        if !self.can_apply_layer_mask(index) {
            return false;
        }
        self.record_stack_history();
        {
            let (below, above) = self.layers.split_at_mut(index);
            punch_inverted(&mut above[0], &below[index - 1]);
            above[0].clips_to = None;
            above[0].clip_invert = false;
        }
        self.remove_layer_inner(index - 1, false);
        true
    }

    fn mark_layer_render_dirty_for_mask(&mut self, index: usize) {
        if let Some(layer) = self.layers.get_mut(index) {
            if let Some(tiles) = layer.tiles_mut() {
                tiles.mark_channel_dirty(DirtyChannel::Render);
            }
        }
    }
}

fn next_mask_name(layers: &[Layer]) -> String {
    let n = layers
        .iter()
        .filter(|layer| layer.name == names::MASK || layer.name.starts_with("Mask "))
        .count()
        + 1;
    names::numbered_mask(n)
}

fn punch_inverted(layer: &mut Layer, mask: &Layer) {
    let Some(grid) = layer.tiles() else {
        return;
    };
    let coords: Vec<_> = grid.coords().collect();
    for coord in coords {
        let Some(pixels) = layer.tiles().and_then(|grid| grid.get(coord)) else {
            continue;
        };
        let mut out = pixels.as_slice().to_vec();
        let (ox, oy) = coord.origin();
        for ty in 0..TILE_SIZE {
            for tx in 0..TILE_SIZE {
                let i = ((ty * TILE_SIZE + tx) * 4) as usize;
                if out[i + 3] == 0 {
                    continue;
                }
                let x = ox + tx as i32;
                let y = oy + ty as i32;
                let base_alpha = clip_base_alpha_for_layer_pixel(layer, mask, x, y);
                out[i + 3] = apply_clip_alpha(out[i + 3], base_alpha, true);
            }
        }
        if let Some(dst) = layer.tiles_mut().and_then(|grid| grid.ensure_mut(coord)) {
            dst.copy_from_slice(&out);
        }
    }
}
