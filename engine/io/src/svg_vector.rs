//! An SVG read as editable vector items rather than as pixels.
//!
//! Every `<path>` (and every shape usvg normalises into one — rect, circle, polygon, flattened
//! text) becomes one `VectorPath`, curves flattened to points in document space. The 1:1
//! vector rule means each lands on its own layer; subpaths stay together as rings so a hole
//! is still a hole.
//!
//! Anything a `VectorPath` cannot say — a gradient or pattern paint, an embedded image, a
//! clip path, a mask, a filter — makes the whole file answer `None`, and the caller pastes it
//! as pixels instead. A half-converted drawing that looks wrong is worse than a raster one
//! that looks right.

use crate::raster::looks_like_svg;
use calumma_core::limits::{
    IMPORT_MAX_SIDE, SVG_FLATTEN_MAX_STEPS, SVG_FLATTEN_TOLERANCE_PX, SVG_VECTOR_MAX_PATHS,
};
use calumma_core::{VectorItem, VectorPath};
use resvg::tiny_skia::{PathSegment, Point, Transform};
use resvg::usvg::{self, FillRule, Group, Node, Paint};

pub struct SvgVector {
    pub width: u32,
    pub height: u32,
    pub items: Vec<VectorItem>,
}

pub fn decode_svg_vector(bytes: &[u8]) -> Option<SvgVector> {
    if !looks_like_svg(bytes) {
        return None;
    }
    let tree = usvg::Tree::from_data(bytes, &usvg::Options::default()).ok()?;
    let size = tree.size();
    let (src_w, src_h) = (size.width().max(1.0), size.height().max(1.0));
    let cap = IMPORT_MAX_SIDE as f32;
    let scale = (cap / src_w.max(src_h)).min(1.0);
    let mut items = Vec::new();
    collect(
        tree.root(),
        Transform::from_scale(scale, scale),
        1.0,
        &mut items,
    )?;
    if items.is_empty() {
        return None;
    }
    Some(SvgVector {
        width: (src_w * scale).round().max(1.0) as u32,
        height: (src_h * scale).round().max(1.0) as u32,
        items,
    })
}

fn collect(group: &Group, fit: Transform, opacity: f32, out: &mut Vec<VectorItem>) -> Option<()> {
    if group.clip_path().is_some() || group.mask().is_some() || !group.filters().is_empty() {
        return None;
    }
    let opacity = opacity * group.opacity().get();
    for node in group.children() {
        match node {
            Node::Group(child) => collect(child, fit, opacity, out)?,
            Node::Path(path) if path.is_visible() => push_path(path, fit, opacity, out)?,
            Node::Path(_) => {}
            Node::Text(text) => collect(text.flattened(), fit, opacity, out)?,
            Node::Image(_) => return None,
        }
        if out.len() > SVG_VECTOR_MAX_PATHS {
            return None;
        }
    }
    Some(())
}

fn push_path(
    path: &usvg::Path,
    fit: Transform,
    opacity: f32,
    out: &mut Vec<VectorItem>,
) -> Option<()> {
    let fill = match path.fill() {
        Some(fill) => Some((
            solid(fill.paint(), fill.opacity().get() * opacity)?,
            fill.rule(),
        )),
        None => None,
    };
    let transform = path.abs_transform().post_concat(fit);
    let stroke = match path.stroke() {
        Some(stroke) => Some((
            solid(stroke.paint(), stroke.opacity().get() * opacity)?,
            stroke.width().get() * uniform_scale(transform),
        )),
        None => None,
    };
    if fill.is_none() && stroke.is_none() {
        return Some(());
    }
    let flat = flatten(path.data(), transform);
    if flat.points.len() < 2 {
        return Some(());
    }
    let (color, rule) = fill.unwrap_or((stroke.map_or([0; 4], |s| s.0), FillRule::NonZero));
    let (stroke_color, stroke_width) = stroke.unwrap_or((color, 1.0));
    out.push(VectorItem::Path(VectorPath {
        points: flat.points,
        closed: fill.is_some() || flat.all_closed,
        fill: fill.is_some(),
        color,
        stroke: stroke.is_some(),
        stroke_color,
        stroke_width,
        ring_starts: flat.ring_starts,
        even_odd: rule == FillRule::EvenOdd,
    }));
    Some(())
}

