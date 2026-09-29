//! Reading the document back as pixels on the CPU: the full composite, per-layer RGBA, the
//! overview and thumbnail proxies, and the single composited pixel the eyedropper samples.

use super::*;

pub(crate) fn layer_source_pixel(layer: &Layer, doc_x: f32, doc_y: f32) -> [u8; 4] {
    let Some(tiles) = layer.tiles() else {
        return match layer.content.item() {
            Some(item) => vector_source_pixel(item, layer, doc_x, doc_y),
            None => [0, 0, 0, 0],
        };
    };
    let (sx, sy) = match layer.transform {
        Some(t) => {
            let Some(raw_bounds) = layer.content_bounds() else {
                return [0, 0, 0, 0];
            };
            t.inverse(bounds_center(raw_bounds), (doc_x, doc_y))
        }
        None => (doc_x, doc_y),
    };
    tiles.get_pixel(sx.floor() as i32, sy.floor() as i32)
}

/// One point of a vector layer, as a color rather than the alpha `vector_alpha_at` answers
/// picking with. This is the per-pixel twin of `vector::rasterize_into_rgba`'s inner loop —
/// same inverse map, same coverage, same `blend_over` — so the zoomed-out overview proxy
/// shows a layer of shapes exactly as the flatten and the shader do, instead of the empty
/// board it would get from a layer that has no tiles to sample.
pub(super) fn vector_source_pixel(
    item: &vector::VectorItem,
    layer: &Layer,
    doc_x: f32,
    doc_y: f32,
) -> [u8; 4] {
    let local = match layer
        .transform
        .filter(|t| !t.is_identity())
        .zip(item.bounds())
    {
        Some((t, raw)) => t.inverse(bounds_center(raw), (doc_x, doc_y)),
        None => (doc_x, doc_y),
    };
    let coverage = item.coverage(local.0, local.1);
    if coverage <= 0.0 {
        return [0, 0, 0, 0];
    }
    let mut src = item.color();
    src[3] = ((src[3] as f32) * coverage).round().clamp(0.0, 255.0) as u8;
    src
}

