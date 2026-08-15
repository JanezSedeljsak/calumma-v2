use crate::compose::{rgba_unit, StrokeInstance};
use bytemuck::{Pod, Zeroable};
use calumma_core::tile::DocRect;
use calumma_core::transform::bounds_center;
use calumma_core::vector::items_bounds;
use calumma_core::{Document, Layer, LayerTransform, VectorItem, VectorPath, VectorShape};

const SELECTION_COLOR: [f32; 4] = [0.24, 0.78, 0.84, 0.95];
const SELECTION_WIDTH: f32 = 1.0;
const SELECTION_CORNER_RADIUS: f32 = 2.5;

/// One parametric vector item, as an instance the board shader re-evaluates per pixel. The
/// same fields `Shape::distance` takes, so live board and flattened export read one geometry
/// definition — and one instance per item means a layer of shapes is a single draw call
/// rather than a uniform rewrite and a render pass each.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct VectorShapeInstance {
    pub p0: [f32; 2],
    pub p1: [f32; 2],
    pub color: [f32; 4],
    pub half_width: f32,
    pub tool: f32,
    pub fill: f32,
    pub _pad: f32,
}

/// Where a layer's items sit in document space. Items are stored in the layer's own space;
/// this carries the pivot and transform needed to place them, and is `None` when the layer
/// has no transform at all — the common case, which then costs nothing per point.
pub type VectorPlacement = Option<((f32, f32), LayerTransform)>;

pub fn vector_placement(layer: &Layer) -> VectorPlacement {
    let t = layer.transform.filter(|t| !t.is_identity())?;
    let raw = items_bounds(layer.content.items()?)?;
    Some((bounds_center(raw), t))
}

fn place(placement: VectorPlacement, p: (f32, f32)) -> (f32, f32) {
    match placement {
        Some((pivot, t)) => t.forward(pivot, p),
        None => p,
    }
}

fn placement_scale(placement: VectorPlacement) -> f32 {
    match placement {
        Some((_, t)) => (t.scale_x.abs() + t.scale_y.abs()) * 0.5,
        None => 1.0,
    }
}

/// Whether any of the item can reach the visible board. Tile layers have been culled against
/// the viewport since the beginning; a vector layer that has grown to thousands of items
/// deserves the same treatment, and its bounds are already parametric — no pixels are read to
/// answer this.
pub fn item_visible(item: &VectorItem, placement: VectorPlacement, visible: DocRect) -> bool {
    let Some(bounds) = item.bounds() else {
        return false;
    };
    let (x0, y0, x1, y1) = match placement {
        None => bounds,
        Some((pivot, t)) => {
            let corners = t.transformed_corners(pivot, bounds);
            corners
                .iter()
                .fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |acc, &(x, y)| {
                    (acc.0.min(x), acc.1.min(y), acc.2.max(x), acc.3.max(y))
                })
        }
    };
    DocRect::from_floats(x0, y0, x1, y1).intersects(visible)
}

/// A freehand path as stroke segments. The shader has no arbitrary-polygon SDF, so a path is
/// drawn the way it was made — segment by segment — through the same instanced pipeline the
/// live pen preview and every board overlay already use. Points are placed as they are read
/// rather than into a temporary buffer: this runs per visible path per frame, and two extra
/// multiplies per point cost far less than an allocation.
pub fn push_path_instances(
    path: &VectorPath,
    placement: VectorPlacement,
    out: &mut Vec<StrokeInstance>,
) {
    if path.closed && path.fill {
        return;
    }
    let color = rgba_unit(path.color);
    let radius = path.stroke_width * 0.5 * placement_scale(placement);
    let mut segment = |a: (f32, f32), b: (f32, f32)| {
        let (a, b) = (place(placement, a), place(placement, b));
        out.push(StrokeInstance {
            segment: [a.0, a.1, b.0, b.1],
            color,
            radius,
            _pad: [0.0; 3],
        });
    };
    match path.points.as_slice() {
        [] => {}
        [only] => segment(*only, *only),
        points => {
            for pair in points.windows(2) {
                segment(pair[0], pair[1]);
            }
            if path.closed {
                segment(points[points.len() - 1], points[0]);
            }
        }
    }
}

/// A parametric shape, placed. Translation and scale fold into the parameters exactly;
/// rotation cannot — the shader's SDFs are axis-aligned — so a *rotated* vector layer draws
/// its shapes unrotated live while the flattened and exported result stays correct.
pub fn shape_instance(shape: &VectorShape, placement: VectorPlacement) -> VectorShapeInstance {
    let start = place(placement, shape.shape.start);
    let end = place(placement, shape.shape.end);
    VectorShapeInstance {
        p0: [start.0, start.1],
        p1: [end.0, end.1],
        color: rgba_unit(shape.color),
        half_width: shape.shape.half_width * placement_scale(placement),
        tool: shape.shape.tool as u32 as f32,
        fill: if shape.shape.fill { 1.0 } else { 0.0 },
        _pad: 0.0,
    }
}

/// The box around the one item being moved. Corner dots mark it as a *thing you grabbed*
/// rather than the layer-wide transform frame, which draws its own handles.
pub fn vector_selection_instances(doc: &Document) -> Vec<StrokeInstance> {
    let Some(corners) = doc.selected_vector_item_corners() else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(8);
    for i in 0..4 {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        out.push(StrokeInstance {
            segment: [a.0, a.1, b.0, b.1],
            color: SELECTION_COLOR,
            radius: SELECTION_WIDTH,
            _pad: [0.0; 3],
        });
    }
    for p in corners {
        out.push(StrokeInstance {
            segment: [p.0, p.1, p.0, p.1],
            color: SELECTION_COLOR,
            radius: SELECTION_CORNER_RADIUS,
            _pad: [0.0; 3],
        });
    }
    out
}
