//! Direct tests for the free functions and `VectorItem` methods in `calumma_core::vector`,
//! as opposed to `vector_items.rs`, which drives the same module through `Document`.

use calumma_core::vector::*;
use calumma_core::{LayerTransform, Shape, Tool};

fn rect_shape(start: (f32, f32), end: (f32, f32), fill: bool) -> VectorShape {
    VectorShape {
        shape: Shape {
            tool: Tool::Rect,
            start,
            end,
            half_width: 1.0,
            fill,
        },
        color: [10, 20, 30, 255],
    }
}

fn open_path(points: Vec<(f32, f32)>) -> VectorPath {
    VectorPath {
        points,
        closed: false,
        fill: false,
        color: [1, 2, 3, 255],
        stroke_width: 4.0,
    }
}

#[test]
fn color_reads_through_to_the_item() {
    let path = VectorItem::Path(open_path(vec![(0.0, 0.0)]));
    let shape = VectorItem::Shape(rect_shape((0.0, 0.0), (10.0, 10.0), true));
    assert_eq!(path.color(), [1, 2, 3, 255]);
    assert_eq!(shape.color(), [10, 20, 30, 255]);
}

#[test]
fn path_bounds_are_none_without_points() {
    let path = VectorItem::Path(open_path(vec![]));
    assert_eq!(path.bounds(), None);
}

#[test]
fn open_path_bounds_pad_by_half_the_stroke_plus_a_pixel() {
    let path = VectorItem::Path(open_path(vec![(10.0, 10.0), (20.0, 20.0)]));
    let (x0, y0, x1, y1) = path.bounds().unwrap();
    // stroke_width 4.0 -> pad = 2.0 + 1.0
    assert!((x0 - 7.0).abs() < 1e-4);
    assert!((y0 - 7.0).abs() < 1e-4);
    assert!((x1 - 23.0).abs() < 1e-4);
    assert!((y1 - 23.0).abs() < 1e-4);
}

#[test]
fn closed_filled_path_bounds_have_no_pad() {
    let mut p = open_path(vec![(10.0, 10.0), (20.0, 10.0), (20.0, 20.0)]);
    p.closed = true;
    p.fill = true;
    let (x0, y0, x1, y1) = VectorItem::Path(p).bounds().unwrap();
    assert_eq!((x0, y0, x1, y1), (10.0, 10.0, 20.0, 20.0));
}

#[test]
fn translate_moves_every_point_of_a_path() {
    let mut item = VectorItem::Path(open_path(vec![(0.0, 0.0), (10.0, 0.0)]));
    item.translate(5.0, -3.0);
    let VectorItem::Path(p) = item else {
        unreachable!()
    };
    assert_eq!(p.points, vec![(5.0, -3.0), (15.0, -3.0)]);
}

#[test]
fn set_translated_path_to_path_reuses_the_point_buffer_and_copies_style() {
    let mut dst = VectorItem::Path(open_path(vec![(99.0, 99.0)]));
    let mut src = open_path(vec![(0.0, 0.0), (10.0, 0.0)]);
    src.closed = true;
    src.color = [9, 8, 7, 6];
    src.stroke_width = 2.5;
    dst.set_translated(&VectorItem::Path(src), 1.0, 1.0);
    let VectorItem::Path(p) = dst else {
        unreachable!()
    };
    assert_eq!(p.points, vec![(1.0, 1.0), (11.0, 1.0)]);
    assert!(p.closed);
    assert_eq!(p.color, [9, 8, 7, 6]);
    assert_eq!(p.stroke_width, 2.5);
}

#[test]
fn set_translated_falls_back_to_clone_and_translate_for_shapes() {
    let mut dst = VectorItem::Shape(rect_shape((0.0, 0.0), (1.0, 1.0), true));
    let src = VectorItem::Shape(rect_shape((10.0, 10.0), (20.0, 20.0), true));
    dst.set_translated(&src, 5.0, 5.0);
    let VectorItem::Shape(s) = dst else {
        unreachable!()
    };
    assert_eq!(s.shape.start, (15.0, 15.0));
    assert_eq!(s.shape.end, (25.0, 25.0));
}

#[test]
fn path_distance_with_no_points_is_effectively_unreachable() {
    let item = VectorItem::Path(open_path(vec![]));
    assert_eq!(item.distance(0.0, 0.0), f32::MAX);
}

#[test]
fn path_distance_with_one_point_is_a_dot() {
    let mut p = open_path(vec![(10.0, 10.0)]);
    p.stroke_width = 6.0;
    let item = VectorItem::Path(p);
    assert!((item.distance(10.0, 10.0) - (-3.0)).abs() < 1e-4);
}

#[test]
fn closed_filled_path_distance_is_negative_inside() {
    let mut p = open_path(vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)]);
    p.closed = true;
    p.fill = true;
    let item = VectorItem::Path(p);
    assert!(item.distance(10.0, 10.0) < 0.0);
}

