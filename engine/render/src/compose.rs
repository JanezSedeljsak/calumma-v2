use crate::tile_atlas::tile_mip_levels;
use bytemuck::{Pod, Zeroable};
use calumma_core::filters::AdjustmentLut;
use calumma_core::tile::{TileCoord, TILE_BYTES, TILE_SIZE};
use calumma_core::{
    Document, Layer, Selection, SelectionShape, StrokePoint, Tool, TransformHandles,
};

const TRANSFORM_OUTLINE_COLOR: [f32; 4] = [0.24, 0.78, 0.84, 0.95];
const TRANSFORM_OUTLINE_WIDTH: f32 = 1.0;
const TRANSFORM_HANDLE_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const TRANSFORM_HANDLE_RADIUS: f32 = 4.0;

const TEXT_BOX_COLOR: [f32; 4] = [0.24, 0.78, 0.84, 0.45];
const TEXT_BOX_WIDTH: f32 = 0.5;
const TEXT_CARET_WIDTH: f32 = 1.0;
/// Seconds for one on-off caret cycle. The blink runs off the renderer clock rather than a
/// shell timer, so nothing outside the engine has to know a caret exists.
const TEXT_CARET_BLINK_SECONDS: f32 = 1.06;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct StrokeInstance {
    pub segment: [f32; 4],
    pub color: [f32; 4],
    pub radius: f32,
    pub _pad: [f32; 3],
}

pub fn rgba_unit(rgba: [u8; 4]) -> [f32; 4] {
    [
        rgba[0] as f32 / 255.0,
        rgba[1] as f32 / 255.0,
        rgba[2] as f32 / 255.0,
        rgba[3] as f32 / 255.0,
    ]
}

const LAYER_HIGHLIGHT_COLOR: [f32; 4] = [0.24, 0.78, 0.84, 0.85];
const LAYER_HIGHLIGHT_WIDTH: f32 = 1.5;
const LAYER_HIGHLIGHT_DASH: f32 = 8.0;
const LAYER_HIGHLIGHT_GAP: f32 = 8.0;
const LAYER_HIGHLIGHT_SPEED: f32 = 40.0;

pub fn layer_highlight_instances(corners: [(f32, f32); 4], elapsed: f32) -> Vec<StrokeInstance> {
    let phase = elapsed * LAYER_HIGHLIGHT_SPEED;
    let mut out = Vec::with_capacity(32);
    for i in 0..4 {
        out.extend(dashed_edge(
            corners[i],
            corners[(i + 1) % 4],
            phase,
            LAYER_HIGHLIGHT_COLOR,
            LAYER_HIGHLIGHT_WIDTH,
            LAYER_HIGHLIGHT_DASH,
            LAYER_HIGHLIGHT_GAP,
        ));
    }
    out
}

fn dashed_edge(
    a: (f32, f32),
    b: (f32, f32),
    phase: f32,
    color: [f32; 4],
    width: f32,
    dash: f32,
    gap: f32,
) -> Vec<StrokeInstance> {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-6 {
        return Vec::new();
    }
    let ux = dx / len;
    let uy = dy / len;
    let period = dash + gap;
    let mut t = -phase.rem_euclid(period);
    let mut out = Vec::new();
    while t < len {
        let start = t.max(0.0);
        let end = (t + dash).min(len);
        if end > start {
            out.push(StrokeInstance {
                segment: [
                    a.0 + ux * start,
                    a.1 + uy * start,
                    a.0 + ux * end,
                    a.1 + uy * end,
                ],
                color,
                radius: width,
                _pad: [0.0; 3],
            });
        }
        t += period;
    }
    out
}

