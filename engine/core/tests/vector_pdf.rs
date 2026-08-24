//! The PDF twin of the `vector_svg` tests in `vector.rs`: same geometry, same fill/stroke
//! split, a different serializer. A rect exported to PDF has to be a real PDF path, so these
//! read the operators back rather than trusting that something was emitted.

use calumma_core::transform::bounds_center;
use calumma_core::vector::*;
use calumma_core::vector_pdf::*;
use calumma_core::{LayerTransform, Shape, Tool};

fn shape_item(tool: Tool, fill: bool, stroke: bool) -> VectorItem {
    VectorItem::Shape(VectorShape {
        shape: Shape {
            tool,
            start: (10.0, 20.0),
            end: (30.0, 60.0),
            half_width: 1.5,
            fill,
            stroke,
        },
        color: [255, 0, 0, 255],
        stroke_color: [0, 0, 255, 255],
    })
}

fn path_item(points: Vec<(f32, f32)>, closed: bool, fill: bool, stroke: bool) -> VectorItem {
    VectorItem::Path(VectorPath {
        points,
        closed,
        fill,
        color: [0, 255, 0, 255],
        stroke,
        stroke_color: [0, 0, 0, 255],
        stroke_width: 3.0,
    })
}

/// The paint operator is the last token of the fragment, and it is the whole fill/stroke
/// decision in one letter — `B` both, `f` fill, `S` stroke, `n` neither.
fn paint_op(pdf: &str) -> &str {
    pdf.split_whitespace().last().expect("an operator")
}

#[test]
fn an_item_with_no_geometry_emits_nothing() {
    assert_eq!(item_pdf(&path_item(vec![], false, false, true)), None);
    assert_eq!(item_pdf(&shape_item(Tool::Pen, true, true)), None);
    assert_eq!(item_pdf(&shape_item(Tool::Eraser, false, true)), None);
}

#[test]
fn an_open_path_is_a_polyline_that_is_never_closed_or_filled() {
    let item = path_item(vec![(1.0, 2.0), (3.0, 4.0), (5.0, 6.0)], false, true, true);
    let pdf = item_pdf(&item).expect("path");

    assert!(pdf.contains("1 2 m "), "{pdf}");
    assert!(pdf.contains("3 4 l 5 6 l "), "{pdf}");
    assert!(!pdf.contains(" h "), "an open path is never closed: {pdf}");
    assert_eq!(paint_op(&pdf), "S", "fill on an open path is ignored");
}

#[test]
fn a_closed_filled_path_closes_and_paints_both() {
    let item = path_item(vec![(0.0, 0.0), (4.0, 0.0), (4.0, 4.0)], true, true, true);
    let pdf = item_pdf(&item).expect("path");

    assert!(pdf.contains("h "), "{pdf}");
    assert_eq!(paint_op(&pdf), "B");
}

#[test]
fn a_closed_path_with_no_stroke_is_filled_only() {
    let item = path_item(vec![(0.0, 0.0), (4.0, 0.0), (4.0, 4.0)], true, true, false);
    assert_eq!(paint_op(&item_pdf(&item).expect("path")), "f");
}

/// `n` — paint nothing — still emits the path. A subpath that neither fills nor strokes is
/// legal PDF and keeps the operator count matching the geometry rather than silently dropping
/// an item the document still contains.
#[test]
fn a_path_that_neither_fills_nor_strokes_still_emits_its_geometry() {
    let item = path_item(vec![(0.0, 0.0), (4.0, 4.0)], false, false, false);
    let pdf = item_pdf(&item).expect("path");

    assert!(pdf.contains("0 0 m "), "{pdf}");
    assert_eq!(paint_op(&pdf), "n");
}