#[test]
fn closed_unfilled_path_closes_the_last_segment_back_to_the_first() {
    let mut p = open_path(vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0)]);
    p.closed = true;
    p.stroke_width = 2.0;
    let item = VectorItem::Path(p);
    // The midpoint of the closing edge (20,20)-(0,0) is (10,10), only reachable
    // through the segment the `closed` branch adds.
    assert!(item.distance(10.0, 10.0) < 1.0);
}

#[test]
fn items_bounds_is_none_for_no_items() {
    assert_eq!(items_bounds(&[]), None);
}

#[test]
fn items_bounds_skips_items_that_have_no_bounds_of_their_own() {
    let boundless = VectorItem::Path(open_path(vec![]));
    let real = VectorItem::Shape(rect_shape((0.0, 0.0), (10.0, 10.0), true));
    let (x0, y0, x1, y1) = items_bounds(&[boundless, real]).unwrap();
    assert!(x0 <= 0.0 && y0 <= 0.0 && x1 >= 10.0 && y1 >= 10.0);
}

#[test]
fn transformed_bounds_is_none_without_items() {
    assert_eq!(transformed_bounds(&[], None), None);
}

#[test]
fn transformed_bounds_matches_the_untransformed_bounds_with_no_transform() {
    let items = [VectorItem::Shape(rect_shape(
        (0.0, 0.0),
        (10.0, 10.0),
        true,
    ))];
    assert_eq!(transformed_bounds(&items, None), items_bounds(&items),);
}

#[test]
fn draws_on_gpu_is_always_true_for_shapes() {
    let item = VectorItem::Shape(rect_shape((0.0, 0.0), (10.0, 10.0), true));
    assert!(draws_on_gpu(&item));
}

#[test]
fn draws_on_gpu_is_false_only_for_a_closed_filled_path() {
    let mut open = open_path(vec![(0.0, 0.0), (10.0, 10.0)]);
    assert!(draws_on_gpu(&VectorItem::Path(open.clone())));
    open.closed = true;
    assert!(
        draws_on_gpu(&VectorItem::Path(open.clone())),
        "closed but unfilled still stroke-draws"
    );
    open.fill = true;
    assert!(!draws_on_gpu(&VectorItem::Path(open)));
}

#[test]
fn rasterize_into_rgba_does_nothing_without_items() {
    let mut buf = vec![0u8; 20 * 20 * 4];
    rasterize_into_rgba(&[], None, &mut buf, 20, 20);
    assert!(buf.iter().all(|&b| b == 0));
}

#[test]
fn rasterize_into_rgba_paints_the_shapes_footprint() {
    let items = [VectorItem::Shape(rect_shape(
        (5.0, 5.0),
        (15.0, 15.0),
        true,
    ))];
    let mut buf = vec![0u8; 20 * 20 * 4];
    rasterize_into_rgba(&items, None, &mut buf, 20, 20);
    let center = ((10 * 20 + 10) * 4) as usize;
    assert_eq!(&buf[center..center + 4], &[10, 20, 30, 255]);
    let outside = ((20 + 1) * 4) as usize;
    assert_eq!(&buf[outside..outside + 4], &[0, 0, 0, 0]);
}

#[test]
fn rasterize_into_rgba_applies_the_layer_transform() {
    let items = [VectorItem::Shape(rect_shape(
        (5.0, 5.0),
        (15.0, 15.0),
        true,
    ))];
    let transform = LayerTransform {
        offset_x: 10.0,
        ..LayerTransform::default()
    };
    let mut buf = vec![0u8; 40 * 20 * 4];
    rasterize_into_rgba(&items, Some(transform), &mut buf, 40, 20);
    let shifted = ((10 * 40 + 20) * 4) as usize; // (20, 10): inside the shifted [15,25) span
    assert_eq!(&buf[shifted..shifted + 4], &[10, 20, 30, 255]);
    let original = ((10 * 40 + 10) * 4) as usize; // (10, 10): no longer covered
    assert_eq!(&buf[original..original + 4], &[0, 0, 0, 0]);
}

#[test]
fn item_from_shape_rejects_tools_with_no_vector_form() {
    let pen = Shape {
        tool: Tool::Pen,
        start: (0.0, 0.0),
        end: (1.0, 1.0),
        half_width: 1.0,
        fill: false,
    };
    assert_eq!(item_from_shape(pen, [0, 0, 0, 255]), None);
}

#[test]
fn item_from_shape_wraps_a_shape_tool() {
    let rect = Shape {
        tool: Tool::Rect,
        start: (0.0, 0.0),
        end: (1.0, 1.0),
        half_width: 1.0,
        fill: true,
    };
    let item = item_from_shape(rect, [1, 2, 3, 4]).unwrap();
    assert_eq!(item.color(), [1, 2, 3, 4]);
}

#[test]
fn item_from_points_rejects_an_empty_stroke() {
    assert_eq!(item_from_points(&[], [0, 0, 0, 255], 1.0), None);
}

#[test]
fn item_from_points_builds_an_open_unfilled_path() {
    let points = [(0.0, 0.0), (10.0, 10.0)];
    let item = item_from_points(&points, [1, 2, 3, 255], 3.0).unwrap();
    let VectorItem::Path(p) = item else {
        panic!("expected a path")
    };
    assert_eq!(p.points, points.to_vec());
    assert!(!p.closed);
    assert!(!p.fill);
    assert_eq!(p.stroke_width, 3.0);
}

