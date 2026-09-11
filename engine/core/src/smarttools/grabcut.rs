//! GrabCut-style background matting: two colour GMMs, an 8-connected contrast-sensitive grid,
//! and a min cut. Work resolution is capped so the cut stays interactive; the matte is
//! bilinearly upsampled back to the layer.

use super::gmm::{dist2, fit_gmm};
use super::maxflow::FlowGraph;
use super::resample::box_downsample;
use rayon::prelude::*;

const SQRT_2: f32 = std::f32::consts::SQRT_2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MatteParams {
    pub max_side: u32,
    pub border_fraction: f32,
    pub components: usize,
    pub iterations: u32,
    pub gamma: f32,
}

impl Default for MatteParams {
    fn default() -> Self {
        Self {
            max_side: 512,
            border_fraction: 0.08,
            components: 5,
            iterations: 3,
            gamma: 50.0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Seed {
    Background,
    Free,
}

fn resample_mask(mask: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    if sw == dw && sh == dh {
        return mask.to_vec();
    }
    let mut out = vec![0u8; dw * dh];
    let x_ratio = sw as f32 / dw as f32;
    let y_ratio = sh as f32 / dh as f32;
    out.par_chunks_mut(dw).enumerate().for_each(|(y, row)| {
        let sy = ((y as f32 + 0.5) * y_ratio - 0.5).max(0.0);
        let y0 = (sy.floor() as usize).min(sh - 1);
        let y1 = (y0 + 1).min(sh - 1);
        let fy = sy - y0 as f32;
        for (x, cell) in row.iter_mut().enumerate() {
            let sx = ((x as f32 + 0.5) * x_ratio - 0.5).max(0.0);
            let x0 = (sx.floor() as usize).min(sw - 1);
            let x1 = (x0 + 1).min(sw - 1);
            let fx = sx - x0 as f32;
            let a = mask[y0 * sw + x0] as f32;
            let b = mask[y0 * sw + x1] as f32;
            let c = mask[y1 * sw + x0] as f32;
            let d = mask[y1 * sw + x1] as f32;
            let top = a + (b - a) * fx;
            let bottom = c + (d - c) * fx;
            *cell = (top + (bottom - top) * fy).round().clamp(0.0, 255.0) as u8;
        }
    });
    out
}

fn shrink_seed(mask: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    if sw == dw && sh == dh {
        return mask.to_vec();
    }
    let mut rgba = vec![0u8; sw * sh * 4];
    for (i, &m) in mask.iter().enumerate() {
        let o = i * 4;
        rgba[o] = m;
        rgba[o + 3] = 255;
    }
    box_downsample(&rgba, sw as u32, sh as u32, dw as u32, dh as u32)
        .chunks_exact(4)
        .map(|p| p[0])
        .collect()
}

pub fn foreground_matte(rgba: &[u8], w: u32, h: u32, params: &MatteParams) -> Option<Vec<u8>> {
    matte(rgba, w, h, None, params)
}

pub fn foreground_matte_in_region(
    rgba: &[u8],
    w: u32,
    h: u32,
    region: &[u8],
    params: &MatteParams,
) -> Option<Vec<u8>> {
    matte(rgba, w, h, Some(region), params)
}

fn work_size(w: u32, h: u32, max_side: u32) -> (u32, u32) {
    let long = w.max(h);
    if long > max_side {
        let s = max_side as f32 / long as f32;
        (
            ((w as f32 * s).round() as u32).max(1),
            ((h as f32 * s).round() as u32).max(1),
        )
    } else {
        (w, h)
    }
}

fn beta(colors: &[[f32; 3]], w: usize, h: usize) -> f32 {
    let (sum, count) = (0..h)
        .into_par_iter()
        .map(|y| {
            let mut s = 0.0f64;
            let mut n = 0usize;
            for x in 0..w {
                let p = colors[y * w + x];
                if x + 1 < w {
                    s += dist2(p, colors[y * w + x + 1]) as f64;
                    n += 1;
                }
                if y + 1 < h {
                    s += dist2(p, colors[(y + 1) * w + x]) as f64;
                    n += 1;
                }
                if x + 1 < w && y + 1 < h {
                    s += dist2(p, colors[(y + 1) * w + x + 1]) as f64;
                    n += 1;
                }
                if x > 0 && y + 1 < h {
                    s += dist2(p, colors[(y + 1) * w + x - 1]) as f64;
                    n += 1;
                }
            }
            (s, n)
        })
        .reduce(|| (0.0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
    if count == 0 || sum <= f64::MIN_POSITIVE {
        return 0.0;
    }
    (1.0 / (2.0 * sum / count as f64)) as f32
}

fn link_weight(gamma: f32, beta: f32, a: [f32; 3], b: [f32; 3], diag: bool) -> f32 {
    let w = gamma * (-beta * dist2(a, b)).exp();
    let w = if diag { w / SQRT_2 } else { w };
    if w.is_finite() {
        w.max(0.0)
    } else {
        0.0
    }
}

fn collect_samples(
    colors: &[[f32; 3]],
    label_fg: &[bool],
    alpha: &[u8],
    fg: bool,
    out: &mut Vec<[f32; 3]>,
) {
    out.clear();
    out.extend(
        colors
            .iter()
            .zip(label_fg.iter())
            .zip(alpha.iter())
            .filter_map(|((c, &lab), &a)| {
                if a == 0 {
                    None
                } else {
                    (lab == fg).then_some(*c)
                }
            }),
    );
}

fn enforce_seeds(label_fg: &mut [bool], seeds: &[Seed]) {
    for (fg, seed) in label_fg.iter_mut().zip(seeds.iter()) {
        if *seed == Seed::Background {
            *fg = false;
        }
    }
}

fn flood(
    start: usize,
    w: usize,
    h: usize,
    want: bool,
    labels: &[bool],
    seen: &mut [bool],
) -> Vec<usize> {
    let mut stack = vec![start];
    let mut cells = Vec::new();
    seen[start] = true;
    while let Some(i) = stack.pop() {
        cells.push(i);
        let x = i % w;
        let y = i / w;
        let mut push = |nx: usize, ny: usize| {
            let j = ny * w + nx;
            if !seen[j] && labels[j] == want {
                seen[j] = true;
                stack.push(j);
            }
        };
        if x > 0 {
            push(x - 1, y);
        }
        if x + 1 < w {
            push(x + 1, y);
        }
        if y > 0 {
            push(x, y - 1);
        }
        if y + 1 < h {
            push(x, y + 1);
        }
    }
    cells
}

fn keep_major_foreground(label_fg: &mut [bool], w: usize, h: usize) {
    let n = w * h;
    let mut seen = vec![false; n];
    let mut parts: Vec<Vec<usize>> = Vec::new();
    for i in 0..n {
        if seen[i] || !label_fg[i] {
            continue;
        }
        parts.push(flood(i, w, h, true, label_fg, &mut seen));
    }
    let largest = parts.iter().map(|p| p.len()).max().unwrap_or(0);
    if largest == 0 {
        return;
    }
    let floor = ((largest / 4).max(n / 500).max(12)).min(largest);
    for part in parts {
        if part.len() < floor {
            for j in part {
                label_fg[j] = false;
            }
        }
    }
}

fn fill_interior_holes(label_fg: &mut [bool], w: usize, h: usize, min_size: usize) {
    let n = w * h;
    let mut seen = vec![false; n];
    for i in 0..n {
        if seen[i] || label_fg[i] {
            continue;
        }
        let cells = flood(i, w, h, false, label_fg, &mut seen);
        let touches_border = cells.iter().any(|&j| {
            let x = j % w;
            let y = j / w;
            x == 0 || y == 0 || x + 1 == w || y + 1 == h
        });
        if !touches_border && cells.len() < min_size {
            for j in cells {
                label_fg[j] = true;
            }
        }
    }
}

fn cleanup_cut(label_fg: &mut [bool], seeds: &[Seed], w: usize, h: usize) {
    let min_size = ((w * h) / 500).max(12);
    keep_major_foreground(label_fg, w, h);
    fill_interior_holes(label_fg, w, h, min_size);
    enforce_seeds(label_fg, seeds);
}

fn matte(
    rgba: &[u8],
    w: u32,
    h: u32,
    region: Option<&[u8]>,
    params: &MatteParams,
) -> Option<Vec<u8>> {
    if w == 0 || h == 0 || rgba.len() != (w as usize) * (h as usize) * 4 {
        return None;
    }
    let (sw, sh) = work_size(w, h, params.max_side);
    let small = if sw == w && sh == h {
        rgba.to_vec()
    } else {
        box_downsample(rgba, w, h, sw, sh)
    };
    let (sw, sh) = (sw as usize, sh as usize);

    let colors: Vec<[f32; 3]> = small
        .par_chunks_exact(4)
        .map(|p| {
            if p[3] == 0 {
                [0.0, 0.0, 0.0]
            } else {
                [p[0] as f32, p[1] as f32, p[2] as f32]
            }
        })
        .collect();
    let alpha: Vec<u8> = small.par_chunks_exact(4).map(|p| p[3]).collect();

    let drawn = region
        .filter(|r| r.len() == (w as usize) * (h as usize))
        .map(|r| {
            if sw == w as usize && sh == h as usize {
                r.to_vec()
            } else {
                shrink_seed(r, w as usize, h as usize, sw, sh)
            }
        });
    let border = (((sw.min(sh) as f32) * params.border_fraction).round() as usize).max(1);
    let on_rim = |i: usize| {
        let (x, y) = (i % sw, i / sw);
        x < border || y < border || x + border >= sw || y + border >= sh
    };
    let mut seeds: Vec<Seed> = (0..sw * sh)
        .into_par_iter()
        .map(|i| {
            let background = match &drawn {
                Some(mask) => mask[i] < 128,
                None => on_rim(i),
            };
            if background || alpha[i] == 0 {
                Seed::Background
            } else {
                Seed::Free
            }
        })
        .collect();
    if seeds.iter().all(|s| *s != Seed::Background) {
        seeds.par_iter_mut().enumerate().for_each(|(i, seed)| {
            if on_rim(i) {
                *seed = Seed::Background;
            }
        });
    }

    let mut label_fg: Vec<bool> = seeds.iter().map(|s| *s == Seed::Free).collect();
    if !label_fg.iter().any(|&f| f) {
        return None;
    }

    let beta = beta(&colors, sw, sh);
    let gamma = params.gamma;
    let hard = gamma * 8.0 * 1000.0;
    let pixels = sw * sh;
    let source = pixels;
    let sink = pixels + 1;

    let mut graph = FlowGraph::with_capacity(pixels + 2, pixels * 5);
    let mut term_src = vec![0usize; pixels];
    let mut term_sink = vec![0usize; pixels];
    let mut built = false;
    let mut fg_samples = Vec::new();
    let mut bg_samples = Vec::new();

    for _ in 0..params.iterations.max(1) {
        collect_samples(&colors, &label_fg, &alpha, true, &mut fg_samples);
        collect_samples(&colors, &label_fg, &alpha, false, &mut bg_samples);
        if fg_samples.is_empty() || bg_samples.is_empty() {
            break;
        }
        let fg_gmm = fit_gmm(&fg_samples, params.components);
        let bg_gmm = fit_gmm(&bg_samples, params.components);
        if fg_gmm.components.is_empty() || bg_gmm.components.is_empty() {
            break;
        }

        let terminals: Vec<(f32, f32)> = colors
            .par_iter()
            .zip(seeds.par_iter())
            .map(|(c, seed)| match seed {
                Seed::Background => (0.0, hard),
                Seed::Free => (bg_gmm.neg_log_density(*c), fg_gmm.neg_log_density(*c)),
            })
            .collect();

        if !built {
            for (i, &(to_source, to_sink)) in terminals.iter().enumerate() {
                term_src[i] = graph.add_edge(source, i, to_source, 0.0);
                term_sink[i] = graph.add_edge(i, sink, to_sink, 0.0);
            }
            for y in 0..sh {
                for x in 0..sw {
                    let i = y * sw + x;
                    if x + 1 < sw {
                        let j = i + 1;
                        let wgt = link_weight(gamma, beta, colors[i], colors[j], false);
                        graph.add_edge(i, j, wgt, wgt);
                    }
                    if y + 1 < sh {
                        let j = i + sw;
                        let wgt = link_weight(gamma, beta, colors[i], colors[j], false);
                        graph.add_edge(i, j, wgt, wgt);
                    }
                    if x + 1 < sw && y + 1 < sh {
                        let j = i + sw + 1;
                        let wgt = link_weight(gamma, beta, colors[i], colors[j], true);
                        graph.add_edge(i, j, wgt, wgt);
                    }
                    if x > 0 && y + 1 < sh {
                        let j = i + sw - 1;
                        let wgt = link_weight(gamma, beta, colors[i], colors[j], true);
                        graph.add_edge(i, j, wgt, wgt);
                    }
                }
            }
            built = true;
        } else {
            graph.reset_flow();
            for (i, &(to_source, to_sink)) in terminals.iter().enumerate() {
                graph.set_capacity(term_src[i], to_source, 0.0);
                graph.set_capacity(term_sink[i], to_sink, 0.0);
            }
        }

        graph.max_flow(source, sink);
        let side = graph.source_side(source);
        let mut next = side[..pixels].to_vec();
        enforce_seeds(&mut next, &seeds);
        if next == label_fg {
            label_fg = next;
            break;
        }
        label_fg = next;
    }

    cleanup_cut(&mut label_fg, &seeds, sw, sh);
    if !label_fg.iter().any(|&f| f) {
        return None;
    }

    let small_mask: Vec<u8> = label_fg.iter().map(|&f| if f { 255 } else { 0 }).collect();
    Some(resample_mask(&small_mask, sw, sh, w as usize, h as usize))
}