fn solid(paint: &Paint, opacity: f32) -> Option<[u8; 4]> {
    match paint {
        Paint::Color(c) => Some([
            c.red,
            c.green,
            c.blue,
            (opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
        ]),
        Paint::LinearGradient(_) | Paint::RadialGradient(_) | Paint::Pattern(_) => None,
    }
}

/// How much a transform scales a stroke's width: the square root of its area scale, which is
/// exact for uniform scales and the usual compromise for the rest.
fn uniform_scale(t: Transform) -> f32 {
    (t.sx * t.sy - t.kx * t.ky).abs().sqrt()
}

struct Flattened {
    points: Vec<(f32, f32)>,
    ring_starts: Vec<u32>,
    all_closed: bool,
}

fn flatten(data: &resvg::tiny_skia::Path, t: Transform) -> Flattened {
    let map = |mut p: Point| {
        t.map_point(&mut p);
        (p.x, p.y)
    };
    let mut flat = Flattened {
        points: Vec::new(),
        ring_starts: Vec::new(),
        all_closed: true,
    };
    let mut ring_open = false;
    let mut last = (0.0, 0.0);
    for segment in data.segments() {
        match segment {
            PathSegment::MoveTo(p) => {
                flat.all_closed &= !ring_open;
                if !flat.points.is_empty() {
                    flat.ring_starts.push(flat.points.len() as u32);
                }
                last = map(p);
                flat.points.push(last);
                ring_open = true;
            }
            PathSegment::LineTo(p) => {
                last = map(p);
                flat.points.push(last);
            }
            PathSegment::QuadTo(c, p) => {
                let (c, p) = (map(c), map(p));
                let steps = steps_for(0.25 * second_difference(last, c, p));
                for i in 1..=steps {
                    let s = i as f32 / steps as f32;
                    let u = 1.0 - s;
                    flat.points.push((
                        u * u * last.0 + 2.0 * u * s * c.0 + s * s * p.0,
                        u * u * last.1 + 2.0 * u * s * c.1 + s * s * p.1,
                    ));
                }
                last = p;
            }
            PathSegment::CubicTo(c1, c2, p) => {
                let (c1, c2, p) = (map(c1), map(c2), map(p));
                let bend = second_difference(last, c1, c2).max(second_difference(c1, c2, p));
                let steps = steps_for(0.75 * bend);
                for i in 1..=steps {
                    let s = i as f32 / steps as f32;
                    let u = 1.0 - s;
                    let (a, b, c, d) = (u * u * u, 3.0 * u * u * s, 3.0 * u * s * s, s * s * s);
                    flat.points.push((
                        a * last.0 + b * c1.0 + c * c2.0 + d * p.0,
                        a * last.1 + b * c1.1 + c * c2.1 + d * p.1,
                    ));
                }
                last = p;
            }
            PathSegment::Close => ring_open = false,
        }
    }
    flat.all_closed &= !ring_open;
    flat
}

fn second_difference(a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> f32 {
    let (x, y) = (a.0 - 2.0 * b.0 + c.0, a.1 - 2.0 * b.1 + c.1);
    (x * x + y * y).sqrt()
}

/// Chords needed so a curve whose flatness bound is `bound / n²` stays within tolerance.
fn steps_for(bound: f32) -> u32 {
    let n = (bound / SVG_FLATTEN_TOLERANCE_PX).sqrt().ceil();
    (n as u32).clamp(1, SVG_FLATTEN_MAX_STEPS)
}
