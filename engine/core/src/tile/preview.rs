//! Downscaled reads of a grid: the layers-panel preview and the thumbnail.

use super::*;

/// A layer's cached picture of itself, cropped to its painted pixels and capped at
/// [`LAYER_PREVIEW_MAX_SIDE`] on the long side. Held behind an `Arc` so cloning a grid — which
/// history does — copies a pointer rather than up to a megabyte of pixels.
#[derive(Clone, Debug)]
pub struct Preview {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Preview {
    /// Point-samples down to `max_side` on the long side, or hands back a copy unchanged when
    /// the caller wants at least what is cached. The preview is already the crop, so every size
    /// derived from it frames the layer identically.
    pub fn scaled(&self, max_side: u32) -> (u32, u32, Vec<u8>) {
        let max_side = max_side.max(1);
        if self.width.max(self.height) <= max_side {
            return (self.width, self.height, self.rgba.clone());
        }
        let scale = (max_side as f32 / self.width as f32).min(max_side as f32 / self.height as f32);
        let tw = ((self.width as f32) * scale).round().max(1.0) as u32;
        let th = ((self.height as f32) * scale).round().max(1.0) as u32;
        let mut out = vec![0u8; (tw as usize) * (th as usize) * CHANNELS];
        for ty in 0..th {
            let sy = nearest_source(ty, th, self.height);
            for tx in 0..tw {
                let sx = nearest_source(tx, tw, self.width);
                let src = ((sy as usize) * (self.width as usize) + (sx as usize)) * CHANNELS;
                let dst = ((ty as usize) * (tw as usize) + (tx as usize)) * CHANNELS;
                out[dst..dst + CHANNELS].copy_from_slice(&self.rgba[src..src + CHANNELS]);
            }
        }
        (tw, th, out)
    }

    pub fn bytes(&self) -> usize {
        self.rgba.capacity()
    }
}

/// Maps output index `i` of `out_len` onto the source index it samples, spreading the samples
/// across the full source extent so the first and last output pixels land on the source's first
/// and last.
#[inline]
pub(super) fn nearest_source(i: u32, out_len: u32, src_len: u32) -> u32 {
    if out_len <= 1 {
        return 0;
    }
    let t = (i as f32) * ((src_len - 1) as f32) / ((out_len - 1) as f32);
    (t.round() as u32).min(src_len.saturating_sub(1))
}

/// The tile a resampling walk last looked up, and whether that coordinate held one at all — a
/// miss is worth remembering too, since the transparent parts of a crop come in runs like the
/// painted ones do.
pub(super) type TileCursor<'a> = Option<(TileCoord, Option<&'a Arc<Vec<u8>>>)>;

impl TileGrid {
    /// The cached [`Preview`] of this grid, rebuilt only when a tile has changed since the last
    /// time it was asked for. Every thumbnail the shell wants is a resample of this, so a layer
    /// is scanned at full resolution once per edit instead of once per request.
    pub fn preview(&mut self) -> Arc<Preview> {
        if self.preview.is_none() || !self.dirty[DirtyChannel::Preview.slot()].is_empty() {
            let (width, height, rgba) = self.thumbnail(LAYER_PREVIEW_MAX_SIDE);
            self.preview = Some(Arc::new(Preview {
                width,
                height,
                rgba,
            }));
            self.clear_dirty(DirtyChannel::Preview);
        }
        Arc::clone(self.preview.as_ref().expect("just rebuilt when missing"))
    }

    pub fn preview_bytes(&self) -> usize {
        self.preview.as_ref().map_or(0, |p| p.bytes())
    }

    /// Point-samples the grid down to `max_side`, cropped to its painted pixels. Prefer
    /// [`TileGrid::preview`] for anything the UI shows repeatedly — this walks the layer at full
    /// resolution every call.
    pub fn thumbnail(&self, max_side: u32) -> (u32, u32, Vec<u8>) {
        let max_side = max_side.max(1);
        let crop = self.opaque_bounds().unwrap_or(self.extent);
        let crop_w = (crop.max_x - crop.min_x + 1).max(1) as u32;
        let crop_h = (crop.max_y - crop.min_y + 1).max(1) as u32;
        let scale = (max_side as f32 / crop_w as f32)
            .min(max_side as f32 / crop_h as f32)
            .min(1.0);
        let tw = ((crop_w as f32) * scale).round().max(1.0) as u32;
        let th = ((crop_h as f32) * scale).round().max(1.0) as u32;
        let mut rgba = vec![0u8; (tw as usize) * (th as usize) * CHANNELS];
        let mut cursor = TileCursor::default();
        for ty in 0..th {
            let sy = crop.min_y + nearest_source(ty, th, crop_h) as i32;
            for tx in 0..tw {
                let sx = crop.min_x + nearest_source(tx, tw, crop_w) as i32;
                let px = self.sample_pixel(sx, sy, &mut cursor);
                let i = ((ty as usize) * (tw as usize) + (tx as usize)) * CHANNELS;
                rgba[i..i + CHANNELS].copy_from_slice(&px);
            }
        }
        (tw, th, rgba)
    }

    /// `get_pixel` with the last tile carried forward. Resampling walks the crop row-major, so
    /// run after run of samples land in the tile the one before them did — hashing the map for
    /// each of them is what made a 512² preview a quarter of a million probes.
    pub(super) fn sample_pixel<'a>(
        &'a self,
        x: i32,
        y: i32,
        cursor: &mut TileCursor<'a>,
    ) -> [u8; 4] {
        if !self.contains_doc_point(x, y) {
            return [0; 4];
        }
        let coord = TileCoord::from_doc_i32(x, y);
        let tile = match cursor {
            Some((at, tile)) if *at == coord => *tile,
            _ => {
                let found = self.tiles.get(&coord);
                *cursor = Some((coord, found));
                found
            }
        };
        let Some(tile) = tile else {
            return [0; 4];
        };
        let (ox, oy) = coord.origin();
        let i = pixel_index((x - ox) as usize, (y - oy) as usize);
        let mut out = [0u8; 4];
        out.copy_from_slice(&tile[i..i + CHANNELS]);
        out
    }
}
