use crate::document::{layer_source_pixel, Document};
use crate::layer::Layer;
use crate::limits::{ALPHA_MAX, ALPHA_ROUND_BIAS};
use crate::tile::DirtyChannel;
use crate::transform::bounds_center;
use rayon::prelude::*;

pub fn multiply_clip_alpha(src: u8, base_raw: u8) -> u8 {
    if src == 0 || base_raw == 0 {
        return 0;
    }
    ((src as u32 * base_raw as u32 + ALPHA_ROUND_BIAS) / ALPHA_MAX) as u8
}

pub fn base_raw_alpha_at(base: &Layer, doc_x: f32, doc_y: f32) -> u8 {
    layer_source_pixel(base, doc_x, doc_y)[3]
}

pub fn clip_base_alpha_for_layer_pixel(layer: &Layer, base: &Layer, x: i32, y: i32) -> u8 {
    let (doc_x, doc_y) = tile_pixel_display_doc(layer, x, y);
    base_raw_alpha_at(base, doc_x, doc_y)
}

pub fn tile_pixel_display_doc(layer: &Layer, x: i32, y: i32) -> (f32, f32) {
    let px = x as f32 + 0.5;
    let py = y as f32 + 0.5;
    let Some(t) = layer.transform.filter(|t| !t.is_identity()) else {
        return (px, py);
    };
    let Some(bounds) = layer.content_bounds() else {
        return (px, py);
    };
    let pivot = bounds_center(bounds);
    t.forward(pivot, (px, py))
}

pub fn apply_clip_alpha_to_buffer(buf: &mut [u8], base: &Layer, w: u32, _h: u32) {
    let row_bytes = (w as usize) * 4;
    buf.par_chunks_mut(row_bytes)
        .enumerate()
        .for_each(|(y, row)| {
            let doc_y = y as f32 + 0.5;
            for x in 0..w as usize {
                let px = &mut row[x * 4..x * 4 + 4];
                if px[3] == 0 {
                    continue;
                }
                let base_alpha = base_raw_alpha_at(base, x as f32 + 0.5, doc_y);
                px[3] = multiply_clip_alpha(px[3], base_alpha);
            }
        });
}

impl Document {
    pub fn clip_base_for_layer<'a>(&'a self, layer: &'a Layer) -> Option<&'a Layer> {
        let base_id = layer.clips_to.as_deref()?;
        self.layers.iter().find(|l| l.id == base_id)
    }

    pub fn is_layer_clipped(&self, index: usize) -> bool {
        self.layers.get(index).is_some_and(|l| l.clips_to.is_some())
    }

    pub fn is_layer_clip_base(&self, index: usize) -> bool {
        if index + 1 >= self.layers.len() {
            return false;
        }
        let base_id = &self.layers[index].id;
        self.layers[index + 1]
            .clips_to
            .as_deref()
            .is_some_and(|id| id == base_id)
    }

    pub fn clip_pair_locked(&self, index: usize) -> bool {
        let Some(layer) = self.layers.get(index) else {
            return false;
        };
        if layer.locked {
            return true;
        }
        if index > 0 {
            let base = &self.layers[index - 1];
            if layer.clips_to.as_deref() == Some(base.id.as_str()) && base.locked {
                return true;
            }
        }
        if let Some(above) = self.layers.get(index + 1) {
            if above.clips_to.as_deref() == Some(layer.id.as_str()) && above.locked {
                return true;
            }
        }
        false
    }

    pub fn can_create_clipping_mask(&self, index: usize) -> bool {
        if index == 0 || index >= self.layers.len() {
            return false;
        }
        if self.is_layer_clipped(index) {
            return false;
        }
        if self.is_layer_clipped(index - 1) {
            return false;
        }
        let base = &self.layers[index - 1];
        if base.is_paper() || !base.is_raster() || base.locked {
            return false;
        }
        let source = &self.layers[index];
        source.is_raster() && !source.locked
    }

    pub fn can_release_clipping_mask(&self, index: usize) -> bool {
        self.is_layer_clipped(index) && !self.clip_pair_locked(index)
    }

    pub fn create_clipping_mask(&mut self, index: usize) -> bool {
        if !self.can_create_clipping_mask(index) {
            return false;
        }
        self.record_layer_props_history(index);
        let base_id = self.layers[index - 1].id.clone();
        self.layers[index].clips_to = Some(base_id);
        self.mark_layer_render_dirty_for_clip(index);
        true
    }

