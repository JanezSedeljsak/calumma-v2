//! Lanczos-3 resampling in premultiplied space, separable, with a flattened tap table shared
//! across every row (or column) of an axis.

use crate::blur::{premultiply, unpremultiply};
use rayon::prelude::*;

const LANCZOS_A: f32 = 3.0;

fn sinc(x: f32) -> f32 {
    if x.abs() < 1e-8 {
        1.0
    } else {
        let px = std::f32::consts::PI * x;
        px.sin() / px
    }
}

fn lanczos_kernel(x: f32) -> f32 {
    if x.abs() >= LANCZOS_A {
        0.0
    } else {
        sinc(x) * sinc(x / LANCZOS_A)
    }
}

struct AxisTaps {
    offsets: Vec<u32>,
    idx: Vec<u32>,
    weight: Vec<f32>,
}

impl AxisTaps {
    fn identity(len: u32) -> Self {
        let len = len.max(1) as usize;
        let mut offsets = Vec::with_capacity(len + 1);
        let mut idx = Vec::with_capacity(len);
        let mut weight = Vec::with_capacity(len);
        for i in 0..len {
            offsets.push(i as u32);
            idx.push(i as u32);
            weight.push(1.0);
        }
        offsets.push(len as u32);
        Self {
            offsets,
            idx,
            weight,
        }
    }

    fn len(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    fn range(&self, dst_i: usize) -> std::ops::Range<usize> {
        self.offsets[dst_i] as usize..self.offsets[dst_i + 1] as usize
    }
}

fn build_taps(src_len: u32, dst_len: u32) -> AxisTaps {
    let src_len = src_len.max(1);
    let dst_len = dst_len.max(1);
    if src_len == dst_len {
        return AxisTaps::identity(src_len);
    }
    let scale = src_len as f32 / dst_len as f32;
    let filter_scale = scale.max(1.0);
    let support = LANCZOS_A * filter_scale;
    let mut offsets = Vec::with_capacity(dst_len as usize + 1);
    let mut idx = Vec::new();
    let mut weight = Vec::new();
    offsets.push(0);
    for dst_i in 0..dst_len as i64 {
        let center = (dst_i as f32 + 0.5) * scale - 0.5;
        let lo = (center - support).floor() as i64;
        let hi = (center + support).ceil() as i64;
        let start = idx.len();
        let mut sum = 0.0f64;
        for si in lo..=hi {
            let w = lanczos_kernel((center - si as f32) / filter_scale) as f64;
            if w == 0.0 {
                continue;
            }
            let clamped = si.clamp(0, src_len as i64 - 1) as u32;
            if idx.len() > start && idx.last() == Some(&clamped) {
                if let Some(last_w) = weight.last_mut() {
                    *last_w += w as f32;
                    sum += w;
                    continue;
                }
            }
            idx.push(clamped);
            weight.push(w as f32);
            sum += w;
        }
        if sum.abs() > 1e-12 {
            let inv = (1.0 / sum) as f32;
            for w in &mut weight[start..] {
                *w *= inv;
            }
        }
        offsets.push(idx.len() as u32);
    }
    AxisTaps {
        offsets,
        idx,
        weight,
    }
}

type Pixel = [f32; 4];

fn resize_width(src: &[Pixel], src_w: usize, src_h: usize, taps: &AxisTaps) -> Vec<Pixel> {
    let dst_w = taps.len();
    let mut out = vec![[0f32; 4]; dst_w * src_h];
    out.par_chunks_mut(dst_w).enumerate().for_each(|(y, row)| {
        let src_row = &src[y * src_w..(y + 1) * src_w];
        for (x, dst_px) in row.iter_mut().enumerate() {
            let mut acc = [0f64; 4];
            for t in taps.range(x) {
                let p = src_row[taps.idx[t] as usize];
                let w = taps.weight[t] as f64;
                acc[0] += p[0] as f64 * w;
                acc[1] += p[1] as f64 * w;
                acc[2] += p[2] as f64 * w;
                acc[3] += p[3] as f64 * w;
            }
            *dst_px = [acc[0] as f32, acc[1] as f32, acc[2] as f32, acc[3] as f32];
        }
    });
    out
}

fn resize_height(src: &[Pixel], w: usize, taps: &AxisTaps) -> Vec<Pixel> {
    let dst_h = taps.len();
    let mut out = vec![[0f32; 4]; w * dst_h];
    out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, dst_px) in row.iter_mut().enumerate() {
            let mut acc = [0f64; 4];
            for t in taps.range(y) {
                let p = src[taps.idx[t] as usize * w + x];
                let wt = taps.weight[t] as f64;
                acc[0] += p[0] as f64 * wt;
                acc[1] += p[1] as f64 * wt;
                acc[2] += p[2] as f64 * wt;
                acc[3] += p[3] as f64 * wt;
            }
            *dst_px = [acc[0] as f32, acc[1] as f32, acc[2] as f32, acc[3] as f32];
        }
    });
    out
}

