use crate::shape::{sd_polygon, sd_segment, Shape, Tool};
use crate::transform::LayerTransform;

/// A freehand path: the points the pointer actually travelled, kept as points rather than
/// stamped into pixels. Stroked by default; `fill` closes and fills it, which only the
/// flattening path implements (see [`VectorItem`]).
#[derive(Clone, Debug, PartialEq)]
pub struct VectorPath {
    pub points: Vec<(f32, f32)>,
    pub closed: bool,
    pub fill: bool,
    pub color: [u8; 4],
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
}

#[derive(Clone, Debug, PartialEq)]
pub enum VectorItem {
    Path(VectorPath),
    Shape(VectorShape),
}

impl VectorItem {
    pub fn color(&self) -> [u8; 4] {
        match self {
            Self::Path(p) => p.color,
            Self::Shape(s) => s.color,
        }
    }

    /// Untransformed extent in the layer's own space, padded by whatever the stroke adds.
    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        match self {
            Self::Shape(s) => Some(s.shape.bounds()),
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
                // A filled polygon ends at its points; a stroked one is half a stroke
                // wider on every side, plus a pixel for the antialiased edge.
                let pad = if p.fill && p.closed {
                    0.0
                } else {
                    p.stroke_width * 0.5 + 1.0
                };
                Some((min.0 - pad, min.1 - pad, max.0 + pad, max.1 + pad))
            }
        }
    }

    /// Signed distance to the item's ink **in its own space**: negative inside, zero on the
    /// edge. Coverage antialiases it; picking compares it against a slack radius so a
    /// hairline can still be grabbed.
    pub fn distance(&self, x: f32, y: f32) -> f32 {
        match self {
            Self::Shape(s) => s.shape.distance(x, y),
            Self::Path(p) => path_distance(p, x, y),
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
                dst.stroke_width = src.stroke_width;
            }
            (dst, src) => {
                *dst = src.clone();
                dst.translate(dx, dy);
            }
        }
    }
}

fn path_distance(path: &VectorPath, x: f32, y: f32) -> f32 {
    let p = (x, y);
    if path.points.len() < 2 {
        let Some(&a) = path.points.first() else {
            return f32::MAX;
        };
        return sd_segment(p, a, a) - path.stroke_width * 0.5;
    }
    if path.closed && path.fill {
        return sd_polygon(p, &path.points);
    }
    let mut d = f32::MAX;
    for pair in path.points.windows(2) {
        d = d.min(sd_segment(p, pair[0], pair[1]));
    }
    if path.closed {
        if let (Some(&first), Some(&last)) = (path.points.first(), path.points.last()) {
            d = d.min(sd_segment(p, last, first));
        }
    }
    d - path.stroke_width * 0.5
}

pub fn items_bounds(items: &[VectorItem]) -> Option<(f32, f32, f32, f32)> {
    let mut acc: Option<(f32, f32, f32, f32)> = None;
    for item in items {
        let Some(b) = item.bounds() else {
            continue;
        };
        acc = Some(match acc {
            None => b,
            Some(a) => (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)),
        });
    }
    acc
}

/// The four corners of `items_bounds` after `transform`, used to find the document-space
/// region a transformed vector layer can touch.
pub fn transformed_bounds(
    items: &[VectorItem],
    transform: Option<LayerTransform>,
) -> Option<(f32, f32, f32, f32)> {
    let raw = items_bounds(items)?;
    let Some(t) = transform.filter(|t| !t.is_identity()) else {
        return Some(raw);
    };
    let pivot = crate::transform::bounds_center(raw);
    let corners = t.transformed_corners(pivot, raw);
    let mut min = corners[0];
    let mut max = corners[0];
    for &(x, y) in &corners[1..] {
        min.0 = min.0.min(x);
        min.1 = min.1.min(y);
        max.0 = max.0.max(x);
        max.1 = max.1.max(y);
    }
    Some((min.0, min.1, max.0, max.1))
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
    items: &[VectorItem],
    transform: Option<LayerTransform>,
    buf: &mut [u8],
    width: u32,
    height: u32,
) {
    let Some(raw) = items_bounds(items) else {
        return;
    };
    let transform = transform.filter(|t| !t.is_identity());
    let pivot = crate::transform::bounds_center(raw);
    let Some((bx0, by0, bx1, by1)) = transformed_bounds(items, transform) else {
        return;
    };
    let x0 = (bx0.floor().max(0.0) as u32).min(width);
    let y0 = (by0.floor().max(0.0) as u32).min(height);
    let x1 = ((bx1.ceil() + 1.0).max(0.0) as u32).min(width);
    let y1 = ((by1.ceil() + 1.0).max(0.0) as u32).min(height);

    for y in y0..y1 {
        for x in x0..x1 {
            let (lx, ly) = match transform {
                Some(t) => t.inverse(pivot, (x as f32 + 0.5, y as f32 + 0.5)),
                None => (x as f32 + 0.5, y as f32 + 0.5),
            };
            let i = ((y as usize) * (width as usize) + (x as usize)) * 4;
            for item in items {
                let coverage = item.coverage(lx, ly);
                if coverage <= 0.0 {
                    continue;
                }
                let mut src = item.color();
                src[3] = ((src[3] as f32) * coverage).round().clamp(0.0, 255.0) as u8;
                if src[3] == 0 {
                    continue;
                }
                let dst = [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]];
                let out = crate::tile::blend_over(dst, src);
                buf[i..i + 4].copy_from_slice(&out);
            }
        }
    }
}

