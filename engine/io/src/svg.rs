use crate::png::encode_png_rgba;
use calumma_core::{vector, vector_svg, BlendMode, Document, Layer};

/// Whole-document SVG export.
///
/// A vector layer stays vector — its items are emitted as the same SVG primitives
/// `Document::layer_svg` already writes for a single layer, so a rect exported here is still a
/// `<rect>` in whatever opens the file. Everything with pixels (raster, text, anything with a
/// layer transform baked in) rides along as an embedded PNG `<image>`: degraded compared to
/// real geometry, but it keeps the export lossless *visually* and honest about what Calumma
/// actually holds, instead of refusing the whole document because one layer is painted.
///
/// Layer opacity and blend mode survive as `opacity` / `mix-blend-mode`, which SVG and every
/// modern renderer understand natively; masks and adjustments cannot be expressed that way, so
/// they are baked into the pixels the same way `Document::layer_rgba` bakes them everywhere
/// else.
pub fn encode_svg(doc: &Document) -> String {
    let width = doc.width.max(1);
    let height = doc.height.max(1);
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" \
viewBox=\"0 0 {width} {height}\">"
    );
    for (index, layer) in doc.layers.iter().enumerate() {
        if !layer.visible {
            continue;
        }
        let body = match layer.content.item() {
            Some(item) => vector_group(item, layer),
            None => raster_image(doc, index),
        };
        let Some(body) = body else {
            continue;
        };
        out.push_str(&format!(
            "<g{}>{body}</g>",
            group_attrs(layer, escape(&layer.name))
        ));
    }
    out.push_str("</svg>");
    out
}

fn blend_style(mode: BlendMode) -> Option<&'static str> {
    match mode {
        BlendMode::Normal => None,
        BlendMode::Multiply => Some("multiply"),
        BlendMode::Screen => Some("screen"),
    }
}

fn group_attrs(layer: &Layer, name: String) -> String {
    let mut attrs = format!(" id=\"{name}\"");
    if layer.opacity < 1.0 {
        attrs.push_str(&format!(" opacity=\"{}\"", layer.opacity.clamp(0.0, 1.0)));
    }
    if let Some(blend) = blend_style(layer.blend_mode) {
        attrs.push_str(&format!(" style=\"mix-blend-mode:{blend}\""));
    }
    attrs
}

fn vector_group(item: &vector::VectorItem, layer: &Layer) -> Option<String> {
    let mut out = String::new();
    if let Some(group) = vector_svg::svg_transform_attr(item, layer.transform) {
        out.push_str(&group);
    }
    if let Some(markup) = vector_svg::item_svg(item) {
        out.push_str(&markup);
    }
    if layer.transform.is_some_and(|t| !t.is_identity()) {
        out.push_str("</g>");
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
}

/// The tight box of everything the layer actually paints. A full-document PNG per layer would
/// make a mostly-empty stack enormous; the crop costs one pass over alpha and is what keeps an
/// exported SVG proportional to its ink.
fn ink_bounds(rgba: &[u8], width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
    let (mut min_x, mut min_y) = (u32::MAX, u32::MAX);
    let (mut max_x, mut max_y) = (0u32, 0u32);
    for y in 0..height {
        let row = (y as usize) * (width as usize) * 4;
        for x in 0..width {
            if rgba[row + (x as usize) * 4 + 3] == 0 {
                continue;
            }
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    if min_x == u32::MAX {
        return None;
    }
    Some((min_x, min_y, max_x - min_x + 1, max_y - min_y + 1))
}

fn crop(rgba: &[u8], width: u32, box_: (u32, u32, u32, u32)) -> Vec<u8> {
    let (x, y, w, h) = box_;
    let mut out = Vec::with_capacity((w as usize) * (h as usize) * 4);
    for row in 0..h {
        let start = (((y + row) as usize) * (width as usize) + x as usize) * 4;
        out.extend_from_slice(&rgba[start..start + (w as usize) * 4]);
    }
    out
}

fn png_bytes(rgba: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    encode_png_rgba(rgba, width, height).ok()
}

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let bits = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                let index = ((bits >> (18 - i * 6)) & 0x3F) as usize;
                out.push(BASE64_ALPHABET[index] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The one color a box is painted in, if it is painted in exactly one. Paper is a
/// document-sized field of solid white, and so is any layer someone flood-filled — embedding
/// megabytes of base64 for a rectangle would be absurd when SVG has a `<rect>` for it.
fn uniform_color(rgba: &[u8], width: u32, box_: (u32, u32, u32, u32)) -> Option<[u8; 4]> {
    let (x, y, w, h) = box_;
    let first = pixel_at(rgba, width, x, y);
    for row in 0..h {
        for col in 0..w {
            if pixel_at(rgba, width, x + col, y + row) != first {
                return None;
            }
        }
    }
    Some(first)
}

fn pixel_at(rgba: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y as usize) * (width as usize) + x as usize) * 4;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

fn raster_image(doc: &Document, index: usize) -> Option<String> {
    let (width, height, rgba) = doc.layer_rgba(index)?;
    let box_ = ink_bounds(&rgba, width, height)?;
    let (x, y, w, h) = box_;
    if let Some(color) = uniform_color(&rgba, width, box_) {
        return Some(format!(
            "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" \
fill=\"rgb({},{},{})\" fill-opacity=\"{}\" />",
            color[0],
            color[1],
            color[2],
            color[3] as f32 / 255.0
        ));
    }
    let cropped = crop(&rgba, width, box_);
    let png = png_bytes(&cropped, w, h)?;
    Some(format!(
        "<image x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" \
href=\"data:image/png;base64,{}\" />",
        base64(&png)
    ))
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
