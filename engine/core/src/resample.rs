//! Area (box) downsampling of a flat RGBA buffer.
//!
//! The engine's other resampler is `tile::nearest_source`, which the thumbnail and layer-preview
//! paths use and which is right for them: those are small, generated constantly, and nobody
//! studies them. Dropping a 4096-wide photo to 1000 through nearest-neighbour is a different
//! matter — it keeps one source pixel in sixteen and throws the rest away, which on anything
//! with fine detail reads as broken rather than as smaller.
//!
//! Two things this does that a naive average does not:
//!
//! - **Colour is weighted by alpha.** Straight-averaging unpremultiplied RGBA lets the colour
//!   of fully transparent pixels vote, which halos every edge with whatever happens to be
//!   sitting in the invisible part of the buffer.
//! - **The footprint covers every source pixel exactly once.** Each destination pixel takes
//!   the half-open source span `[x * src / dst, (x + 1) * src / dst)`, so nothing is sampled
//!   twice and nothing is skipped — that is what makes it an area filter rather than a blur.

use crate::limits::ALPHA_MAX;
use rayon::prelude::*;

const CHANNELS: usize = 4;

/// Scales `src` down to `dst_w` × `dst_h`. Upscaling is not this function's job — it returns
/// the source unchanged rather than inventing pixels, because every caller here is fitting
/// something oversized into something smaller.
pub fn box_downsample(src: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    let dst_w = dst_w.max(1);
    let dst_h = dst_h.max(1);
    if src_w == 0 || src_h == 0 || (dst_w >= src_w && dst_h >= src_h) {
        return src.to_vec();
    }
    let dst_row = (dst_w as usize) * CHANNELS;
    let mut out = vec![0u8; dst_row * (dst_h as usize)];
    out.par_chunks_mut(dst_row)
        .enumerate()
        .for_each(|(y, row)| {
            let (y0, y1) = span(y as u32, dst_h, src_h);
            for x in 0..dst_w as usize {
                let (x0, x1) = span(x as u32, dst_w, src_w);
                let px = average(src, src_w, (x0, x1), (y0, y1));
                row[x * CHANNELS..x * CHANNELS + CHANNELS].copy_from_slice(&px);
            }
        });
    out
}

/// The half-open source span one destination index covers, never empty.
fn span(index: u32, dst: u32, src: u32) -> (u32, u32) {
    let start = ((index as u64 * src as u64) / dst as u64) as u32;
    let end = (((index as u64 + 1) * src as u64) / dst as u64) as u32;
    (start, end.max(start + 1).min(src))
}

fn average(src: &[u8], src_w: u32, xs: (u32, u32), ys: (u32, u32)) -> [u8; 4] {
    let mut weighted = [0u64; 3];
    let mut alpha_sum = 0u64;
    let mut count = 0u64;
    for y in ys.0..ys.1 {
        let row = (y as usize) * (src_w as usize) * CHANNELS;
        for x in xs.0..xs.1 {
            let i = row + (x as usize) * CHANNELS;
            let Some(px) = src.get(i..i + CHANNELS) else {
                continue;
            };
            let a = px[3] as u64;
            weighted[0] += px[0] as u64 * a;
            weighted[1] += px[1] as u64 * a;
            weighted[2] += px[2] as u64 * a;
            alpha_sum += a;
            count += 1;
        }
    }
    if count == 0 {
        return [0; 4];
    }
    let alpha = (alpha_sum / count) as u8;
    if alpha_sum == 0 {
        return [0, 0, 0, 0];
    }
    [
        (weighted[0] / alpha_sum).min(ALPHA_MAX as u64) as u8,
        (weighted[1] / alpha_sum).min(ALPHA_MAX as u64) as u8,
        (weighted[2] / alpha_sum).min(ALPHA_MAX as u64) as u8,
        alpha,
    ]
}

/// The largest size that fits inside `max_w` × `max_h` without changing the aspect ratio.
/// Never returns zero on either axis — an image scaled to nothing is not a fit, it is a
/// different way of losing the paste.
pub fn fit_within(width: u32, height: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    if width <= max_w && height <= max_h {
        return (width, height);
    }
    let by_width = (max_w as f64) / (width as f64);
    let by_height = (max_h as f64) / (height as f64);
    let scale = by_width.min(by_height);
    (
        ((width as f64 * scale).round() as u32).clamp(1, max_w),
        ((height as f64 * scale).round() as u32).clamp(1, max_h),
    )
}
