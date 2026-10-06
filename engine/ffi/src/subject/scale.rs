pub(crate) fn fitted_size(width: u32, height: u32, max_side: u32) -> (u32, u32) {
    let long = width.max(height);
    if long == 0 || max_side == 0 || long <= max_side {
        return (width.max(1), height.max(1));
    }
    let long = long as u64;
    let cap = max_side as u64;
    let fit = |side: u32| -> u32 {
        let rounded = ((side as u64) * cap + long / 2) / long;
        rounded.clamp(1, cap) as u32
    };
    (fit(width), fit(height))
}

pub(crate) fn crop_rgba(
    src: Vec<u8>,
    src_w: u32,
    src_h: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> Vec<u8> {
    let fits = x.saturating_add(w) <= src_w
        && y.saturating_add(h) <= src_h
        && src.len()
            >= (src_w as usize)
                .saturating_mul(src_h as usize)
                .saturating_mul(4);
    if !fits || (x == 0 && y == 0 && w == src_w && h == src_h) {
        return src;
    }
    let mut out = vec![0u8; (w as usize) * (h as usize) * 4];
    for row in 0..h {
        let src_i = (((y + row) as usize) * (src_w as usize) + x as usize) * 4;
        let dst_i = (row as usize) * (w as usize) * 4;
        let n = (w as usize) * 4;
        out[dst_i..dst_i + n].copy_from_slice(&src[src_i..src_i + n]);
    }
    out
}

pub(crate) fn expand_matte(
    doc_w: u32,
    doc_h: u32,
    x: u32,
    y: u32,
    matte_w: u32,
    matte_h: u32,
    matte: &[u8],
) -> Vec<u8> {
    let fits = x.saturating_add(matte_w) <= doc_w
        && y.saturating_add(matte_h) <= doc_h
        && matte.len() >= (matte_w as usize).saturating_mul(matte_h as usize);
    if !fits || (x == 0 && y == 0 && matte_w == doc_w && matte_h == doc_h) {
        return matte.to_vec();
    }
    let mut out = vec![0u8; (doc_w as usize) * (doc_h as usize)];
    for row in 0..matte_h {
        let src_i = (row as usize) * (matte_w as usize);
        let dst_i = ((y + row) as usize) * (doc_w as usize) + x as usize;
        let n = matte_w as usize;
        out[dst_i..dst_i + n].copy_from_slice(&matte[src_i..src_i + n]);
    }
    out
}

pub(crate) fn downscale_premultiplied(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
) -> Vec<u8> {
    if src_w == dst_w && src_h == dst_h {
        let mut out = src.to_vec();
        premultiply_in_place(&mut out);
        return out;
    }
    let mut out = vec![0u8; (dst_w as usize) * (dst_h as usize) * 4];
    for y in 0..dst_h {
        let sy = (y as f32 + 0.5) * (src_h as f32) / (dst_h as f32) - 0.5;
        for x in 0..dst_w {
            let sx = (x as f32 + 0.5) * (src_w as f32) / (dst_w as f32) - 0.5;
            let px = sample_rgba(src, src_w, src_h, sx, sy);
            let i = ((y as usize) * (dst_w as usize) + x as usize) * 4;
            out[i..i + 4].copy_from_slice(&px);
        }
    }
    out
}

pub(crate) fn resample_coverage(
    src: &[f32],
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
) -> Vec<f32> {
    if src_w == dst_w && src_h == dst_h {
        return src.to_vec();
    }
    let mut out = vec![0.0; (dst_w as usize) * (dst_h as usize)];
    for y in 0..dst_h {
        let sy = (y as f32 + 0.5) * (src_h as f32) / (dst_h as f32) - 0.5;
        for x in 0..dst_w {
            let sx = (x as f32 + 0.5) * (src_w as f32) / (dst_w as f32) - 0.5;
            out[(y * dst_w + x) as usize] = sample_f32(src, src_w, src_h, sx, sy);
        }
    }
    out
}

pub(crate) fn quantize_coverage(src: &[f32]) -> Vec<u8> {
    src.iter()
        .map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect()
}

fn premultiply_in_place(rgba: &mut [u8]) {
    for px in rgba.chunks_exact_mut(4) {
        let alpha = u16::from(px[3]);
        if alpha == 255 {
            continue;
        }
        px[0] = ((u16::from(px[0]) * alpha + 127) / 255) as u8;
        px[1] = ((u16::from(px[1]) * alpha + 127) / 255) as u8;
        px[2] = ((u16::from(px[2]) * alpha + 127) / 255) as u8;
    }
}

fn sample_rgba(src: &[u8], width: u32, height: u32, x: f32, y: f32) -> [u8; 4] {
    let x = x.clamp(0.0, (width - 1) as f32);
    let y = y.clamp(0.0, (height - 1) as f32);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let at = |px: u32, py: u32| {
        let i = ((py * width + px) * 4) as usize;
        let alpha = u16::from(src[i + 3]);
        let premultiply = |channel: u8| (u16::from(channel) * alpha + 127) / 255;
        [
            premultiply(src[i]) as f32,
            premultiply(src[i + 1]) as f32,
            premultiply(src[i + 2]) as f32,
            alpha as f32,
        ]
    };
    let a = at(x0, y0);
    let b = at(x1, y0);
    let c = at(x0, y1);
    let d = at(x1, y1);
    let mut out = [0u8; 4];
    for i in 0..4 {
        let top = a[i] + (b[i] - a[i]) * tx;
        let bottom = c[i] + (d[i] - c[i]) * tx;
        out[i] = (top + (bottom - top) * ty).round().clamp(0.0, 255.0) as u8;
    }
    out
}

fn sample_f32(src: &[f32], width: u32, height: u32, x: f32, y: f32) -> f32 {
    let x = x.clamp(0.0, (width - 1) as f32);
    let y = y.clamp(0.0, (height - 1) as f32);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let at = |px: u32, py: u32| src[(py * width + px) as usize];
    let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * tx;
    let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * tx;
    top + (bottom - top) * ty
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitted_size_shrinks_only_the_long_side_past_the_cap() {
        assert_eq!(fitted_size(800, 400, 4096), (800, 400));
        assert_eq!(fitted_size(8000, 4000, 4096), (4096, 2048));
        for side in 1..64 {
            let (w, h) = fitted_size(side, side.saturating_mul(3), 17);
            assert!(w <= 17 && h <= 17);
            assert!(w >= 1 && h >= 1);
        }
    }

    #[test]
    fn a_subject_crop_round_trips_through_the_document_matte() {
        let src = vec![
            1, 0, 0, 255, 2, 0, 0, 255, 3, 0, 0, 255, 0, 0, 0, 0, 4, 0, 0, 255, 5, 0, 0, 255,
        ];
        let cropped = crop_rgba(src.clone(), 3, 2, 1, 0, 2, 2);
        assert_eq!(
            cropped,
            vec![2, 0, 0, 255, 3, 0, 0, 255, 4, 0, 0, 255, 5, 0, 0, 255]
        );
        assert_eq!(crop_rgba(src.clone(), 3, 2, 0, 0, 3, 2), src);
        let matte = [9u8, 8, 7, 6];
        assert_eq!(
            expand_matte(3, 2, 1, 0, 2, 2, &matte),
            vec![0, 9, 8, 0, 7, 6]
        );
    }

    #[test]
    fn coverage_resamples_before_it_is_quantized() {
        let src = [0.0, 1.0];
        let up = resample_coverage(&src, 2, 1, 2, 1);
        assert_eq!(up, src);
        let wide = resample_coverage(&src, 2, 1, 3, 1);
        assert_eq!(wide.len(), 3);
        assert!(wide[0] < 0.2);
        assert!(wide[2] > 0.8);
        assert!(wide[1] > wide[0] && wide[1] < wide[2]);
        assert_eq!(quantize_coverage(&[0.0, 1.0, 0.5]), vec![0, 255, 128]);
    }
}