#[test]
fn colors_are_emitted_as_unit_floats_with_stroke_width_in_points() {
    let item = path_item(vec![(0.0, 0.0), (1.0, 1.0)], true, true, true);
    let pdf = item_pdf(&item).expect("path");

    assert!(pdf.starts_with("0 1 0 rg "), "green fill first: {pdf}");
    assert!(
        pdf.contains("0 0 0 RG 3 w "),
        "black stroke, width 3: {pdf}"
    );
}

#[test]
fn a_rect_is_a_real_pdf_rectangle_in_document_coordinates() {
    let pdf = item_pdf(&shape_item(Tool::Rect, true, false)).expect("rect");

    assert!(pdf.contains("10 20 20 40 re "), "{pdf}");
    assert_eq!(paint_op(&pdf), "f");
}

/// PDF has no ellipse primitive, so the tool survives as four cubic quarter-arcs rather than
/// being flattened to a polygon.
#[test]
fn an_ellipse_is_four_bezier_quarter_arcs_and_not_a_polygon() {
    let pdf = item_pdf(&shape_item(Tool::Ellipse, true, false)).expect("ellipse");

    assert_eq!(pdf.matches(" c ").count(), 4, "{pdf}");
    assert!(!pdf.contains(" l "), "no straight segments: {pdf}");
    assert!(
        pdf.contains("30 40 m "),
        "starts at the rightmost point: {pdf}"
    );
    assert!(pdf.contains("h "), "{pdf}");
}

/// A line has no inside, so it is stroked whatever the shape's fill flag says — the same rule
/// `takes_fill` states in `shape.rs`.
#[test]
fn a_line_is_stroked_and_left_open_even_when_marked_filled() {
    let pdf = item_pdf(&shape_item(Tool::Line, true, false)).expect("line");

    assert!(pdf.contains("10 20 m 30 60 l "), "{pdf}");
    assert!(!pdf.contains("h "), "{pdf}");
    assert_eq!(paint_op(&pdf), "S");
    assert!(
        pdf.contains("0 0 1 RG 3 w "),
        "stroke width is 2x half_width: {pdf}"
    );
}

#[test]
fn every_polygon_tool_emits_a_closed_subpath() {
    for tool in [Tool::Triangle, Tool::Pentagon, Tool::Arrow] {
        let pdf = item_pdf(&shape_item(tool, true, true)).expect("polygon");
        assert!(pdf.contains(" m "), "{tool:?}: {pdf}");
        assert!(pdf.contains(" l "), "{tool:?}: {pdf}");
        assert!(pdf.contains("h "), "{tool:?} must close: {pdf}");
    }
}

#[test]
fn a_pentagon_draws_one_move_and_four_lines() {
    let pdf = item_pdf(&shape_item(Tool::Pentagon, true, false)).expect("pentagon");

    assert_eq!(pdf.matches(" m ").count(), 1, "{pdf}");
    assert_eq!(pdf.matches(" l ").count(), 4, "{pdf}");
}

/// An arrow is always an outline — `Tool::takes_fill` excludes it — so marking the shape
/// filled changes nothing, exactly as the SVG twin already promises. It is still a closed
/// polygon; it is just never painted with `f` or `B`.
#[test]
fn an_arrow_is_stroked_as_an_outline_even_when_the_shape_is_marked_filled() {
    let pdf = item_pdf(&shape_item(Tool::Arrow, true, true)).expect("arrow");

    assert!(pdf.contains("h "), "{pdf}");
    assert_eq!(paint_op(&pdf), "S");
    assert!(!pdf.contains(" rg "), "no fill color is set at all: {pdf}");
}

