//! One way to ask "what colour is this layer at this pixel", whatever the layer is made of.
//!
//! Every select tool answers against the active layer's visible pixels. A raster or text layer
//! already has a `TileGrid` to read; a vector layer has parameters instead, and is sampled by
//! evaluating the same distance functions the board and the exporter use — nothing is written
//! back, so the `VectorItem` stays editable.
//!
//! The layer is resolved **once**, in [`LayerSelectSample::new`]. That matters: a colour-range
//! walk asks per pixel over the layer's whole painted box, and re-deriving the source, the
//! pivot and the bounds each time turned a masked layer's cached-bounds mutex into a lock per
//! pixel across every rayon row.

use crate::layer::{Layer, LayerContent};
use crate::limits::LAYER_PICK_MIN_ALPHA;
use crate::selection::SelectionShape;
use crate::selection_mask::SelectionMask;
use crate::tile::{DocRect, TileGrid};
use crate::transform::{bounds_center, LayerTransform};
use crate::vector::VectorItem;

/// Where the pixels come from. Both arms are borrowed from the layer, so a sample never copies
/// the artwork it is reading.
enum Source<'a> {
    Tiles(&'a TileGrid),
    Vector(&'a VectorItem),
}

pub struct LayerSelectSample<'a> {
    /// The layer's painted box in document space, clipped to the document. Nothing outside it
    /// is ever asked of the source, so a walk is bounded by the artwork rather than the canvas.
    pub scope: DocRect,
    source: Source<'a>,
    /// Pivot and transform, resolved once. `None` on the common untransformed layer, which then
    /// costs nothing per pixel.
    placement: Option<((f32, f32), LayerTransform)>,
}

impl<'a> LayerSelectSample<'a> {
    pub fn new(layer: &'a Layer, doc_bounds: DocRect) -> Option<Self> {
        let raw = layer.content_bounds()?;
        let aabb = layer.transform.unwrap_or_default().transformed_aabb(raw);
        let scope = DocRect::from_floats(aabb.0, aabb.1, aabb.2, aabb.3).intersect(doc_bounds)?;
        let source = match &layer.content {
            LayerContent::Vector(item) => Source::Vector(item),
            LayerContent::Raster(tiles) | LayerContent::Text { tiles, .. } => Source::Tiles(tiles),
        };
        let placement = layer
            .transform
            .filter(|t| !t.is_identity())
            .map(|t| (bounds_center(raw), t));
        Some(Self {
            scope,
            source,
            placement,
        })
    }

    pub fn pixel(&self, x: i32, y: i32) -> [u8; 4] {
        if !self.scope.contains(x, y) {
            return [0, 0, 0, 0];
        }
        let point = (x as f32 + 0.5, y as f32 + 0.5);
        let (lx, ly) = match self.placement {
            Some((pivot, t)) => t.inverse(pivot, point),
            None => point,
        };
        match self.source {
            Source::Tiles(tiles) => tiles.get_pixel(lx.floor() as i32, ly.floor() as i32),
            Source::Vector(item) => vector_pixel(item, lx, ly),
        }
    }

    /// Whether there is enough ink here to belong to the artwork, on the same threshold picking
    /// a layer by clicking it uses — so what a marquee keeps and what a click can grab are the
    /// same answer about the same pixel.
    pub fn opaque_enough(&self, x: i32, y: i32) -> bool {
        self.pixel(x, y)[3] >= LAYER_PICK_MIN_ALPHA
    }
}

/// The layer's painted box, clipped to the document — `None` when there is nothing painted at
/// all. Callers that only need to know *whether* a layer has ink ask this rather than building
/// a whole sample.
pub fn painted_scope(layer: &Layer, doc_bounds: DocRect) -> Option<DocRect> {
    let raw = layer.content_bounds()?;
    let aabb = layer.transform.unwrap_or_default().transformed_aabb(raw);
    DocRect::from_floats(aabb.0, aabb.1, aabb.2, aabb.3).intersect(doc_bounds)
}

fn vector_pixel(item: &VectorItem, x: f32, y: f32) -> [u8; 4] {
    let coverage = item.coverage(x, y);
    if coverage <= 0.0 {
        return [0, 0, 0, 0];
    }
    let mut src = item.color();
    src[3] = ((src[3] as f32) * coverage).round().clamp(0.0, 255.0) as u8;
    src
}

/// A geometric selection, kept only where the layer actually has ink — the rule that makes a
/// marquee hug the artwork instead of taking the transparent box around it.
pub fn selection_from_geometry(
    layer: &Layer,
    doc_bounds: DocRect,
    geom: &SelectionShape,
) -> Option<SelectionMask> {
    let sample = LayerSelectSample::new(layer, doc_bounds)?;
    let scope = geom
        .bounds()
        .intersect(doc_bounds)?
        .intersect(sample.scope)?;
    let w = (scope.max_x - scope.min_x + 1) as u32;
    let h = (scope.max_y - scope.min_y + 1) as u32;
    SelectionMask::from_predicate((scope.min_x, scope.min_y), w, h, |x, y| {
        geom.contains(x as f32 + 0.5, y as f32 + 0.5) && sample.opaque_enough(x, y)
    })
    .finish()
}

/// Drops the points a polygon does not need: repeats from a pointer that did not move, and
/// midpoints that sit on the line between their neighbours. Both are free to remove and both
/// are paid for on every `contains` — a lasso is tested once per pixel of its own bounding box.
pub fn simplify_lasso_points(points: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    let mut out: Vec<(f32, f32)> = Vec::with_capacity(points.len());
    for p in points {
        if !p.0.is_finite() || !p.1.is_finite() {
            continue;
        }
        if out.last().copied() == Some(p) {
            continue;
        }
        if let [.., a, b] = out.as_slice() {
            if collinear(*a, *b, p) {
                out.pop();
            }
        }
        out.push(p);
    }
    out
}

/// How far a point may sit off the line through its neighbours and still be dropped, as twice
/// the triangle's area. A pointer sampled at display rate wobbles well inside a document pixel
/// on a straight drag, and keeping those points changes nothing about which pixels are inside.
const COLLINEAR_AREA_EPSILON: f32 = 0.5;

fn collinear(a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
    let cross = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
    cross.abs() <= COLLINEAR_AREA_EPSILON
}