#[test]
fn item_svg_open_path_has_no_closing_z() {
    let item = VectorItem::Path(open_path(vec![(0.0, 0.0), (10.0, 0.0)]));
    let svg = item_svg(&item).unwrap();
    assert!(svg.starts_with("<path d=\"M 0 0 L 10 0\""));
    assert!(!svg.contains(" Z"));
}

#[test]
fn item_svg_closed_path_closes_with_z() {
    let mut p = open_path(vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]);
    p.closed = true;
    let svg = item_svg(&VectorItem::Path(p)).unwrap();
    assert!(svg.contains(" Z\""));
}

#[test]
fn item_svg_is_none_without_points() {
    assert_eq!(item_svg(&VectorItem::Path(open_path(vec![]))), None);
}

#[test]
fn item_svg_covers_every_shape_tool() {
    let cases: &[(Tool, &str)] = &[
        (Tool::Rect, "<rect "),
        (Tool::Ellipse, "<ellipse "),
        (Tool::Line, "<line "),
        (Tool::Triangle, "<polygon "),
        (Tool::Pentagon, "<polygon "),
        (Tool::Arrow, "<polygon "),
    ];
    for &(tool, tag) in cases {
        let shape = VectorShape {
            shape: Shape {
                tool,
                start: (0.0, 0.0),
                end: (10.0, 10.0),
                half_width: 1.0,
                fill: true,
            },
            color: [0, 0, 0, 255],
        };
        let svg = item_svg(&VectorItem::Shape(shape)).unwrap();
        assert!(svg.starts_with(tag), "{tool:?} -> {svg}");
    }
}

#[test]
fn item_svg_arrow_is_always_an_outline_even_when_the_tool_is_marked_filled() {
    let shape = VectorShape {
        shape: Shape {
            tool: Tool::Arrow,
            start: (0.0, 0.0),
            end: (10.0, 10.0),
            half_width: 1.0,
            fill: true,
        },
        color: [0, 0, 0, 255],
    };
    let svg = item_svg(&VectorItem::Shape(shape)).unwrap();
    assert!(svg.contains("fill=\"none\""));
}

#[test]
fn item_svg_is_none_for_a_tool_with_no_svg_primitive() {
    let shape = VectorShape {
        shape: Shape {
            tool: Tool::Pen,
            start: (0.0, 0.0),
            end: (10.0, 10.0),
            half_width: 1.0,
            fill: false,
        },
        color: [0, 0, 0, 255],
    };
    assert_eq!(item_svg(&VectorItem::Shape(shape)), None);
}

#[test]
fn svg_transform_attr_is_none_without_a_transform() {
    let items = [VectorItem::Shape(rect_shape(
        (0.0, 0.0),
        (10.0, 10.0),
        true,
    ))];
    assert_eq!(svg_transform_attr(&items, None), None);
}

#[test]
fn svg_transform_attr_is_none_for_an_identity_transform() {
    let items = [VectorItem::Shape(rect_shape(
        (0.0, 0.0),
        (10.0, 10.0),
        true,
    ))];
    assert_eq!(
        svg_transform_attr(&items, Some(LayerTransform::default())),
        None
    );
}

#[test]
fn svg_transform_attr_is_none_without_items_to_pivot_around() {
    let transform = LayerTransform {
        offset_x: 5.0,
        ..LayerTransform::default()
    };
    assert_eq!(svg_transform_attr(&[], Some(transform)), None);
}

#[test]
fn svg_transform_attr_emits_a_group_carrying_offset_and_rotation() {
    let items = [VectorItem::Shape(rect_shape(
        (0.0, 0.0),
        (10.0, 10.0),
        true,
    ))];
    let transform = LayerTransform {
        offset_x: 5.0,
        offset_y: 0.0,
        rotation: std::f32::consts::FRAC_PI_2,
        ..LayerTransform::default()
    };
    let attr = svg_transform_attr(&items, Some(transform)).unwrap();
    assert!(attr.starts_with("<g transform=\"translate(5 0)"));
    assert!(attr.contains("rotate(90)"));
}

#[test]
fn tool_makes_vector_covers_the_pen_and_every_shape_but_nothing_else() {
    for tool in [
        Tool::Pen,
        Tool::Line,
        Tool::Rect,
        Tool::Ellipse,
        Tool::Arrow,
        Tool::Triangle,
        Tool::Pentagon,
    ] {
        assert!(tool_makes_vector(tool), "{tool:?} should make vector ink");
    }
    for tool in [
        Tool::Eraser,
        Tool::SelectRect,
        Tool::SelectEllipse,
        Tool::SelectLasso,
        Tool::Fill,
        Tool::Transform,
        Tool::Eyedropper,
        Tool::Text,
        Tool::Move,
    ] {
        assert!(
            !tool_makes_vector(tool),
            "{tool:?} should not make vector ink"
        );
    }
}