pub fn transform_overlay_instances(handles: TransformHandles) -> Vec<StrokeInstance> {
    let (_, corners, rotate_handle) = handles;
    let mut out = Vec::with_capacity(4 + 1 + 5);
    let outline = |a: (f32, f32), b: (f32, f32)| StrokeInstance {
        segment: [a.0, a.1, b.0, b.1],
        color: TRANSFORM_OUTLINE_COLOR,
        radius: TRANSFORM_OUTLINE_WIDTH,
        _pad: [0.0; 3],
    };
    for i in 0..4 {
        out.push(outline(corners[i], corners[(i + 1) % 4]));
    }
    let top_mid = (
        (corners[0].0 + corners[1].0) * 0.5,
        (corners[0].1 + corners[1].1) * 0.5,
    );
    out.push(outline(top_mid, rotate_handle));
    for p in corners.iter().chain(std::iter::once(&rotate_handle)) {
        out.push(StrokeInstance {
            segment: [p.0, p.1, p.0, p.1],
            color: TRANSFORM_HANDLE_COLOR,
            radius: TRANSFORM_HANDLE_RADIUS,
            _pad: [0.0; 3],
        });
    }
    out
}

/// The board furniture for a live text session: a hairline box around the run's layout and
/// a caret that blinks. Both are stroke segments, the same primitive the transform overlay
/// and the lasso already draw with — no new pipeline, and nothing drawn in Swift.
pub fn text_overlay_instances(doc: &Document, elapsed: f32) -> Vec<StrokeInstance> {
    let Some((x0, y0, x1, y1)) = doc.text_box() else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(5);
    let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    for i in 0..4 {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        out.push(StrokeInstance {
            segment: [a.0, a.1, b.0, b.1],
            color: TEXT_BOX_COLOR,
            radius: TEXT_BOX_WIDTH,
            _pad: [0.0; 3],
        });
    }
    let visible = (elapsed / TEXT_CARET_BLINK_SECONDS).fract() < 0.5;
    if let (true, Some((a, b))) = (visible, doc.text_caret_segment()) {
        out.push(StrokeInstance {
            segment: [a.0, a.1, b.0, b.1],
            color: rgba_unit(doc.text_caret_color()),
            radius: TEXT_CARET_WIDTH,
            _pad: [0.0; 3],
        });
    }
    out
}

pub fn selection_rect_or_ellipse(doc: &Document) -> Option<([f32; 2], [f32; 2], Tool)> {
    match &doc.selection.as_ref()?.shape {
        SelectionShape::Rect { start, end } => {
            Some(([start.0, start.1], [end.0, end.1], Tool::Rect))
        }
        SelectionShape::Ellipse { start, end } => {
            Some(([start.0, start.1], [end.0, end.1], Tool::Ellipse))
        }
        SelectionShape::Lasso { .. } => None,
    }
}

pub fn selection_lasso_points(doc: &Document) -> Option<Vec<StrokePoint>> {
    let Selection {
        shape: SelectionShape::Lasso { points },
    } = doc.selection.as_ref()?
    else {
        return None;
    };
    let mut closed: Vec<StrokePoint> = points.iter().map(|&(x, y)| StrokePoint { x, y }).collect();
    if let Some(&first) = closed.first() {
        closed.push(first);
    }
    Some(closed)
}

pub fn stroke_instances(
    points: &[StrokePoint],
    radius: f32,
    color: [f32; 4],
) -> Vec<StrokeInstance> {
    if points.is_empty() {
        return Vec::new();
    }
    let instance = |a: &StrokePoint, b: &StrokePoint| StrokeInstance {
        segment: [a.x, a.y, b.x, b.y],
        color,
        radius,
        _pad: [0.0; 3],
    };
    if points.len() == 1 {
        return vec![instance(&points[0], &points[0])];
    }
    points.windows(2).map(|p| instance(&p[0], &p[1])).collect()
}

