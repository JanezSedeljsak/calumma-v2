//! A vector item as SVG markup. Kept apart from `vector` because emitting a file is a
//! different job from being an item: the model side answers where an item is and how it is
//! edited, this side answers what it looks like written down.
use crate::shape::Tool;
use crate::transform::{bounds_center, LayerTransform};
use crate::vector::{items_bounds, VectorItem};

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
    let pivot = bounds_center(items_bounds(items)?);
    let degrees = t.rotation.to_degrees();
    Some(format!(
        "<g transform=\"translate({} {}) translate({} {}) rotate({}) scale({} {}) translate({} {})\">",
        t.offset_x, t.offset_y, pivot.0, pivot.1, degrees, t.scale_x, t.scale_y, -pivot.0, -pivot.1
    ))
}
