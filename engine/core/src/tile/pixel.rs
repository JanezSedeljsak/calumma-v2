//! Per-pixel RGBA arithmetic shared by every tile writer: compositing, blend modes, and the
//! uniform-tile helpers that let a solid fill share one allocation.

use super::*;

pub fn blend_over(dst: [u8; 4], src: [u8; 4]) -> [u8; 4] {
    let src_a = src[3] as u32;
    if src_a == 0 {
        return dst;
    }
    if src_a == ALPHA_MAX {
        return src;
    }
    let inv = ALPHA_MAX - src_a;
    let dst_a = dst[3] as u32;
    let out_scaled = src_a * ALPHA_MAX + dst_a * inv;
    if out_scaled == 0 {
        return [0; 4];
    }
    let bias = out_scaled / 2;
    let channel = |i: usize| {
        let numerator = src[i] as u32 * src_a * ALPHA_MAX + dst[i] as u32 * dst_a * inv;
        ((numerator + bias) / out_scaled) as u8
    };
    [
        channel(0),
        channel(1),
        channel(2),
        ((out_scaled + ALPHA_ROUND_BIAS) / ALPHA_MAX) as u8,
    ]
}

/// `src` composited over `dst` through `mode`: the source colour becomes
/// `(1 - αb)·Cs + αb·B(Cb, Cs)` — the blend only applies where there is a backdrop to blend
/// with — and is then laid over `dst` like any Normal pixel.
pub fn blend_with_mode(dst: [u8; 4], src: [u8; 4], mode: crate::layer::BlendMode) -> [u8; 4] {
    if mode == crate::layer::BlendMode::Normal || src[3] == 0 {
        return blend_over(dst, src);
    }
    let unit = |v: u8| v as f32 / ALPHA_MAX as f32;
    let backdrop = [unit(dst[0]), unit(dst[1]), unit(dst[2])];
    let source = [unit(src[0]), unit(src[1]), unit(src[2])];
    let backdrop_alpha = unit(dst[3]);
    let blended = crate::blend::blend_rgb(mode, backdrop, source);
    let mixed: [u8; 3] = std::array::from_fn(|i| {
        let v = (1.0 - backdrop_alpha) * source[i] + backdrop_alpha * blended[i];
        (v.clamp(0.0, 1.0) * ALPHA_MAX as f32).round() as u8
    });
    blend_over(dst, [mixed[0], mixed[1], mixed[2], src[3]])
}

pub fn unpremultiply_rgba(rgba: &mut [u8]) {
    rgba.par_chunks_mut(EFFECT_CHUNK_BYTES).for_each(|block| {
        for px in block.chunks_exact_mut(CHANNELS) {
            let alpha = px[3] as u32;
            if alpha == ALPHA_MAX {
                continue;
            }
            if alpha == 0 {
                px[0] = 0;
                px[1] = 0;
                px[2] = 0;
                continue;
            }
            for channel in px[..3].iter_mut() {
                let scaled = (*channel as u32 * ALPHA_MAX + alpha / 2) / alpha;
                *channel = scaled.min(ALPHA_MAX) as u8;
            }
        }
    });
}

pub(crate) fn uniform_tile(rgba: [u8; 4]) -> Vec<u8> {
    rgba.repeat(TILE_SIZE as usize * TILE_SIZE as usize)
}

/// The one color a tile is painted in, or `None` the moment a second one turns up. A mixed
/// tile — every tile with actual drawing in it — bails within the first few pixels, so asking
/// this of every tile on load costs nothing worth measuring, while the tiles it *does* answer
/// for are exactly the ones worth sharing.
pub fn uniform_color(pixels: &[u8]) -> Option<[u8; 4]> {
    let first: [u8; 4] = pixels.get(..CHANNELS)?.try_into().ok()?;
    pixels
        .chunks_exact(CHANNELS)
        .all(|px| px == first)
        .then_some(first)
}

#[inline]
pub(super) fn pixel_index(local_x: usize, local_y: usize) -> usize {
    (local_y * TILE_SIZE as usize + local_x) * CHANNELS
}
