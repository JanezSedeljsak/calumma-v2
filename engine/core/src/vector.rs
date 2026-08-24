use crate::shape::{ink_sample, sd_polygon, sd_segment, Shape, Tool};
use crate::transform::LayerTransform;
use rayon::prelude::*;

/// A freehand path: the points the pointer actually travelled, kept as points rather than
/// stamped into pixels. Stroked by default; `fill` closes and fills it in `color`, which
/// only the flattening path implements (see [`VectorItem`]). The two are independent, so a
/// filled path can also carry an outline in `stroke_color`.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorPath {
    pub points: Vec<(f32, f32)>,
    pub closed: bool,
    pub fill: bool,
    pub color: [u8; 4],
    pub stroke: bool,
    pub stroke_color: [u8; 4],
    pub stroke_width: f32,
}

/// A parametric shape: the two drag endpoints and the style, which is all a rect, ellipse,
/// line, arrow, triangle or pentagon needs. Storing the *parameters* rather than a
/// flattened polyline is what makes the shape resolution-independent — `board.wgsl`
/// evaluates the same signed-distance function `Shape::distance` does, so the shape is
/// re-derived at whatever size it is being viewed or exported at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VectorShape {
    pub shape: Shape,
    pub color: [u8; 4],
    pub stroke_color: [u8; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub enum VectorItem {
    Path(VectorPath),
    Shape(VectorShape),
}

impl VectorItem {
    /// The item's fill color — what a swatch of it would show. The stroke has its own
    /// (`stroke_color`); painting the item goes through [`samples`](Self::samples), which
    /// reads both.
    pub fn color(&self) -> [u8; 4] {
        match self {
            Self::Path(p) => p.color,
            Self::Shape(s) => s.color,
        }
    }

    /// The item's outline color, the sibling of [`color`](Self::color). Both exist because a
    /// shape carries a fill and a stroke independently — a white rectangle with a black
    /// border is one item, not two.
    pub fn stroke_color(&self) -> [u8; 4] {
        match self {
            Self::Path(p) => p.stroke_color,
            Self::Shape(s) => s.stroke_color,
        }
    }

    /// Untransformed extent in the layer's own space, padded by whatever the stroke adds.
    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let (x0, y0, x1, y1) = self.geometry_bounds()?;
        let pad = self.ink_pad();
        Some((x0 - pad, y0 - pad, x1 + pad, y1 + pad))
    }

    /// The extent of the item's *geometry* alone — a shape's two endpoints, a path's points —
    /// with no allowance for how far the ink around it runs. This is the box a resize works
    /// on, because stroke width is not something a corner drag changes.
    pub fn geometry_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        match self {
            Self::Shape(s) => {
                let (a, b) = (s.shape.start, s.shape.end);
                Some((a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1)))
            }
            Self::Path(p) => {
                let (&first, rest) = p.points.split_first()?;
                let mut min = first;
                let mut max = first;
                for &(x, y) in rest {
                    min.0 = min.0.min(x);
                    min.1 = min.1.min(y);
                    max.0 = max.0.max(x);
                    max.1 = max.1.max(y);
                }
                Some((min.0, min.1, max.0, max.1))
            }
        }
    }

    /// How far past its geometry the item's ink reaches: half the stroke plus a pixel for the
    /// antialiased edge, and for an arrow the head that hangs off the end. A filled closed
    /// polygon ends at its points and adds nothing.
    ///
    /// A resize leaves this alone — the ink keeps its weight the way it does in Figma and
    /// Photoshop — which is exactly why `set_scaled` can take it off both the box and the
    /// pointer and land the dragged corner where the pointer actually is.
    pub fn ink_pad(&self) -> f32 {
        match self {
            Self::Shape(s) => s.shape.padding(),
            Self::Path(p) if !p.stroke => 0.0,
            Self::Path(p) => p.stroke_width * 0.5 + 1.0,
        }
    }

    /// Signed distance to the item's ink **in its own space**: negative inside, zero on the
    /// edge. Coverage antialiases it; picking compares it against a slack radius so a
    /// hairline can still be grabbed.
    pub fn distance(&self, x: f32, y: f32) -> f32 {
        match self {
            Self::Shape(s) => s.shape.distance(x, y),
            Self::Path(p) => match (path_fill_distance(p, x, y), path_stroke_distance(p, x, y)) {
                (Some(fill), Some(stroke)) => fill.min(stroke),
                (Some(d), None) | (None, Some(d)) => d,
                (None, None) => f32::MAX,
            },
        }
    }

    /// The item's paint at a point, fill first then stroke, each as a straight-alpha source
    /// color ready to blend over what is already there. Two entries rather than one because
    /// the two parts have their own colors — this is the CPU twin of `board.wgsl`'s
    /// `shape_ink`, and the reason a flattened export shows the same border a live board does.
    pub fn samples(&self, x: f32, y: f32) -> [Option<[u8; 4]>; 2] {
        match self {
            Self::Shape(s) => [
                ink_sample(s.shape.fill_distance(x, y), s.color),
                ink_sample(s.shape.stroke_distance(x, y), s.stroke_color),
            ],
            Self::Path(p) => [
                ink_sample(path_fill_distance(p, x, y), p.color),
                ink_sample(path_stroke_distance(p, x, y), p.stroke_color),
            ],
        }
    }

    /// Distance for *picking*, which is not the same question as coverage. A closed shape
    /// counts as solid whether or not it is filled — clicking inside an outlined rectangle
    /// grabs the rectangle, because that is what the user thinks they clicked on. Lines,
    /// arrows and freehand paths have no inside, so for them this is the ink distance.
    pub fn pick_distance(&self, x: f32, y: f32) -> f32 {
        match self {
            Self::Shape(s) if s.shape.tool.takes_fill() && !s.shape.fill => Shape {
                fill: true,
                ..s.shape
            }
            .distance(x, y),
            _ => self.distance(x, y),
        }
    }

    /// Ink coverage at a point **in the item's own space**, 0–1 with an antialiased edge.
    /// This is the CPU twin of the shader: same distance functions, same half-pixel band, so
    /// a flattened export matches what the board showed.
    pub fn coverage(&self, x: f32, y: f32) -> f32 {
        (0.5 - self.distance(x, y)).clamp(0.0, 1.0)
    }

    /// Move the item inside its layer. Parameters are the storage, so a move is a move of
    /// the parameters — nothing is resampled and nothing loses sharpness, which is the
    /// whole reason an item stays a vector after it is committed.
    pub fn translate(&mut self, dx: f32, dy: f32) {
        match self {
            Self::Path(p) => {
                for point in &mut p.points {
                    point.0 += dx;
                    point.1 += dy;
                }
            }
            Self::Shape(s) => {
                s.shape.start.0 += dx;
                s.shape.start.1 += dy;
                s.shape.end.0 += dx;
                s.shape.end.1 += dy;
            }
        }
    }

    /// Become `source` moved by `(dx, dy)`. A drag re-derives the item from the capture it
    /// took at pointer-down every frame — exact, with no accumulated rounding — and going
    /// through the existing point buffer keeps that free of allocation even for a freehand
    /// path with thousands of points.
    pub fn set_translated(&mut self, source: &Self, dx: f32, dy: f32) {
        match (self, source) {
            (Self::Path(dst), Self::Path(src)) => {
                dst.points.clear();
                dst.points
                    .extend(src.points.iter().map(|&(x, y)| (x + dx, y + dy)));
                dst.closed = src.closed;
                dst.fill = src.fill;
                dst.color = src.color;
                dst.stroke = src.stroke;
                dst.stroke_color = src.stroke_color;
                dst.stroke_width = src.stroke_width;
            }
            (dst, src) => {
                *dst = src.clone();
                dst.translate(dx, dy);
            }
        }
    }

    /// Become `source` resized about `pivot`. Same re-derive-from-pointer-down contract as
    /// `set_translated`, and the same reason for it: parameters are the storage, so a resize
    /// scales the endpoints or the path points and the shape is re-evaluated at its new size
    /// rather than resampled from the size it used to be.
    ///
    /// Ink width is left where it was. Resizing a rectangle in Figma or Photoshop does not
    /// thicken its outline, and here it would also make [`ink_pad`](Self::ink_pad) move under
    /// the drag it is being subtracted from.
    pub fn set_scaled(&mut self, source: &Self, pivot: (f32, f32), scale: (f32, f32)) {
        let map = |p: (f32, f32)| {
            (
                pivot.0 + (p.0 - pivot.0) * scale.0,
                pivot.1 + (p.1 - pivot.1) * scale.1,
            )
        };
        match (self, source) {
            (Self::Path(dst), Self::Path(src)) => {
                dst.points.clear();
                dst.points.extend(src.points.iter().copied().map(map));
                dst.closed = src.closed;
                dst.fill = src.fill;
                dst.color = src.color;
                dst.stroke = src.stroke;
                dst.stroke_color = src.stroke_color;
                dst.stroke_width = src.stroke_width;
            }
            (Self::Shape(dst), Self::Shape(src)) => {
                dst.color = src.color;
                dst.stroke_color = src.stroke_color;
                dst.shape = Shape {
                    start: map(src.shape.start),
                    end: map(src.shape.end),
                    ..src.shape
                };
            }
            (dst, src) => *dst = src.clone(),
        }
    }
}

