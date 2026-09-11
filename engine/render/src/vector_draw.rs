use crate::compose::{box_overlay_instances, brush_params, rgba_unit, StrokeInstance};
use bytemuck::{Pod, Zeroable};
use calumma_core::tile::DocRect;
use calumma_core::transform::bounds_center;
use calumma_core::{
    BrushProfile, Document, Layer, LayerTransform, VectorItem, VectorPath, VectorShape,
};

/// One parametric vector item, as an instance the board shader re-evaluates per pixel. The
/// same fields `Shape::distance` takes, so live board and flattened export read one geometry
/// definition — one instance, one draw.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct VectorShapeInstance {
    pub p0: [f32; 2],
    pub p1: [f32; 2],
    pub color: [f32; 4],
    pub stroke_color: [f32; 4],
    pub half_width: f32,
    pub tool: f32,
    pub fill: f32,
    pub stroke: f32,
}

/// Where a layer's items sit in document space. Items are stored in the layer's own space;
/// this carries the pivot and transform needed to place them, and is `None` when the layer
/// has no transform at all — the common case, which then costs nothing per point.
pub type VectorPlacement = Option<((f32, f32), LayerTransform)>;

pub fn vector_placement(layer: &Layer) -> VectorPlacement {
    let t = layer.transform.filter(|t| !t.is_identity())?;
    let raw = layer.content.item()?.bounds()?;
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
    if !path.stroke {
        return;
    }
    let color = rgba_unit(path.stroke_color);
    let radius = path.stroke_width * 0.5 * placement_scale(placement);
    let mut segment = |a: (f32, f32), b: (f32, f32)| {
        let (a, b) = (place(placement, a), place(placement, b));
        out.push(StrokeInstance {
            segment: [a.0, a.1, b.0, b.1],
            color,
            brush: brush_params(radius, &BrushProfile::HARD),
        });
    };
    for ring in path.rings() {
        match ring {
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
}

pub const FILL_FLAG_FILL: u32 = 1;
pub const FILL_FLAG_STROKE: u32 = 2;
pub const FILL_FLAG_EVEN_ODD: u32 = 4;
pub const FILL_INSTANCE_CAPACITY: usize = 64;
pub const FILL_EDGE_CAPACITY: usize = 4096;

/// A filled path as one instance: the box it can paint in, and the run of the shared edge
/// buffer holding every edge of every ring, already placed in document space. The fragment
/// shader walks that run for the distance and the winding number together, so rotation is
/// exact here — the edges were rotated on the CPU — unlike a parametric shape's.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct VectorFillInstance {
    pub lo: [f32; 2],
    pub hi: [f32; 2],
    pub color: [f32; 4],
    pub stroke_color: [f32; 4],
    pub half_width: f32,
    pub first_edge: u32,
    pub edge_count: u32,
    pub flags: u32,
}

pub fn push_fill_instance(
    path: &VectorPath,
    placement: VectorPlacement,
    fills: &mut Vec<VectorFillInstance>,
    edges: &mut Vec<[f32; 4]>,
) {
    let first_edge = edges.len();
    let mut lo = (f32::MAX, f32::MAX);
    let mut hi = (f32::MIN, f32::MIN);
    for ring in path.rings() {
        for (i, &a) in ring.iter().enumerate() {
            let (a, b) = (
                place(placement, a),
                place(placement, ring[(i + 1) % ring.len()]),
            );
            lo = (lo.0.min(a.0), lo.1.min(a.1));
            hi = (hi.0.max(a.0), hi.1.max(a.1));
            edges.push([a.0, a.1, b.0, b.1]);
        }
    }
    let edge_count = edges.len() - first_edge;
    if edge_count == 0 {
        return;
    }
    let half_width = if path.stroke {
        path.stroke_width * 0.5 * placement_scale(placement)
    } else {
        0.0
    };
    let pad = half_width + 1.0;
    let mut flags = 0;
    if path.fill {
        flags |= FILL_FLAG_FILL;
    }
    if path.stroke {
        flags |= FILL_FLAG_STROKE;
    }
    if path.even_odd {
        flags |= FILL_FLAG_EVEN_ODD;
    }
    fills.push(VectorFillInstance {
        lo: [lo.0 - pad, lo.1 - pad],
        hi: [hi.0 + pad, hi.1 + pad],
        color: rgba_unit(path.color),
        stroke_color: rgba_unit(path.stroke_color),
        half_width,
        first_edge: first_edge as u32,
        edge_count: edge_count as u32,
        flags,
    });
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
        stroke_color: rgba_unit(shape.stroke_color),
        half_width: shape.shape.half_width * placement_scale(placement),
        tool: shape.shape.tool as u32 as f32,
        fill: f32::from(u8::from(shape.shape.fill)),
        stroke: f32::from(u8::from(shape.shape.stroke)),
    }
}

/// The frame around the selected item, drawn only inside `⌘T` — its corners are what resizes
/// the item, and plain Move does not resize, so under Move a vector drags frameless like any
/// painted layer. No rotate stalk: item rotation is not a thing the board can draw yet, so the
/// frame does not offer it.
pub fn vector_selection_instances(doc: &Document) -> Vec<StrokeInstance> {
    if !doc.transform_active {
        return Vec::new();
    }
    let Some(corners) = doc.selected_vector_item_corners() else {
        return Vec::new();
    };
    box_overlay_instances(corners, None)
}