/// Hit-testing a vector layer evaluates its items' coverage directly rather than sampling
/// pixels — there are no pixels to sample. The point is inverse-mapped through the layer
/// transform first, exactly as the rasterizer and the shader both do.
pub(super) fn vector_alpha_at(
    item: &vector::VectorItem,
    layer: &Layer,
    doc_x: f32,
    doc_y: f32,
) -> u8 {
    let local = match layer
        .transform
        .filter(|t| !t.is_identity())
        .zip(item.bounds())
    {
        Some((t, raw)) => t.inverse(bounds_center(raw), (doc_x, doc_y)),
        None => (doc_x, doc_y),
    };
    let coverage = item.coverage(local.0, local.1) * (item.color()[3] as f32 / 255.0);
    (coverage * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Alpha alone, for hit-testing. Adjustments never touch alpha, so picking skips the
/// color work `layer_composited_pixel` does — an HSL round trip per layer per click.
pub(crate) fn layer_alpha_at(layer: &Layer, doc_x: f32, doc_y: f32) -> u8 {
    let alpha = match layer.content.item() {
        Some(item) => vector_alpha_at(item, layer, doc_x, doc_y),
        None => layer_source_pixel(layer, doc_x, doc_y)[3],
    };
    if alpha == 0 {
        return 0;
    }
    if layer.opacity < 1.0 {
        ((alpha as f32) * layer.opacity).round().clamp(0.0, 255.0) as u8
    } else {
        alpha
    }
}

/// A layer paired with the document-space box it can paint into — what
/// `Document::contributing_layers` hands the per-pixel composite so the loop can skip a layer
/// without touching its pixels.
pub(super) type BoundedLayer<'a> = (&'a Layer, (f32, f32, f32, f32));

pub(super) fn layer_composited_pixel(
    layer: &Layer,
    layers: &[Layer],
    doc_x: f32,
    doc_y: f32,
) -> [u8; 4] {
    let mut px = layer_source_pixel(layer, doc_x, doc_y);
    if px[3] == 0 {
        return px;
    }
    if let Some(base_id) = layer.clips_to.as_deref() {
        if let Some(base) = layers.iter().find(|l| l.id == base_id) {
            if !layer.clip_invert || base.visible {
                px[3] = crate::clip::apply_clip_alpha(
                    px[3],
                    crate::clip::base_raw_alpha_at(base, doc_x, doc_y),
                    layer.clip_invert,
                );
            }
        }
    }
    if let Some(adj) = layer.adjustments.as_ref().filter(|a| !a.is_neutral()) {
        let rgb = crate::filters::apply([px[0], px[1], px[2]], adj);
        px[0] = rgb[0];
        px[1] = rgb[1];
        px[2] = rgb[2];
    }
    if layer.opacity < 1.0 {
        px[3] = ((px[3] as f32) * layer.opacity).round().clamp(0.0, 255.0) as u8;
    }
    px
}

pub(crate) fn copy_layer_into_rgba(layer: &Layer, buf: &mut [u8], w: u32, h: u32) {
    if let Some(item) = layer.content.item() {
        vector::rasterize_into_rgba(item, layer.transform, buf, w, h);
        return;
    }
    let Some(tiles) = layer.tiles() else {
        return;
    };
    let Some(t) = layer.transform.filter(|t| !t.is_identity()) else {
        tiles.copy_into_rgba(buf, w, h);
        return;
    };
    let Some(raw_bounds) = layer.content_bounds() else {
        return;
    };
    let pivot = bounds_center(raw_bounds);
    let Some((x0, y0, x1, y1)) = clipped_pixel_span(t.transformed_aabb(raw_bounds), w, h) else {
        return;
    };
    let row_bytes = (w as usize) * 4;
    let y0 = y0 as usize;
    let x0 = x0 as usize;
    let x1 = x1 as usize;
    // Bilinear, not `get_pixel`'s nearest texel: the live GPU view samples a transformed
    // layer's tile atlas through a linear-filtered sampler (`board.wgsl`'s `fs_tile`), so a
    // flatten that snapped to the nearest source pixel would visibly disagree with what was
    // on screen at any rotation or non-integer scale — the export has to match what the user
    // was looking at.
    buf[y0 * row_bytes..y1 as usize * row_bytes]
        .par_chunks_mut(row_bytes)
        .enumerate()
        .for_each(|(i, row)| {
            let y = y0 + i;
            for x in x0..x1 {
                let (rx, ry) = t.inverse(pivot, (x as f32 + 0.5, y as f32 + 0.5));
                let px = tiles.sample_bilinear(rx - 0.5, ry - 0.5);
                if px[3] == 0 {
                    continue;
                }
                row[x * 4..x * 4 + 4].copy_from_slice(&px);
            }
        });
}

pub(crate) fn apply_layer_effects(rgba: &mut [u8], layer: &Layer, lut: Option<&AdjustmentLut>) {
    let lut = lut.filter(|l| !l.is_neutral());
    let opacity = layer.opacity;
    if lut.is_none() && opacity >= 1.0 {
        return;
    }
    rgba.par_chunks_mut(EFFECT_CHUNK_BYTES).for_each(|block| {
        for chunk in block.chunks_exact_mut(4) {
            if let Some(lut) = lut {
                let rgb = lut.apply([chunk[0], chunk[1], chunk[2]]);
                chunk[0..3].copy_from_slice(&rgb);
            }
            if opacity < 1.0 {
                chunk[3] = ((chunk[3] as f32) * opacity).round().clamp(0.0, 255.0) as u8;
            }
        }
    });
}

impl Document {
    pub fn composite_rgba(&self) -> (u32, u32, Vec<u8>) {
        let w = self.width.max(1);
        let h = self.height.max(1);
        let mut out = vec![0u8; (w as usize) * (h as usize) * 4];
        let mut layer_buf = vec![0u8; (w as usize) * (h as usize) * 4];
        for layer in &self.layers {
            if !layer.visible {
                continue;
            }
            if self.is_mask_base_id(&layer.id) {
                continue;
            }
            if layer.tiles().is_none() && layer.content.item().is_none() {
                continue;
            }
            layer_buf.fill(0);
            copy_layer_into_rgba(layer, &mut layer_buf, w, h);
            if let Some(base) = self.clip_base_for_layer(layer) {
                crate::clip::apply_clip_alpha_to_buffer(
                    &mut layer_buf,
                    base,
                    layer.clip_invert,
                    w,
                    h,
                );
            }
            let lut = layer.adjustments.map(|a| a.lut());
            apply_layer_effects(&mut layer_buf, layer, lut.as_ref());
            let mode = layer.blend_mode;
            out.par_chunks_mut(EFFECT_CHUNK_BYTES)
                .zip(layer_buf.par_chunks(EFFECT_CHUNK_BYTES))
                .for_each(|(dst_block, src_block)| {
                    for (dst, src) in dst_block.chunks_exact_mut(4).zip(src_block.chunks_exact(4)) {
                        if src[3] == 0 {
                            continue;
                        }
                        let blended = blend_with_mode(
                            [dst[0], dst[1], dst[2], dst[3]],
                            [src[0], src[1], src[2], src[3]],
                            mode,
                        );
                        dst.copy_from_slice(&blended);
                    }
                });
        }
        (w, h, out)
    }

    /// One document pixel of the visible composite, as the eyedropper sees it — the same
    /// stack walk flatten uses, so a vector layer answers `I` the way a painted one does.
    pub(super) fn sampled_pixel(&self, doc_x: f32, doc_y: f32) -> Option<[u8; 4]> {
        let ix = doc_x.floor() as i32;
        let iy = doc_y.floor() as i32;
        if ix < 0 || iy < 0 || (ix as u32) >= self.width || (iy as u32) >= self.height {
            return None;
        }
        let mut acc = [0u8; 4];
        for layer in &self.layers {
            if !layer.visible {
                continue;
            }
            let src = layer_composited_pixel(layer, &self.layers, doc_x, doc_y);
            if src[3] == 0 {
                continue;
            }
            acc = blend_with_mode(acc, src, layer.blend_mode);
        }
        if acc[3] == 0 {
            None
        } else {
            Some(acc)
        }
    }

    /// The eyedropper's color: the mean of the disc of radius `r + 0.5` around the clicked
    /// pixel, so the default `r = 1` is the 3×3 every image editor offers.
    ///
    /// Averaged in **premultiplied** space, for the same reason `blur.rs` works there — tiles
    /// hold straight alpha, so a plain mean would pull the sample toward whatever color sits
    /// in the fully transparent pixels beside a painted edge. Weighting each sample by its own
    /// alpha is what makes picking on the boundary of a stroke return the stroke's color
    /// rather than a color that is nowhere on the board.
    ///
    /// Pixels off the paper are skipped rather than counted as transparent, so a sample near
    /// the edge is not darkened by the void outside it. A radius of 0 short-circuits to the
    /// single pixel, byte-for-byte what this returned before the average existed.
    pub fn sample_color(&self, doc_x: f32, doc_y: f32) -> Option<[u8; 4]> {
        let radius = self.eyedropper_radius as i32;
        if radius == 0 {
            return self.sampled_pixel(doc_x, doc_y);
        }
        let cx = doc_x.floor() as i32;
        let cy = doc_y.floor() as i32;
        let (mut sum_r, mut sum_g, mut sum_b, mut sum_a) = (0u32, 0u32, 0u32, 0u32);
        let mut count = 0u32;
        for y in (cy - radius)..=(cy + radius) {
            for x in (cx - radius)..=(cx + radius) {
                if x < 0 || y < 0 || (x as u32) >= self.width || (y as u32) >= self.height {
                    continue;
                }
                if !Self::eyedropper_covers(x - cx, y - cy, radius) {
                    continue;
                }
                count += 1;
                let Some(px) = self.sampled_pixel(x as f32 + 0.5, y as f32 + 0.5) else {
                    continue;
                };
                let a = px[3] as u32;
                sum_r += px[0] as u32 * a;
                sum_g += px[1] as u32 * a;
                sum_b += px[2] as u32 * a;
                sum_a += a;
            }
        }
        if count == 0 || sum_a == 0 {
            return None;
        }
        let unpremultiply = |sum: u32| ((sum + sum_a / 2) / sum_a).min(ALPHA_MAX) as u8;
        Some([
            unpremultiply(sum_r),
            unpremultiply(sum_g),
            unpremultiply(sum_b),
            ((sum_a + count / 2) / count).min(ALPHA_MAX) as u8,
        ])
    }

    pub(super) fn eyedropper_covers(dx: i32, dy: i32, radius: i32) -> bool {
        let r = radius as f32 + 0.5;
        (dx * dx + dy * dy) as f32 <= r * r
    }

    /// The layers a composite has to sample, each with the box it can possibly paint into.
    /// Hoisting this out of the per-pixel loop is what keeps a whole-document flatten from
    /// scaling with the layers that are hidden, empty, or nowhere near the pixel being asked
    /// about — on a deep stack that is most of them, for most pixels.
    pub(super) fn contributing_layers(&self) -> Vec<BoundedLayer<'_>> {
        self.layers
            .iter()
            .filter(|l| l.visible)
            .filter(|l| !self.is_mask_base_id(&l.id))
            .filter(|l| l.tiles().is_some() || l.content.item().is_some())
            .filter_map(|l| {
                let raw = l.content_bounds()?;
                let t = l.transform.unwrap_or_default();
                Some((l, t.transformed_aabb(raw)))
            })
            .collect()
    }

    pub(super) fn composite_pixel_of(
        &self,
        layers: &[BoundedLayer<'_>],
        doc_x: f32,
        doc_y: f32,
    ) -> [u8; 4] {
        let mut acc = [0u8; 4];
        for (layer, bounds) in layers {
            if doc_x < bounds.0 || doc_y < bounds.1 || doc_x > bounds.2 || doc_y > bounds.3 {
                continue;
            }
            let src = layer_composited_pixel(layer, &self.layers, doc_x, doc_y);
            if src[3] == 0 {
                continue;
            }
            acc = blend_with_mode(acc, src, layer.blend_mode);
        }
        acc
    }

    pub fn overview_dimensions(width: u32, height: u32, max_side: u32) -> (u32, u32) {
        let max_side = max_side.max(1);
        let dw = width.max(1);
        let dh = height.max(1);
        let scale = (max_side as f32 / dw as f32)
            .min(max_side as f32 / dh as f32)
            .min(1.0);
        let tw = ((dw as f32) * scale).round().max(1.0) as u32;
        let th = ((dh as f32) * scale).round().max(1.0) as u32;
        (tw, th)
    }

    pub fn composite_overview_rect(
        &self,
        max_side: u32,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
    ) -> Vec<u8> {
        let (tw, th) = Self::overview_dimensions(self.width, self.height, max_side);
        let x = x.min(tw);
        let y = y.min(th);
        let w = w.min(tw.saturating_sub(x));
        let h = h.min(th.saturating_sub(y));
        if w == 0 || h == 0 {
            return Vec::new();
        }
        let dw = self.width.max(1);
        let dh = self.height.max(1);
        let mut rgba = vec![0u8; (w as usize) * (h as usize) * 4];
        let contributing = self.contributing_layers();
        let row_bytes = (w as usize) * 4;
        rgba.par_chunks_mut(row_bytes)
            .enumerate()
            .for_each(|(row, line)| {
                let ty = y + row as u32;
                let doc_y = if th <= 1 {
                    0.0
                } else {
                    ty as f32 * (dh - 1) as f32 / (th - 1) as f32
                };
                for (col, px) in line.chunks_exact_mut(4).enumerate() {
                    let tx = x + col as u32;
                    let doc_x = if tw <= 1 {
                        0.0
                    } else {
                        tx as f32 * (dw - 1) as f32 / (tw - 1) as f32
                    };
                    px.copy_from_slice(&self.composite_pixel_of(&contributing, doc_x, doc_y));
                }
            });
        rgba
    }

    pub fn pick_color(&mut self, doc_x: f32, doc_y: f32) -> Option<[u8; 4]> {
        let color = self.sample_color(doc_x, doc_y)?;
        self.color = color;
        Some(color)
    }

    pub fn composite_thumbnail(&self, max_side: u32) -> (u32, u32, Vec<u8>) {
        let max_side = max_side.max(1);
        let (dw, dh, full) = self.composite_rgba();
        let (crop_x, crop_y, crop_w, crop_h) = self
            .painted_content_bounds()
            .map(|(x0, y0, x1, y1)| {
                let x0 = x0.floor().max(0.0) as u32;
                let y0 = y0.floor().max(0.0) as u32;
                let x1 = x1.ceil().min(dw as f32).max(x0 as f32 + 1.0) as u32;
                let y1 = y1.ceil().min(dh as f32).max(y0 as f32 + 1.0) as u32;
                (x0, y0, (x1 - x0).max(1).min(dw), (y1 - y0).max(1).min(dh))
            })
            .unwrap_or((0, 0, dw, dh));
        let crop_w = crop_w.min(dw.saturating_sub(crop_x)).max(1);
        let crop_h = crop_h.min(dh.saturating_sub(crop_y)).max(1);

        let scale = (max_side as f32 / crop_w as f32)
            .min(max_side as f32 / crop_h as f32)
            .min(1.0);
        let tw = ((crop_w as f32) * scale).round().max(1.0) as u32;
        let th = ((crop_h as f32) * scale).round().max(1.0) as u32;

        if tw == dw && th == dh && crop_x == 0 && crop_y == 0 {
            return (dw, dh, full);
        }

        let mut rgba = vec![0u8; (tw as usize) * (th as usize) * 4];
        for ty in 0..th {
            for tx in 0..tw {
                let sx = if tw <= 1 {
                    crop_x
                } else {
                    crop_x
                        + ((tx as f32) * ((crop_w - 1) as f32) / ((tw - 1) as f32)).round() as u32
                };
                let sy = if th <= 1 {
                    crop_y
                } else {
                    crop_y
                        + ((ty as f32) * ((crop_h - 1) as f32) / ((th - 1) as f32)).round() as u32
                };
                let si = ((sy as usize) * (dw as usize) + (sx as usize)) * 4;
                let di = ((ty as usize) * (tw as usize) + (tx as usize)) * 4;
                rgba[di..di + 4].copy_from_slice(&full[si..si + 4]);
            }
        }
        (tw, th, rgba)
    }

    pub fn painted_content_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut any = false;
        for layer in &self.layers {
            if !layer.visible || layer.is_paper() {
                continue;
            }
            let Some(raw) = layer.content_bounds() else {
                continue;
            };
            let corners = match layer.transform {
                Some(t) if !t.is_identity() => {
                    let pivot = bounds_center(raw);
                    t.transformed_corners(pivot, raw)
                }
                _ => [
                    (raw.0, raw.1),
                    (raw.2, raw.1),
                    (raw.2, raw.3),
                    (raw.0, raw.3),
                ],
            };
            for (x, y) in corners {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
                any = true;
            }
        }
        if !any {
            return None;
        }
        Some((
            min_x.clamp(0.0, self.width as f32),
            min_y.clamp(0.0, self.height as f32),
            max_x.clamp(0.0, self.width as f32),
            max_y.clamp(0.0, self.height as f32),
        ))
    }

    pub fn layer_rgba(&self, index: usize) -> Option<(u32, u32, Vec<u8>)> {
        let layer = self.layers.get(index)?;
        if layer.tiles().is_none() && layer.content.item().is_none() {
            return None;
        }
        let w = self.width.max(1);
        let h = self.height.max(1);
        let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
        copy_layer_into_rgba(layer, &mut buf, w, h);
        if let Some(base) = self.clip_base_for_layer(layer) {
            crate::clip::apply_clip_alpha_to_buffer(&mut buf, base, layer.clip_invert, w, h);
        }
        if let Some(adj) = &layer.adjustments {
            let lut = adj.lut();
            buf.par_chunks_mut(EFFECT_CHUNK_BYTES)
                .for_each(|block| lut.apply_rgba(block));
        }
        Some((w, h, buf))
    }

    pub fn layer_svg(&self, index: usize) -> Option<String> {
        let layer = self.layers.get(index)?;
        let item = layer.content.item()?;
        let mut svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">",
            self.width, self.height, self.width, self.height
        );
        if let Some(group) = crate::vector_svg::svg_transform_attr(item, layer.transform) {
            svg.push_str(&group);
        }
        if let Some(markup) = crate::vector_svg::item_svg(item) {
            svg.push_str(&markup);
        }
        if layer.transform.is_some_and(|t| !t.is_identity()) {
            svg.push_str("</g>");
        }
        svg.push_str("</svg>");
        Some(svg)
    }

    pub fn set_eyedropper_radius(&mut self, radius: u32) {
        self.eyedropper_radius = radius.clamp(EYEDROPPER_RADIUS_MIN, EYEDROPPER_RADIUS_MAX);
    }
}