/// The filled interior of a closed path, or `None` when the path is open or unfilled.
fn path_fill_distance(path: &VectorPath, x: f32, y: f32) -> Option<f32> {
    if !path.fill || !path.closed || path.points.len() < 3 {
        return None;
    }
    Some(sd_polygon((x, y), &path.points))
}

/// The stroked polyline: the nearest segment, widened. Independent of the fill, so a filled
/// path can carry a border and an unfilled one is nothing but this.
fn path_stroke_distance(path: &VectorPath, x: f32, y: f32) -> Option<f32> {
    if !path.stroke {
        return None;
    }
    let p = (x, y);
    let (&first, rest) = path.points.split_first()?;
    let mut d = sd_segment(p, first, first);
    let mut previous = first;
    for &point in rest {
        d = d.min(sd_segment(p, previous, point));
        previous = point;
    }
    if path.closed {
        d = d.min(sd_segment(p, previous, first));
    }
    Some(d - path.stroke_width * 0.5)
}

pub fn transformed_bounds(
    item: &VectorItem,
    transform: Option<LayerTransform>,
) -> Option<(f32, f32, f32, f32)> {
    let raw = item.bounds()?;
    Some(crate::transform::transformed_aabb(raw, transform))
}

/// Whether an item's own signed-distance function can be evaluated by `board.wgsl`.
/// A parametric shape always can. A path is drawn as stroke segments, which the stroke
/// pipeline handles — but an arbitrary *filled* polygon has no GPU path today (the shader
/// only carries fixed-arity `sd_polygon3`/`sd_polygon5`), so those fall back to the
/// rasterizer at flatten time and simply do not appear live. Nothing the tools produce
/// creates one; the case exists only for data built directly in Rust.
pub fn draws_on_gpu(item: &VectorItem) -> bool {
    match item {
        VectorItem::Shape(_) => true,
        VectorItem::Path(p) => !(p.closed && p.fill),
    }
}