/// Build a vector item from a shape the user just finished dragging. Selection tools and
/// the non-drawing tools have no vector form.
pub fn item_from_shape(shape: Shape, color: [u8; 4]) -> Option<VectorItem> {
    if !shape.tool.is_shape() {
        return None;
    }
    Some(VectorItem::Shape(VectorShape { shape, color }))
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
        stroke_width,
    }))
}

fn svg_paint(color: [u8; 4], stroke_width: f32, filled: bool) -> String {
    let alpha = color[3] as f32 / 255.0;
    let (r, g, b) = (color[0], color[1], color[2]);
    if filled {
        format!("fill=\"rgb({r},{g},{b})\" fill-opacity=\"{alpha}\"")
    } else {
        format!(
            "fill=\"none\" stroke=\"rgb({r},{g},{b})\" stroke-opacity=\"{alpha}\" stroke-width=\"{stroke_width}\""
        )
    }
}

fn polygon_svg(verts: &[(f32, f32)], color: [u8; 4], width: f32, filled: bool) -> String {
    let points = verts
        .iter()
        .map(|(x, y)| format!("{x},{y}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "<polygon points=\"{points}\" {} />",
        svg_paint(color, width, filled)
    )
}

/// SVG for one item. A parametric shape emits the matching SVG *primitive* rather than a
/// flattened polyline, so an exported rect stays a `<rect>` and stays editable in whatever
/// opens it — the same reason the shape is stored as parameters in the first place.
pub fn item_svg(item: &VectorItem) -> Option<String> {
    match item {
        VectorItem::Path(p) => {
            let (&first, rest) = p.points.split_first()?;
            let mut d = format!("M {} {}", first.0, first.1);
            for &(x, y) in rest {
                d.push_str(&format!(" L {x} {y}"));
            }
            if p.closed {
                d.push_str(" Z");
            }
            Some(format!(
                "<path d=\"{d}\" {} />",
                svg_paint(p.color, p.stroke_width, p.fill)
            ))
        }
        VectorItem::Shape(s) => {
            let shape = s.shape;
            let (x0, y0) = shape.start;
            let (x1, y1) = shape.end;
            let (min_x, min_y) = (x0.min(x1), y0.min(y1));
            let (w, h) = ((x1 - x0).abs(), (y1 - y0).abs());
            let paint = svg_paint(s.color, shape.half_width * 2.0, shape.fill);
            Some(match shape.tool {
                Tool::Rect => {
                    format!(
                        "<rect x=\"{min_x}\" y=\"{min_y}\" width=\"{w}\" height=\"{h}\" {paint} />"
                    )
                }
                Tool::Ellipse => format!(
                    "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\" {paint} />",
                    min_x + w * 0.5,
                    min_y + h * 0.5,
                    w * 0.5,
                    h * 0.5
                ),
                Tool::Line => {
                    format!("<line x1=\"{x0}\" y1=\"{y0}\" x2=\"{x1}\" y2=\"{y1}\" {paint} />")
                }
                Tool::Triangle => polygon_svg(
                    &shape.triangle_vertices(),
                    s.color,
                    shape.half_width * 2.0,
                    shape.fill,
                ),
                Tool::Pentagon => polygon_svg(
                    &shape.pentagon_vertices(),
                    s.color,
                    shape.half_width * 2.0,
                    shape.fill,
                ),
                Tool::Arrow => {
                    let verts = shape.arrow_outline();
                    polygon_svg(&verts, s.color, shape.half_width * 2.0, false)
                }
                _ => return None,
            })
        }
    }
}

/// A layer transform exported as an SVG `<g transform=...>` rather than baked into every
/// coordinate — an SVG group carries translate/rotate/scale natively, so the exported file
/// stays as editable as the layer is.
pub fn svg_transform_attr(
    items: &[VectorItem],
    transform: Option<LayerTransform>,
) -> Option<String> {
    let t = transform.filter(|t| !t.is_identity())?;
    let pivot = crate::transform::bounds_center(items_bounds(items)?);
    let degrees = t.rotation.to_degrees();
    Some(format!(
        "<g transform=\"translate({} {}) translate({} {}) rotate({}) scale({} {}) translate({} {})\">",
        t.offset_x, t.offset_y, pivot.0, pivot.1, degrees, t.scale_x, t.scale_y, -pivot.0, -pivot.1
    ))
}

/// An eraser has no vector meaning — there is nothing to subtract from parameters — so the
/// tools that can produce vector content are exactly the shapes plus the pen.
pub fn tool_makes_vector(tool: Tool) -> bool {
    tool.is_shape() || tool == Tool::Pen
}
