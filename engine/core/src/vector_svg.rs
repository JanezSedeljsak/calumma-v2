//! A vector item as SVG markup. Kept apart from `vector` because emitting a file is a
//! different job from being an item: the model side answers where an item is and how it is
//! edited, this side answers what it looks like written down.
use crate::shape::{Shape, Tool};
use crate::transform::{bounds_center, LayerTransform};
use crate::vector::VectorItem;

/// Fill and stroke as the two independent SVG attributes they natively are, which is a
/// closer match to the format than the either/or this used to emit. A part that is switched
/// off is `fill="none"` / no stroke attributes at all, so the file never carries a color
/// for something the board does not draw.
fn svg_paint(fill: Option<[u8; 4]>, stroke: Option<([u8; 4], f32)>) -> String {
    let mut out = match fill {
        Some(color) => {
            let (r, g, b) = (color[0], color[1], color[2]);
            let alpha = color[3] as f32 / 255.0;
            format!("fill=\"rgb({r},{g},{b})\" fill-opacity=\"{alpha}\"")
        }
        None => "fill=\"none\"".to_string(),
    };
    if let Some((color, width)) = stroke {
        let (r, g, b) = (color[0], color[1], color[2]);
        let alpha = color[3] as f32 / 255.0;
        out.push_str(&format!(
            " stroke=\"rgb({r},{g},{b})\" stroke-opacity=\"{alpha}\" stroke-width=\"{width}\""
        ));
    }
    out
}

fn shape_paint(shape: &Shape, fill: [u8; 4], stroke: [u8; 4]) -> String {
    svg_paint(
        (shape.fill && shape.tool.takes_fill()).then_some(fill),
        (!shape.tool.takes_fill() || shape.stroke).then_some((stroke, shape.half_width * 2.0)),
    )
}

fn polygon_svg(verts: &[(f32, f32)], paint: &str) -> String {
    let points = verts
        .iter()
        .map(|(x, y)| format!("{x},{y}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!("<polygon points=\"{points}\" {paint} />")
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
                svg_paint(
                    (p.fill && p.closed).then_some(p.color),
                    p.stroke.then_some((p.stroke_color, p.stroke_width)),
                )
            ))
        }
        VectorItem::Shape(s) => {
            let shape = s.shape;
            let (x0, y0) = shape.start;
            let (x1, y1) = shape.end;
            let (min_x, min_y) = (x0.min(x1), y0.min(y1));
            let (w, h) = ((x1 - x0).abs(), (y1 - y0).abs());
            let paint = shape_paint(&shape, s.color, s.stroke_color);
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
                Tool::Triangle => polygon_svg(&shape.triangle_vertices(), &paint),
                Tool::Pentagon => polygon_svg(&shape.pentagon_vertices(), &paint),
                Tool::Arrow => polygon_svg(&shape.arrow_outline(), &paint),
                _ => return None,
            })
        }
    }
}

/// A layer transform exported as an SVG `<g transform=...>` rather than baked into every
/// coordinate — an SVG group carries translate/rotate/scale natively, so the exported file
/// stays as editable as the layer is.
pub fn svg_transform_attr(item: &VectorItem, transform: Option<LayerTransform>) -> Option<String> {
    let t = transform.filter(|t| !t.is_identity())?;
    let pivot = bounds_center(item.bounds()?);
    let degrees = t.rotation.to_degrees();
    Some(format!(
        "<g transform=\"translate({} {}) translate({} {}) rotate({}) scale({} {}) translate({} {})\">",
        t.offset_x, t.offset_y, pivot.0, pivot.1, degrees, t.scale_x, t.scale_y, -pivot.0, -pivot.1
    ))
}