/// Rasterize a whole vector layer into a tightly packed document-sized RGBA buffer.
///
/// This is the flatten path — composite, export, PSD, merge-down and thumbnails — not the
/// live view, which draws the same parameters on the GPU instead. Sampling walks the
/// *transformed* bounding box and inverse-maps each pixel back into item space, so the
/// shape is re-evaluated at the destination's resolution rather than resampled from a
/// smaller bitmap. That is the whole point of keeping the parameters: scaling a vector up
/// costs sharpness nothing.
pub fn rasterize_into_rgba(
    item: &VectorItem,
    transform: Option<LayerTransform>,
    buf: &mut [u8],
    width: u32,
    height: u32,
) {
    let Some(raw) = item.bounds() else {
        return;
    };
    let transform = transform.filter(|t| !t.is_identity());
    let pivot = crate::transform::bounds_center(raw);
    let Some(aabb) = transformed_bounds(item, transform) else {
        return;
    };
    let Some((x0, y0, x1, y1)) = crate::transform::clipped_pixel_span(aabb, width, height) else {
        return;
    };

    let row_bytes = (width as usize) * 4;
    let y0 = y0 as usize;
    let x0 = x0 as usize;
    let x1 = x1 as usize;
    buf[y0 * row_bytes..(y1 as usize) * row_bytes]
        .par_chunks_mut(row_bytes)
        .enumerate()
        .for_each(|(i, row)| {
            let y = y0 + i;
            for x in x0..x1 {
                let (lx, ly) = match transform {
                    Some(t) => t.inverse(pivot, (x as f32 + 0.5, y as f32 + 0.5)),
                    None => (x as f32 + 0.5, y as f32 + 0.5),
                };
                let i = x * 4;
                for src in item.samples(lx, ly).into_iter().flatten() {
                    let dst = [row[i], row[i + 1], row[i + 2], row[i + 3]];
                    let out = crate::tile::blend_over(dst, src);
                    row[i..i + 4].copy_from_slice(&out);
                }
            }
        });
}

/// Build a vector item from a shape the user just finished dragging. Selection tools and
/// the non-drawing tools have no vector form.
pub fn item_from_shape(shape: Shape, color: [u8; 4], stroke_color: [u8; 4]) -> Option<VectorItem> {
    if !shape.tool.is_shape() {
        return None;
    }
    Some(VectorItem::Shape(VectorShape {
        shape,
        color,
        stroke_color,
    }))
}

pub fn item_from_points(
    points: &[(f32, f32)],
    color: [u8; 4],
    stroke_width: f32,
) -> Option<VectorItem> {
    if points.is_empty() {
        return None;
    }
    Some(VectorItem::Path(VectorPath {
        points: points.to_vec(),
        closed: false,
        fill: false,
        color,
        stroke: true,
        stroke_color: color,
        stroke_width,
    }))
}

/// An eraser has no vector meaning — there is nothing to subtract from parameters — so the
/// tools that can produce vector content are exactly the shapes plus the pen.
pub fn tool_makes_vector(tool: Tool) -> bool {
    tool.is_shape() || tool == Tool::Pen
}