/// Mask / adjustments / opacity baked into a tile before upload. Returns `None` when the
/// layer needs none of that and the tile's own bytes can go to the GPU untouched — the
/// common case, and the reason this allocates only for layers that are actually filtered.
pub fn composited_tile_payload(
    pixels: &[u8],
    coord: TileCoord,
    layer: &Layer,
    lut: Option<&AdjustmentLut>,
    doc_width: u32,
) -> Option<Vec<u8>> {
    let mask = layer.mask();
    let lut = lut.filter(|l| !l.is_neutral());
    let opacity = layer.opacity;
    if mask.is_none() && lut.is_none() && opacity >= 1.0 {
        return None;
    }
    let mut out = Vec::with_capacity(TILE_BYTES);
    out.extend_from_slice(pixels);
    out.resize(TILE_BYTES, 0);
    let (ox, oy) = coord.origin();
    for ty in 0..TILE_SIZE {
        for tx in 0..TILE_SIZE {
            let x = ox + tx as i32;
            let y = oy + ty as i32;
            let i = ((ty * TILE_SIZE + tx) * 4) as usize;
            if let Some(lut) = lut {
                let rgb = lut.apply([out[i], out[i + 1], out[i + 2]]);
                out[i..i + 3].copy_from_slice(&rgb);
            }
            if x < 0 || y < 0 {
                continue;
            }
            if let Some(mask) = mask {
                let mi = (y as u32)
                    .saturating_mul(doc_width)
                    .saturating_add(x as u32) as usize;
                if let Some(&m) = mask.get(mi) {
                    let a = out[i + 3] as u16 * m as u16 / 255;
                    out[i + 3] = a as u8;
                }
            }
            if opacity < 1.0 {
                let a = (out[i + 3] as f32) * opacity;
                out[i + 3] = a.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    Some(out)
}

/// A tile's full mip chain — `base` as level 0, then each level halved down to 1×1 — for
/// `TileAtlas::write`. Panning or zooming a document out samples these coarser levels instead
/// of raw 256×256 texels through a plain bilinear filter, which is what shimmering/moiré during
/// a pan actually is: minification aliasing from sampling a texture well below its own
/// resolution with nothing pre-filtered to fall back to.
pub fn tile_mip_chain(base: &[u8]) -> Vec<Vec<u8>> {
    let levels = tile_mip_levels();
    let mut out = Vec::with_capacity(levels as usize);
    out.push(base.to_vec());
    let mut side = TILE_SIZE;
    for _ in 1..levels {
        let prev = out.last().expect("just pushed the base level");
        out.push(downsample_box(prev, side));
        side = (side / 2).max(1);
    }
    out
}

/// One 2×2 box-filter pass, straight (non-premultiplied) alpha in and out. Averaging straight
/// RGB directly would let a fully transparent neighbour's colour bleed into a translucent or
/// opaque one — invisible pixels are not "no colour", they are colour nobody sees yet — so each
/// tap is weighted by its own alpha (premultiplied) before averaging, and the result is
/// unpremultiplied back out. This is the same reason blending happens in premultiplied space
/// everywhere else in this renderer.
fn downsample_box(src: &[u8], side: u32) -> Vec<u8> {
    let out_side = (side / 2).max(1);
    let mut out = vec![0u8; (out_side * out_side * 4) as usize];
    for dy in 0..out_side {
        for dx in 0..out_side {
            let sx0 = (dx * 2).min(side - 1);
            let sy0 = (dy * 2).min(side - 1);
            let sx1 = (dx * 2 + 1).min(side - 1);
            let sy1 = (dy * 2 + 1).min(side - 1);

            let mut sum_rgb = [0.0f32; 3];
            let mut sum_a = 0.0f32;
            for (sx, sy) in [(sx0, sy0), (sx1, sy0), (sx0, sy1), (sx1, sy1)] {
                let i = ((sy * side + sx) * 4) as usize;
                let a = src[i + 3] as f32 / 255.0;
                sum_rgb[0] += src[i] as f32 * a;
                sum_rgb[1] += src[i + 1] as f32 * a;
                sum_rgb[2] += src[i + 2] as f32 * a;
                sum_a += a;
            }

            let out_a = sum_a / 4.0;
            let rgb = if out_a > 0.0 {
                sum_rgb.map(|c| (c / (out_a * 4.0)).round().clamp(0.0, 255.0) as u8)
            } else {
                [0u8; 3]
            };
            let di = ((dy * out_side + dx) * 4) as usize;
            out[di..di + 3].copy_from_slice(&rgb);
            out[di + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}
