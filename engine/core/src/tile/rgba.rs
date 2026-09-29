//! Moving whole images in and out of a grid as flat RGBA, and reading between pixels.

use super::*;

impl TileGrid {
    pub fn blit_rgba(&mut self, rgba: &[u8], width: u32, height: u32) -> usize {
        self.blit_rgba_at(rgba, width, height, 0, 0)
    }

    pub fn blit_rgba_at(
        &mut self,
        rgba: &[u8],
        width: u32,
        height: u32,
        offset_x: i32,
        offset_y: i32,
    ) -> usize {
        if width == 0 || height == 0 {
            return 0;
        }
        let rect = DocRect::new(
            offset_x,
            offset_y,
            offset_x + width as i32 - 1,
            offset_y + height as i32 - 1,
        );
        self.paint_rect(rect, |x, y, _| {
            let sx = x - offset_x;
            let sy = y - offset_y;
            let i = ((sy as usize) * (width as usize) + sx as usize) * CHANNELS;
            let px = rgba.get(i..i + CHANNELS)?;
            if px[3] == 0 {
                return None;
            }
            Some([px[0], px[1], px[2], px[3]])
        })
    }

    pub fn copy_into_rgba(&self, rgba: &mut [u8], width: u32, height: u32) {
        let doc = DocRect::from_size(width, height);
        for (coord, pixels) in self.iter() {
            let (ox, oy) = coord.origin();
            let ts = TILE_SIZE as i32;
            let Some(span) = doc.intersect(DocRect::new(ox, oy, ox + ts - 1, oy + ts - 1)) else {
                continue;
            };
            for y in span.min_y..=span.max_y {
                let src_row = pixel_index(0, (y - oy) as usize);
                let src_start = src_row + ((span.min_x - ox) as usize) * CHANNELS;
                let run = ((span.max_x - span.min_x + 1) as usize) * CHANNELS;
                let dst_start = ((y as usize) * (width as usize) + span.min_x as usize) * CHANNELS;
                rgba[dst_start..dst_start + run]
                    .copy_from_slice(&pixels[src_start..src_start + run]);
            }
        }
    }

    /// Copy an arbitrary document-space rectangle out into a tightly packed RGBA buffer,
    /// `rect` wide and transparent wherever the grid has no tile. Row-at-a-time per tile, the
    /// same way `copy_into_rgba` walks the whole grid — a read-modify-write brush needs its
    /// neighbourhood in one flat buffer, and doing that with `get_pixel` would be a hash
    /// lookup per pixel.
    ///
    /// Pixels of `rect` that fall outside the document are left transparent rather than
    /// clamped; the caller decides what a document edge means to it.
    pub fn copy_rect_rgba(&self, rect: DocRect) -> Vec<u8> {
        let width = (rect.max_x - rect.min_x + 1).max(0) as usize;
        let height = (rect.max_y - rect.min_y + 1).max(0) as usize;
        let mut out = vec![0u8; width * height * CHANNELS];
        if width == 0 || height == 0 {
            return out;
        }
        let Some(span) = rect.intersect(self.bounds()) else {
            return out;
        };
        let ts = TILE_SIZE as i32;
        let (tx0, ty0, tx1, ty1) = span.tile_span();
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                let coord = TileCoord { x: tx, y: ty };
                let Some(pixels) = self.tiles.get(&coord) else {
                    continue;
                };
                let (ox, oy) = coord.origin();
                let Some(cell) = span.intersect(DocRect::new(ox, oy, ox + ts - 1, oy + ts - 1))
                else {
                    continue;
                };
                let run = ((cell.max_x - cell.min_x + 1) as usize) * CHANNELS;
                for y in cell.min_y..=cell.max_y {
                    let src = pixel_index((cell.min_x - ox) as usize, (y - oy) as usize);
                    let dst = (((y - rect.min_y) as usize) * width
                        + (cell.min_x - rect.min_x) as usize)
                        * CHANNELS;
                    out[dst..dst + run].copy_from_slice(&pixels[src..src + run]);
                }
            }
        }
        out
    }

    /// A 4-tap bilinear read around `(x, y)`, blended in premultiplied space so a sample
    /// straddling a transparency edge does not drag transparent black into the color channels —
    /// the same reasoning `blur.rs` documents for why tiles get premultiplied before any
    /// interpolation runs on them. `get_pixel` already answers `[0; 4]` outside the grid's
    /// extent, so a sample near an edge blends smoothly toward transparent instead of a hard
    /// nearest-neighbor snap.
    pub fn sample_bilinear(&self, x: f32, y: f32) -> [u8; 4] {
        let x0 = x.floor();
        let y0 = y.floor();
        let (fx, fy) = (x - x0, y - y0);
        let (ix0, iy0) = (x0 as i32, y0 as i32);
        let p00 = premultiply(self.get_pixel(ix0, iy0));
        let p10 = premultiply(self.get_pixel(ix0 + 1, iy0));
        let p01 = premultiply(self.get_pixel(ix0, iy0 + 1));
        let p11 = premultiply(self.get_pixel(ix0 + 1, iy0 + 1));
        let lerp = |a: [f32; 4], b: [f32; 4], t: f32| {
            [
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
                a[3] + (b[3] - a[3]) * t,
            ]
        };
        unpremultiply(lerp(lerp(p00, p10, fx), lerp(p01, p11, fx), fy))
    }
}
