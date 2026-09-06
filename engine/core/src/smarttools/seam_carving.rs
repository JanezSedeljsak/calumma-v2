//! Content-aware resize: lowest-energy vertical seams, then the same on the transpose.
//! Energy is Sobel on luminance, alpha, and chroma so isoluminant colour edges stay expensive;
//! the DP uses forward energy so removing a seam also prices the edges that removal would create.
//! Inserted seams are penalized so the next pass does not keep duplicating the same path.

use crate::blur::{premultiply, unpremultiply};
use rayon::prelude::*;

const SOBEL_X: [f32; 9] = [-1.0, 0.0, 1.0, -2.0, 0.0, 2.0, -1.0, 0.0, 1.0];
const SOBEL_Y: [f32; 9] = [-1.0, -2.0, -1.0, 0.0, 0.0, 0.0, 1.0, 2.0, 1.0];
const INSERT_PENALTY: f32 = 1.0e5;
const PAR_WIDTH: usize = 96;
const DIAGONAL_STEP: f32 = 2.0;

fn luminance(px: [u8; 4]) -> f32 {
    0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32
}

fn sample(field: &[f32], w: usize, h: usize, x: i64, y: i64) -> f32 {
    let cx = x.clamp(0, w as i64 - 1) as usize;
    let cy = y.clamp(0, h as i64 - 1) as usize;
    field[cy * w + cx]
}

fn sobel_at(field: &[f32], w: usize, h: usize, x: usize, y: usize) -> f32 {
    let mut gx = 0.0f32;
    let mut gy = 0.0f32;
    let mut i = 0;
    for dy in -1i64..=1 {
        for dx in -1i64..=1 {
            let v = sample(field, w, h, x as i64 + dx, y as i64 + dy);
            gx += SOBEL_X[i] * v;
            gy += SOBEL_Y[i] * v;
            i += 1;
        }
    }
    (gx * gx + gy * gy).sqrt()
}

fn energy_at(
    luma: &[f32],
    alpha: &[f32],
    chroma: &[f32],
    w: usize,
    h: usize,
    x: usize,
    y: usize,
) -> f32 {
    sobel_at(luma, w, h, x, y) + sobel_at(alpha, w, h, x, y) + 0.35 * sobel_at(chroma, w, h, x, y)
}

fn chroma_of(px: [u8; 4]) -> f32 {
    let r = px[0] as f32;
    let g = px[1] as f32;
    let b = px[2] as f32;
    (r - g).abs() + (g - b).abs() + (b - r).abs()
}

pub fn energy_map(rgba: &[u8], w: u32, h: u32) -> Vec<f32> {
    let (w, h) = (w as usize, h as usize);
    let luma: Vec<f32> = rgba
        .par_chunks_exact(4)
        .map(|p| luminance([p[0], p[1], p[2], p[3]]))
        .collect();
    let alpha: Vec<f32> = rgba
        .par_iter()
        .skip(3)
        .step_by(4)
        .map(|&a| a as f32)
        .collect();
    let chroma: Vec<f32> = rgba
        .par_chunks_exact(4)
        .map(|p| chroma_of([p[0], p[1], p[2], p[3]]))
        .collect();
    let mut out = vec![0f32; w * h];
    fill_energy(&mut out, &luma, &alpha, &chroma, w, h);
    out
}

fn fill_energy(out: &mut [f32], luma: &[f32], alpha: &[f32], chroma: &[f32], w: usize, h: usize) {
    out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, cell) in row.iter_mut().enumerate() {
            let e = energy_at(luma, alpha, chroma, w, h, x, y);
            *cell = if e.is_finite() { e } else { 0.0 };
        }
    });
}

fn pix_diff(luma: &[f32], alpha: &[f32], chroma: &[f32], i: usize, j: usize) -> f32 {
    (luma[i] - luma[j]).abs() + (alpha[i] - alpha[j]).abs() + 0.35 * (chroma[i] - chroma[j]).abs()
}

fn cost_up(luma: &[f32], alpha: &[f32], chroma: &[f32], w: usize, x: usize, y: usize) -> f32 {
    if x == 0 || x + 1 >= w {
        0.0
    } else {
        let i = y * w + x;
        pix_diff(luma, alpha, chroma, i - 1, i + 1)
    }
}

