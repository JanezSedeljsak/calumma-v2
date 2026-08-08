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