    pub fn release_clipping_mask(&mut self, index: usize) -> bool {
        if !self.can_release_clipping_mask(index) {
            return false;
        }
        self.record_layer_props_history(index);
        self.layers[index].clips_to = None;
        self.mark_layer_render_dirty_for_clip(index);
        true
    }

    pub(crate) fn validate_clip_links(&mut self) {
        for i in 0..self.layers.len() {
            let Some(base_id) = self.layers[i].clips_to.clone() else {
                continue;
            };
            let valid = self
                .layers
                .iter()
                .position(|l| l.id == base_id)
                .is_some_and(|base_idx| base_idx == i - 1);
            if !valid {
                self.layers[i].clips_to = None;
            }
        }
    }

    pub fn mark_clip_dependents_render_dirty(&mut self, base_index: usize) {
        let Some(base_id) = self.layers.get(base_index).map(|l| l.id.clone()) else {
            return;
        };
        for layer in &mut self.layers {
            if layer.clips_to.as_deref() == Some(base_id.as_str()) {
                mark_layer_clip_render_dirty(layer);
            }
        }
    }

    pub fn clipped_layer_thumbnail(
        &self,
        index: usize,
        max_side: u32,
    ) -> Option<(u32, u32, Vec<u8>)> {
        let layer = self.layers.get(index)?;
        layer.clips_to.as_ref()?;
        let dw = self.width.max(1);
        let dh = self.height.max(1);
        let max_side = max_side.max(1);
        let scale = (max_side as f32 / dw as f32)
            .min(max_side as f32 / dh as f32)
            .min(1.0);
        let tw = ((dw as f32) * scale).round().max(1.0) as u32;
        let th = ((dh as f32) * scale).round().max(1.0) as u32;
        let mut rgba = vec![0u8; (tw as usize) * (th as usize) * 4];
        let mut min_x = tw;
        let mut min_y = th;
        let mut max_x = 0u32;
        let mut max_y = 0u32;
        let base = self.clip_base_for_layer(layer);
        for ty in 0..th {
            let doc_y = (ty as f32 + 0.5) * (dh as f32 / th as f32);
            for tx in 0..tw {
                let doc_x = (tx as f32 + 0.5) * (dw as f32 / tw as f32);
                let mut px = crate::document::layer_source_pixel(layer, doc_x, doc_y);
                if px[3] == 0 {
                    continue;
                }
                if let Some(base) = base {
                    px[3] = multiply_clip_alpha(px[3], base_raw_alpha_at(base, doc_x, doc_y));
                }
                if px[3] == 0 {
                    continue;
                }
                let i = ((ty * tw + tx) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&px);
                min_x = min_x.min(tx);
                min_y = min_y.min(ty);
                max_x = max_x.max(tx);
                max_y = max_y.max(ty);
            }
        }
        if min_x > max_x {
            return Some((1, 1, vec![0; 4]));
        }
        let cw = max_x - min_x + 1;
        let ch = max_y - min_y + 1;
        if cw == tw && ch == th {
            return Some((tw, th, rgba));
        }
        let mut cropped = vec![0u8; (cw as usize) * (ch as usize) * 4];
        let row = (cw * 4) as usize;
        for y in 0..ch {
            let src = (((min_y + y) * tw + min_x) * 4) as usize;
            let dst = (y as usize) * row;
            cropped[dst..dst + row].copy_from_slice(&rgba[src..src + row]);
        }
        Some((cw, ch, cropped))
    }

    pub fn schedule_clip_recalc_for_indices(&mut self, indices: &[usize]) {
        let mut mark = std::collections::HashSet::new();
        for &index in indices {
            if index >= self.layers.len() {
                continue;
            }
            if self.is_layer_clipped(index) {
                mark.insert(index);
            }
            let base_id = self.layers[index].id.clone();
            for (i, layer) in self.layers.iter().enumerate() {
                if layer.clips_to.as_deref() == Some(base_id.as_str()) {
                    mark.insert(i);
                }
            }
        }
        for index in mark {
            if let Some(layer) = self.layers.get_mut(index) {
                mark_layer_clip_render_dirty(layer);
            }
        }
    }

    fn mark_layer_render_dirty_for_clip(&mut self, index: usize) {
        if let Some(layer) = self.layers.get_mut(index) {
            mark_layer_clip_render_dirty(layer);
        }
    }
}

fn mark_layer_clip_render_dirty(layer: &mut Layer) {
    if let Some(tiles) = layer.tiles_mut() {
        tiles.mark_channel_dirty(DirtyChannel::Render);
    }
}