fn cost_left(luma: &[f32], alpha: &[f32], chroma: &[f32], w: usize, x: usize, y: usize) -> f32 {
    let mut c = cost_up(luma, alpha, chroma, w, x, y);
    if y > 0 && x > 0 {
        c += pix_diff(luma, alpha, chroma, (y - 1) * w + x, y * w + x - 1);
    }
    c
}

fn cost_right(luma: &[f32], alpha: &[f32], chroma: &[f32], w: usize, x: usize, y: usize) -> f32 {
    let mut c = cost_up(luma, alpha, chroma, w, x, y);
    if y > 0 && x + 1 < w {
        c += pix_diff(luma, alpha, chroma, (y - 1) * w + x, y * w + x + 1);
    }
    c
}

fn accumulate(
    energy: &[f32],
    luma: &[f32],
    alpha: &[f32],
    chroma: &[f32],
    w: usize,
    h: usize,
) -> (Vec<f32>, Vec<i8>) {
    let mut parent = vec![0i8; w * h];
    let mut prev = energy[..w].to_vec();
    let mut curr = vec![0f32; w];
    for y in 1..h {
        let row_parent = &mut parent[y * w..(y + 1) * w];
        let e_row = &energy[y * w..(y + 1) * w];
        let body = |x: usize, cell: &mut f32, par: &mut i8| {
            let up = prev[x] + cost_up(luma, alpha, chroma, w, x, y);
            let left = if x > 0 {
                prev[x - 1] + cost_left(luma, alpha, chroma, w, x, y) + DIAGONAL_STEP
            } else {
                f32::INFINITY
            };
            let right = if x + 1 < w {
                prev[x + 1] + cost_right(luma, alpha, chroma, w, x, y) + DIAGONAL_STEP
            } else {
                f32::INFINITY
            };
            if left <= up && left <= right {
                *par = -1;
                *cell = e_row[x] + left;
            } else if up <= right {
                *par = 0;
                *cell = e_row[x] + up;
            } else {
                *par = 1;
                *cell = e_row[x] + right;
            }
        };
        if w >= PAR_WIDTH {
            curr.par_iter_mut()
                .zip(row_parent.par_iter_mut())
                .enumerate()
                .for_each(|(x, (cell, par))| body(x, cell, par));
        } else {
            for x in 0..w {
                body(x, &mut curr[x], &mut row_parent[x]);
            }
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    (prev, parent)
}

fn find_seam(last: &[f32], parent: &[i8], w: usize, h: usize) -> Vec<usize> {
    let mut x = (0..w)
        .min_by(|&a, &b| last[a].total_cmp(&last[b]))
        .unwrap_or(0);
    let mut seam = vec![0usize; h];
    seam[h - 1] = x;
    for y in (1..h).rev() {
        let off = parent[y * w + x] as isize;
        x = (x as isize + off).clamp(0, w as isize - 1) as usize;
        seam[y - 1] = x;
    }
    seam
}

fn inserted_pixel(row: &[u8], cut: usize, w: usize) -> [u8; 4] {
    if cut == 0 || w == 1 {
        return row[0..4].try_into().unwrap();
    }
    let a = premultiply(row[(cut - 1) * 4..cut * 4].try_into().unwrap());
    let b = premultiply(row[cut * 4..cut * 4 + 4].try_into().unwrap());
    unpremultiply([
        (a[0] + b[0]) * 0.5,
        (a[1] + b[1]) * 0.5,
        (a[2] + b[2]) * 0.5,
        (a[3] + b[3]) * 0.5,
    ])
}

struct Workspace {
    rgba: Vec<u8>,
    luma: Vec<f32>,
    alpha: Vec<f32>,
    chroma: Vec<f32>,
    energy: Vec<f32>,
    w: usize,
    h: usize,
}

impl Workspace {
    fn new(rgba: Vec<u8>, w: usize, h: usize) -> Self {
        let luma: Vec<f32> = rgba
            .par_chunks_exact(4)
            .map(|p| luminance([p[0], p[1], p[2], p[3]]))
            .collect();
        let alpha: Vec<f32> = rgba
            .par_iter()
            .skip(3)
            .step_by(4)
            .map(|&a| a as f32)
            .collect();
        let chroma: Vec<f32> = rgba
            .par_chunks_exact(4)
            .map(|p| chroma_of([p[0], p[1], p[2], p[3]]))
            .collect();
        let mut energy = vec![0f32; w * h];
        fill_energy(&mut energy, &luma, &alpha, &chroma, w, h);
        Self {
            rgba,
            luma,
            alpha,
            chroma,
            energy,
            w,
            h,
        }
    }

    fn pick_seam(&self) -> Vec<usize> {
        let (last, parent) = accumulate(
            &self.energy,
            &self.luma,
            &self.alpha,
            &self.chroma,
            self.w,
            self.h,
        );
        find_seam(&last, &parent, self.w, self.h)
    }

    fn compact_row<T: Copy>(dst: &mut [T], src: &[T], cut: usize) {
        dst[..cut].copy_from_slice(&src[..cut]);
        dst[cut..].copy_from_slice(&src[cut + 1..]);
    }

    fn remove_seam(&mut self, seam: &[usize]) {
        let new_w = self.w - 1;
        let h = self.h;
        let mut rgba = vec![0u8; new_w * h * 4];
        let mut luma = vec![0f32; new_w * h];
        let mut alpha = vec![0f32; new_w * h];
        let mut chroma = vec![0f32; new_w * h];
        let mut energy = vec![0f32; new_w * h];
        rgba.par_chunks_mut(new_w * 4)
            .zip(luma.par_chunks_mut(new_w))
            .zip(alpha.par_chunks_mut(new_w))
            .zip(chroma.par_chunks_mut(new_w))
            .zip(energy.par_chunks_mut(new_w))
            .enumerate()
            .for_each(
                |(y, ((((row, luma_row), alpha_row), chroma_row), energy_row))| {
                    let cut = seam[y];
                    let src = &self.rgba[y * self.w * 4..(y + 1) * self.w * 4];
                    row[..cut * 4].copy_from_slice(&src[..cut * 4]);
                    row[cut * 4..].copy_from_slice(&src[(cut + 1) * 4..]);
                    Self::compact_row(luma_row, &self.luma[y * self.w..(y + 1) * self.w], cut);
                    Self::compact_row(alpha_row, &self.alpha[y * self.w..(y + 1) * self.w], cut);
                    Self::compact_row(chroma_row, &self.chroma[y * self.w..(y + 1) * self.w], cut);
                    Self::compact_row(energy_row, &self.energy[y * self.w..(y + 1) * self.w], cut);
                },
            );
        self.rgba = rgba;
        self.luma = luma;
        self.alpha = alpha;
        self.chroma = chroma;
        self.energy = energy;
        self.w = new_w;
        self.refresh_band(seam, false);
    }

    fn insert_seam(&mut self, seam: &[usize]) {
        let new_w = self.w + 1;
        let h = self.h;
        let mut rgba = vec![0u8; new_w * h * 4];
        let mut luma = vec![0f32; new_w * h];
        let mut alpha = vec![0f32; new_w * h];
        let mut chroma = vec![0f32; new_w * h];
        let mut energy = vec![0f32; new_w * h];
        rgba.par_chunks_mut(new_w * 4)
            .zip(luma.par_chunks_mut(new_w))
            .zip(alpha.par_chunks_mut(new_w))
            .zip(chroma.par_chunks_mut(new_w))
            .zip(energy.par_chunks_mut(new_w))
            .enumerate()
            .for_each(
                |(y, ((((row, luma_row), alpha_row), chroma_row), energy_row))| {
                    let cut = seam[y];
                    let src = &self.rgba[y * self.w * 4..(y + 1) * self.w * 4];
                    let filler = inserted_pixel(src, cut, self.w);
                    row[..cut * 4].copy_from_slice(&src[..cut * 4]);
                    row[cut * 4..cut * 4 + 4].copy_from_slice(&filler);
                    row[(cut + 1) * 4..].copy_from_slice(&src[cut * 4..]);
                    let ls = &self.luma[y * self.w..(y + 1) * self.w];
                    let alpha_src = &self.alpha[y * self.w..(y + 1) * self.w];
                    let chroma_src = &self.chroma[y * self.w..(y + 1) * self.w];
                    let es = &self.energy[y * self.w..(y + 1) * self.w];
                    luma_row[..cut].copy_from_slice(&ls[..cut]);
                    luma_row[cut] = if cut == 0 {
                        ls[0]
                    } else {
                        (ls[cut - 1] + ls[cut]) * 0.5
                    };
                    luma_row[cut + 1..].copy_from_slice(&ls[cut..]);
                    alpha_row[..cut].copy_from_slice(&alpha_src[..cut]);
                    alpha_row[cut] = if cut == 0 {
                        alpha_src[0]
                    } else {
                        (alpha_src[cut - 1] + alpha_src[cut]) * 0.5
                    };
                    alpha_row[cut + 1..].copy_from_slice(&alpha_src[cut..]);
                    chroma_row[..cut].copy_from_slice(&chroma_src[..cut]);
                    chroma_row[cut] = if cut == 0 {
                        chroma_src[0]
                    } else {
                        (chroma_src[cut - 1] + chroma_src[cut]) * 0.5
                    };
                    chroma_row[cut + 1..].copy_from_slice(&chroma_src[cut..]);
                    energy_row[..cut].copy_from_slice(&es[..cut]);
                    energy_row[cut] = es[cut];
                    energy_row[cut + 1..].copy_from_slice(&es[cut..]);
                },
            );
        self.rgba = rgba;
        self.luma = luma;
        self.alpha = alpha;
        self.chroma = chroma;
        self.energy = energy;
        self.w = new_w;
        self.refresh_band(seam, true);
        for (y, &x) in seam.iter().enumerate() {
            self.energy[y * self.w + x] += INSERT_PENALTY;
        }
    }

    fn refresh_band(&mut self, seam: &[usize], inserted: bool) {
        let w = self.w;
        let h = self.h;
        let shift = if inserted { 0 } else { 1 };
        for y in 0..h {
            let sl = seam[y.saturating_sub(1)];
            let sm = seam[y];
            let sr = seam[(y + 1).min(h - 1)];
            let lo = sl.min(sm).min(sr).saturating_sub(3);
            let hi = (sl.max(sm).max(sr) + 3 + shift).min(w.saturating_sub(1));
            for x in lo..=hi {
                if inserted && x == sm {
                    continue;
                }
                self.energy[y * w + x] =
                    energy_at(&self.luma, &self.alpha, &self.chroma, w, h, x, y);
            }
        }
    }

    fn carve_width(mut self, dst_w: usize) -> (Vec<u8>, usize) {
        if self.h == 0 || self.w == 0 {
            return (self.rgba, self.w.max(1));
        }
        while self.w > dst_w {
            let seam = self.pick_seam();
            self.remove_seam(&seam);
        }
        while self.w < dst_w {
            let seam = self.pick_seam();
            self.insert_seam(&seam);
        }
        (self.rgba, self.w)
    }
}

fn transpose(rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    let mut out = vec![0u8; w * h * 4];
    out.par_chunks_mut(h * 4).enumerate().for_each(|(x, col)| {
        for y in 0..h {
            let src = (y * w + x) * 4;
            col[y * 4..y * 4 + 4].copy_from_slice(&rgba[src..src + 4]);
        }
    });
    out
}

pub fn seam_carve(rgba: &[u8], w: u32, h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    let (dst_w, dst_h) = (dst_w.max(1), dst_h.max(1));
    if w == 0 || h == 0 {
        return vec![0u8; (dst_w * dst_h * 4) as usize];
    }
    if w == dst_w && h == dst_h {
        return rgba.to_vec();
    }
    let (buf, w2) = if w == dst_w {
        (rgba.to_vec(), w as usize)
    } else {
        Workspace::new(rgba.to_vec(), w as usize, h as usize).carve_width(dst_w as usize)
    };
    if h == dst_h {
        return buf;
    }
    let transposed = transpose(&buf, w2 as u32, h);
    let (buf2, h2) = Workspace::new(transposed, h as usize, w2).carve_width(dst_h as usize);
    transpose(&buf2, h2 as u32, w2 as u32)
}