pub fn lanczos3_resize(rgba: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    let (dst_w, dst_h) = (dst_w.max(1), dst_h.max(1));
    if src_w == dst_w && src_h == dst_h {
        return rgba.to_vec();
    }
    let mut buf: Vec<Pixel> = rgba
        .par_chunks_exact(4)
        .map(|p| premultiply([p[0], p[1], p[2], p[3]]))
        .collect();
    let mut cur_w = src_w as usize;
    if src_w != dst_w {
        let x_taps = build_taps(src_w, dst_w);
        buf = resize_width(&buf, cur_w, src_h as usize, &x_taps);
        cur_w = dst_w as usize;
    }
    if src_h != dst_h {
        let y_taps = build_taps(src_h, dst_h);
        buf = resize_height(&buf, cur_w, &y_taps);
    }
    let mut out = vec![0u8; (dst_w as usize) * (dst_h as usize) * 4];
    out.par_chunks_exact_mut(4)
        .zip(buf.par_iter())
        .for_each(|(dst_px, &p)| dst_px.copy_from_slice(&unpremultiply(p)));
    out
}

pub fn box_downsample(rgba: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    let (dst_w, dst_h) = (dst_w.max(1), dst_h.max(1));
    let (src_w, src_h) = (src_w.max(1), src_h.max(1));
    if src_w == dst_w && src_h == dst_h {
        return rgba.to_vec();
    }
    let mut out = vec![0u8; (dst_w as usize) * (dst_h as usize) * 4];
    let src_w = src_w as usize;
    let src_h = src_h as usize;
    let dst_w_us = dst_w as usize;
    let dst_h_us = dst_h as usize;
    out.par_chunks_mut(dst_w_us * 4)
        .enumerate()
        .for_each(|(y, row)| {
            let y0 = y * src_h / dst_h_us;
            let y1 = ((y + 1) * src_h / dst_h_us).max(y0 + 1).min(src_h);
            for x in 0..dst_w_us {
                let x0 = x * src_w / dst_w_us;
                let x1 = ((x + 1) * src_w / dst_w_us).max(x0 + 1).min(src_w);
                let mut acc = [0.0f64; 4];
                let mut n = 0.0f64;
                for sy in y0..y1 {
                    for sx in x0..x1 {
                        let i = (sy * src_w + sx) * 4;
                        let p = premultiply([rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]);
                        acc[0] += p[0] as f64;
                        acc[1] += p[1] as f64;
                        acc[2] += p[2] as f64;
                        acc[3] += p[3] as f64;
                        n += 1.0;
                    }
                }
                let o = x * 4;
                if n > 0.0 {
                    let px = unpremultiply([
                        (acc[0] / n) as f32,
                        (acc[1] / n) as f32,
                        (acc[2] / n) as f32,
                        (acc[3] / n) as f32,
                    ]);
                    row[o..o + 4].copy_from_slice(&px);
                }
            }
        });
    out
}