/// Numbers are trimmed, not padded: four decimals of precision, then every trailing zero and
/// a bare trailing point are dropped. Every token still has to parse as a PDF number — a value
/// that rounds away must not leave a lone `-` behind.
#[test]
fn numbers_are_trimmed_to_the_shortest_form_that_still_parses() {
    let item = path_item(vec![(-0.00001, 2.5), (1.25, 3.0)], false, false, true);
    let pdf = item_pdf(&item).expect("path");

    assert!(pdf.contains("1.25 3 l "), "{pdf}");
    assert!(!pdf.contains(".0000"), "no padding: {pdf}");
    for token in pdf.split_whitespace() {
        if token.starts_with('-') || token.starts_with(|c: char| c.is_ascii_digit()) {
            assert!(
                token.parse::<f32>().is_ok(),
                "{token} is not a number: {pdf}"
            );
        }
    }
    let tokens: Vec<&str> = pdf.split_whitespace().collect();
    let m = tokens.iter().position(|&t| t == "m").expect("a moveto");
    assert_eq!(
        tokens[m - 2].parse::<f32>().expect("number"),
        0.0,
        "a coordinate that rounds away is still a zero: {pdf}"
    );
    assert_eq!(tokens[m - 1], "2.5");
}

#[test]
fn a_layer_with_no_meaningful_transform_needs_no_matrix() {
    let item = shape_item(Tool::Rect, true, false);

    assert_eq!(pdf_transform_matrix(&item, None), None);
    assert_eq!(
        pdf_transform_matrix(&item, Some(LayerTransform::default())),
        None,
        "an identity transform is not worth a cm"
    );
}

#[test]
fn an_item_with_no_bounds_has_no_pivot_to_transform_around() {
    let empty = path_item(vec![], false, false, true);
    let moved = LayerTransform {
        offset_x: 5.0,
        ..LayerTransform::default()
    };
    assert_eq!(pdf_transform_matrix(&empty, Some(moved)), None);
}

#[test]
fn a_pure_offset_is_an_identity_matrix_carrying_the_translation() {
    let item = shape_item(Tool::Rect, true, false);
    let moved = LayerTransform {
        offset_x: 12.0,
        offset_y: -7.5,
        ..LayerTransform::default()
    };

    let cm = pdf_transform_matrix(&item, Some(moved)).expect("matrix");

    let parts: Vec<f32> = cm
        .split_whitespace()
        .take(6)
        .map(|t| t.parse().expect("number"))
        .collect();
    assert_eq!(parts, [1.0, 0.0, 0.0, 1.0, 12.0, -7.5]);
    assert!(cm.ends_with("cm "), "{cm}");
}

/// Scale happens about the item's own centre, so a scaled layer stays where it is instead of
/// sliding toward the origin — the translation terms carry that correction.
#[test]
fn a_scale_is_taken_about_the_items_centre() {
    let item = shape_item(Tool::Rect, true, false);
    let (px, py) = bounds_center(item.bounds().expect("bounds"));
    let scaled = LayerTransform {
        scale_x: 2.0,
        scale_y: 2.0,
        ..LayerTransform::default()
    };

    let cm = pdf_transform_matrix(&item, Some(scaled)).expect("matrix");

    let parts: Vec<f32> = cm
        .split_whitespace()
        .take(6)
        .map(|t| t.parse().expect("number"))
        .collect();
    assert_eq!(parts[0..4], [2.0, 0.0, 0.0, 2.0]);
    assert!((parts[4] + px).abs() < 0.01, "{cm}");
    assert!((parts[5] + py).abs() < 0.01, "{cm}");
}

#[test]
fn a_rotation_fills_all_four_matrix_terms() {
    let item = shape_item(Tool::Rect, true, false);
    let turned = LayerTransform {
        rotation: std::f32::consts::FRAC_PI_2,
        ..LayerTransform::default()
    };

    let cm = pdf_transform_matrix(&item, Some(turned)).expect("matrix");

    let parts: Vec<f32> = cm
        .split_whitespace()
        .take(4)
        .map(|t| t.parse().expect("number"))
        .collect();
    assert!(parts[0].abs() < 1e-3, "cos is ~0 at a quarter turn: {cm}");
    assert!((parts[1] - 1.0).abs() < 1e-3, "{cm}");
    assert!((parts[2] + 1.0).abs() < 1e-3, "{cm}");
    assert!(parts[3].abs() < 1e-3, "{cm}");
}
