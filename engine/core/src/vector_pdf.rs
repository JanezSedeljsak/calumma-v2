//! A vector item as PDF path operators. The twin of [`crate::vector_svg`]: same geometry,
//! same fill/stroke split, a different serializer — so a rect exported to PDF is a real PDF
//! path and not a picture of one, exactly as it is a real `<rect>` in SVG.
//!
//! Everything here emits **document** coordinates. `io::pdf` puts one flip matrix at the top
//! of the page content stream, so nothing downstream has to remember that PDF measures y
//! upward from the bottom-left.
use crate::shape::{Shape, Tool};
use crate::transform::{bounds_center, LayerTransform};
use crate::vector::VectorItem;

/// Circle-to-bezier constant: the control-point offset, as a fraction of the radius, that
/// makes a cubic segment approximate a quarter arc. PDF has no ellipse primitive, so this is
/// how `Tool::Ellipse` survives as geometry rather than being flattened to a polygon.
const KAPPA: f32 = 0.552_284_8;

fn n(value: f32) -> String {
    let text = format!("{value:.4}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

fn rgb(color: [u8; 4]) -> (f32, f32, f32) {
    (
        color[0] as f32 / 255.0,
        color[1] as f32 / 255.0,
        color[2] as f32 / 255.0,
    )
}

/// The paint operator for a subpath: fill, stroke, both, or neither. PDF spells the
/// combination as one operator rather than as two attributes, so the either/or that
/// `vector_svg` writes as separate `fill=` / `stroke=` collapses to a single letter here.
fn paint_op(fill: bool, stroke: bool, closed: bool) -> &'static str {
    match (fill && closed, stroke) {
        (true, true) => "B",
        (true, false) => "f",
        (false, true) => "S",
        (false, false) => "n",
    }
}

fn color_ops(fill: Option<[u8; 4]>, stroke: Option<([u8; 4], f32)>) -> String {
    let mut out = String::new();
    if let Some(color) = fill {
        let (r, g, b) = rgb(color);
        out.push_str(&format!("{} {} {} rg ", n(r), n(g), n(b)));
    }
    if let Some((color, width)) = stroke {
        let (r, g, b) = rgb(color);
        out.push_str(&format!(
            "{} {} {} RG {} w ",
            n(r),
            n(g),
            n(b),
            n(width.max(0.0))
        ));
    }
    out
}

fn polygon(verts: &[(f32, f32)]) -> String {
    let Some((first, rest)) = verts.split_first() else {
        return String::new();
    };
    let mut out = format!("{} {} m ", n(first.0), n(first.1));
    for (x, y) in rest {
        out.push_str(&format!("{} {} l ", n(*x), n(*y)));
    }
    out.push_str("h ");
    out
}

fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> String {
    let (ox, oy) = (rx * KAPPA, ry * KAPPA);
    let mut out = format!("{} {} m ", n(cx + rx), n(cy));
    let curve = |x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32| {
        format!(
            "{} {} {} {} {} {} c ",
            n(x1),
            n(y1),
            n(x2),
            n(y2),
            n(x3),
            n(y3)
        )
    };
    out.push_str(&curve(cx + rx, cy + oy, cx + ox, cy + ry, cx, cy + ry));
    out.push_str(&curve(cx - ox, cy + ry, cx - rx, cy + oy, cx - rx, cy));
    out.push_str(&curve(cx - rx, cy - oy, cx - ox, cy - ry, cx, cy - ry));
    out.push_str(&curve(cx + ox, cy - ry, cx + rx, cy - oy, cx + rx, cy));
    out.push_str("h ");
    out
}

/// One item as a content-stream fragment. `None` for anything with no geometry to draw — an
/// empty path, or a shape tool that has no vector form.
pub fn item_pdf(item: &VectorItem) -> Option<String> {
    match item {
        VectorItem::Path(p) => {
            let (&first, rest) = p.points.split_first()?;
            let mut out = color_ops(
                (p.fill && p.closed).then_some(p.color),
                p.stroke.then_some((p.stroke_color, p.stroke_width)),
            );
            out.push_str(&format!("{} {} m ", n(first.0), n(first.1)));
            for &(x, y) in rest {
                out.push_str(&format!("{} {} l ", n(x), n(y)));
            }
            if p.closed {
                out.push_str("h ");
            }
            out.push_str(paint_op(p.fill, p.stroke, p.closed));
            Some(out)
        }
        VectorItem::Shape(s) => {
            let shape = s.shape;
            let takes_fill = shape.tool.takes_fill();
            let fills = shape.fill && takes_fill;
            let strokes = !takes_fill || shape.stroke;
            let mut out = color_ops(
                fills.then_some(s.color),
                strokes.then_some((s.stroke_color, shape.half_width * 2.0)),
            );
            out.push_str(&shape_path(&shape)?);
            out.push_str(paint_op(fills, strokes, shape.tool != Tool::Line));
            Some(out)
        }
    }
}

fn shape_path(shape: &Shape) -> Option<String> {
    let (x0, y0) = shape.start;
    let (x1, y1) = shape.end;
    let (min_x, min_y) = (x0.min(x1), y0.min(y1));
    let (w, h) = ((x1 - x0).abs(), (y1 - y0).abs());
    Some(match shape.tool {
        Tool::Rect => format!("{} {} {} {} re ", n(min_x), n(min_y), n(w), n(h)),
        Tool::Ellipse => ellipse(min_x + w * 0.5, min_y + h * 0.5, w * 0.5, h * 0.5),
        Tool::Line => format!("{} {} m {} {} l ", n(x0), n(y0), n(x1), n(y1)),
        Tool::Triangle => polygon(&shape.triangle_vertices()),
        Tool::Pentagon => polygon(&shape.pentagon_vertices()),
        Tool::Arrow => polygon(&shape.arrow_outline()),
        _ => return None,
    })
}

/// A layer transform as a PDF `cm` matrix, so a moved or scaled vector layer stays geometry
/// instead of being baked into its coordinates — the same reason `vector_svg` emits a `<g
/// transform=...>`. The order matches that function exactly: translate to the pivot, rotate,
/// scale, translate back, then the layer offset.
pub fn pdf_transform_matrix(
    item: &VectorItem,
    transform: Option<LayerTransform>,
) -> Option<String> {
    let t = transform.filter(|t| !t.is_identity())?;
    let (px, py) = bounds_center(item.bounds()?);
    let (sin, cos) = t.rotation.sin_cos();
    let (a, b) = (cos * t.scale_x, sin * t.scale_x);
    let (c, d) = (-sin * t.scale_y, cos * t.scale_y);
    let e = t.offset_x + px - (a * px + c * py);
    let f = t.offset_y + py - (b * px + d * py);
    Some(format!(
        "{} {} {} {} {} {} cm ",
        n(a),
        n(b),
        n(c),
        n(d),
        n(e),
        n(f)
    ))
}
