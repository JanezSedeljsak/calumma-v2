use calumma_core::shape::*;

#[test]
fn line_coverage_on_path() {
    let s = Shape {
        tool: Tool::Line,
        start: (0.0, 0.0),
        end: (100.0, 0.0),
        half_width: 2.0,
        fill: false,
        stroke: true,
    };
    assert!(s.coverage(50.0, 0.0) > 0.9);
    assert!(s.coverage(50.0, 20.0) < 0.1);
}

#[test]
fn rect_bounds_include_pad() {
    let s = Shape {
        tool: Tool::Rect,
        start: (10.0, 10.0),
        end: (40.0, 40.0),
        half_width: 2.0,
        fill: false,
        stroke: true,
    };
    let (x0, y0, x1, y1) = s.bounds();
    assert!(x0 < 10.0 && y0 < 10.0 && x1 > 40.0 && y1 > 40.0);
}

#[test]
fn triangle_fill_covers_center() {
    let s = Shape {
        tool: Tool::Triangle,
        start: (0.0, 0.0),
        end: (100.0, 100.0),
        half_width: 1.0,
        fill: true,
        stroke: false,
    };
    assert!(s.coverage(50.0, 70.0) > 0.9);
    assert!(s.coverage(10.0, 10.0) < 0.1);
}

#[test]
fn pentagon_is_a_shape_that_takes_fill() {
    assert!(Tool::Pentagon.is_shape());
    assert!(Tool::Pentagon.takes_fill());
    assert_eq!(Tool::from_u32(13), Some(Tool::Pentagon));
    assert_eq!(Tool::from_u32(12), Some(Tool::Triangle));
}

fn shape(tool: Tool, fill: bool) -> Shape {
    Shape {
        tool,
        start: (20.0, 20.0),
        end: (80.0, 60.0),
        half_width: 2.0,
        fill,
        stroke: !fill,
    }
}

/// An outlined shape is hollow: the rim has ink, the middle does not. Filled, the middle is
/// solid. Every closed shape has to answer this the same way, since one `fill` flag drives
/// all four.
#[test]
fn every_closed_shape_is_hollow_when_it_is_not_filled() {
    for tool in [Tool::Rect, Tool::Ellipse, Tool::Triangle, Tool::Pentagon] {
        let outlined = shape(tool, false);
        let filled = shape(tool, true);
        let middle = (50.0, 45.0);

        assert!(
            filled.coverage(middle.0, middle.1) > 0.9,
            "{tool:?} filled covers its middle"
        );
        assert!(
            outlined.coverage(middle.0, middle.1) < 0.1,
            "{tool:?} outlined leaves its middle empty"
        );
        assert!(
            outlined.coverage(20.0, 40.0) > 0.0 || outlined.coverage(50.0, 20.0) > 0.0,
            "{tool:?} outlined still has ink on its rim"
        );
        assert!(
            outlined.coverage(-40.0, -40.0) < 0.1,
            "{tool:?} outlined is empty far outside"
        );
    }
}

/// The tools that draw nothing have no distance field at all, so a stray preview built from
/// one cannot paint a shape nobody asked for.
#[test]
fn a_tool_with_no_geometry_is_infinitely_far_from_every_point() {
    for tool in [
        Tool::Pen,
        Tool::Eraser,
        Tool::SelectRect,
        Tool::SelectEllipse,
        Tool::SelectLasso,
        Tool::Fill,
        Tool::Transform,
        Tool::Eyedropper,
        Tool::Text,
        Tool::Move,
        Tool::Blur,
        Tool::MagicWand,
    ] {
        let s = shape(tool, true);
        assert_eq!(s.distance(50.0, 40.0), f32::MAX, "{tool:?}");
        assert_eq!(s.coverage(50.0, 40.0), 0.0, "{tool:?}");
    }
}

/// A click that never moved is a zero-length drag. The arrow has no direction to point its
/// barbs along, so it falls back to the shaft alone rather than dividing by the span.
#[test]
fn a_zero_length_arrow_degrades_to_its_shaft() {
    let s = Shape {
        tool: Tool::Arrow,
        start: (30.0, 30.0),
        end: (30.0, 30.0),
        half_width: 3.0,
        fill: false,
        stroke: true,
    };
    assert!(s.distance(30.0, 30.0).is_finite());
    assert!(s.coverage(30.0, 30.0) > 0.9, "the stamp under the pointer");
    assert!(s.coverage(80.0, 30.0) < 0.1);
    assert_eq!(
        s.arrow_outline(),
        vec![(30.0, 30.0), (30.0, 30.0)],
        "no barbs without a direction to hang them off"
    );
}

/// The head is clamped to the shaft, so a very short arrow is all head rather than barbs that
/// overshoot backwards past its own tail.
#[test]
fn a_short_arrow_keeps_its_head_inside_its_own_length() {
    let s = Shape {
        tool: Tool::Arrow,
        start: (0.0, 0.0),
        end: (6.0, 0.0),
        half_width: 4.0,
        fill: false,
        stroke: true,
    };
    let verts = s.arrow_outline();
    assert_eq!(verts.len(), 5);
    for (x, _) in &verts {
        assert!(
            (-0.01..=6.01).contains(x),
            "a barb reached back past the tail: {verts:?}"
        );
    }
}

#[test]
fn a_polygon_with_no_vertices_is_infinitely_far_away() {
    assert_eq!(sd_polygon((0.0, 0.0), &[]), f32::MAX);
}

#[test]
fn a_degenerate_segment_measures_the_distance_to_its_point() {
    let d = sd_segment((3.0, 4.0), (0.0, 0.0), (0.0, 0.0));
    assert!((d - 5.0).abs() < 1e-3, "{d}");
}

/// The tool taxonomy is product rule, not derived geometry — the tools island reads every one
/// of these to decide which knobs to show, so each needs a true *and* a false case pinned.
#[test]
fn the_tool_taxonomy_says_which_knobs_a_tool_carries() {
    assert!(Tool::Pen.takes_brush());
    assert!(!Tool::Eraser.takes_brush(), "the eraser takes ink away");

    assert!(Tool::Eraser.takes_eraser_hardness());
    assert!(
        !Tool::Pen.takes_eraser_hardness(),
        "the pen's rides its brush"
    );

    assert!(Tool::Fill.takes_tolerance() && Tool::MagicWand.takes_tolerance());
    assert!(!Tool::Pen.takes_tolerance());

    assert!(Tool::Blur.takes_blur_strength());
    assert!(!Tool::Pen.takes_blur_strength());

    assert!(Tool::Eyedropper.takes_eyedropper_radius());
    assert!(!Tool::Pen.takes_eyedropper_radius());

    assert!(Tool::Pen.previews_stroke() && Tool::SelectLasso.previews_stroke());
    assert!(
        !Tool::Blur.previews_stroke(),
        "blur commits as it goes, so there is nothing to preview"
    );

    assert!(Tool::Blur.is_stroke(), "blur is dragged freehand");
    assert!(!Tool::Rect.is_stroke());
}

/// Blur reads pixels rather than laying color down, so it takes neither ink opacity nor a
/// color — but it does take a size, like every other dragged tool.
#[test]
fn a_tool_that_reads_pixels_takes_a_size_but_no_ink() {
    assert!(Tool::Blur.takes_brush_size());
    assert!(!Tool::Blur.takes_ink_opacity());
    assert!(Tool::Pen.takes_ink_opacity() && Tool::Fill.takes_ink_opacity());
    assert!(
        !Tool::Fill.takes_brush_size(),
        "the bucket has no brush to size"
    );
    assert!(!Tool::Eraser.takes_ink_opacity());
}

#[test]
fn only_the_shapes_and_the_pen_can_become_vector_items() {
    for tool in [
        Tool::Rect,
        Tool::Ellipse,
        Tool::Line,
        Tool::Arrow,
        Tool::Pen,
    ] {
        assert!(tool.shows_vector_mode(), "{tool:?}");
    }
    for tool in [Tool::Eraser, Tool::Fill, Tool::Text, Tool::Move, Tool::Blur] {
        assert!(!tool.shows_vector_mode(), "{tool:?}");
    }
}

#[test]
fn an_unknown_wire_value_is_not_a_tool() {
    assert_eq!(Tool::from_u32(999), None);
    assert_eq!(Tool::from_u32(18), None);
    assert_eq!(Tool::from_u32(0), Some(Tool::Pen));
}

/// Shift squares a Rect or an Ellipse and nothing else — Line and Arrow want an angle snap
/// and the polygons a regular-polygon lock, which are different clamps that were not built.
#[test]
fn only_rect_and_ellipse_constrain_to_a_square() {
    assert!(Tool::Rect.constrains_to_square() && Tool::Ellipse.constrains_to_square());
    for tool in [Tool::Line, Tool::Arrow, Tool::Triangle, Tool::Pentagon] {
        assert!(!tool.constrains_to_square(), "{tool:?}");
    }
}

/// The longer side wins so the square *fills* the drag, and each delta keeps its sign so a
/// drag up and to the left still draws up and to the left.
#[test]
fn a_constrained_drag_fills_the_longer_side_in_every_direction() {
    assert_eq!(square_end((10.0, 10.0), (40.0, 20.0)), (40.0, 40.0));
    assert_eq!(square_end((10.0, 10.0), (20.0, 40.0)), (40.0, 40.0));
    assert_eq!(square_end((10.0, 10.0), (-20.0, 0.0)), (-20.0, -20.0));
    assert_eq!(square_end((10.0, 10.0), (0.0, -20.0)), (-20.0, -20.0));
    assert_eq!(
        square_end((10.0, 10.0), (10.0, 10.0)),
        (10.0, 10.0),
        "a drag that never moved stays where it is"
    );
}

fn bordered_rect() -> Shape {
    Shape {
        tool: Tool::Rect,
        start: (20.0, 20.0),
        end: (80.0, 60.0),
        half_width: 2.0,
        fill: true,
        stroke: true,
    }
}

#[test]
fn a_shape_can_carry_a_fill_and_a_stroke_at_once() {
    let s = bordered_rect();
    let inside = (50.0, 40.0);
    let on_edge = (20.0, 40.0);

    assert!(s.fill_distance(inside.0, inside.1).unwrap() < 0.0);
    assert!(s.stroke_distance(on_edge.0, on_edge.1).unwrap() < 0.0);
    assert!(
        s.fill_distance(on_edge.0, on_edge.1).unwrap().abs() < 0.001,
        "the fill reaches the edge the stroke straddles"
    );
}

#[test]
fn a_fill_only_shape_has_no_stroke_part_and_a_stroke_only_shape_has_no_fill() {
    let filled = Shape {
        stroke: false,
        ..bordered_rect()
    };
    assert!(filled.stroke_distance(20.0, 40.0).is_none());
    assert!(filled.fill_distance(50.0, 40.0).is_some());

    let outlined = Shape {
        fill: false,
        ..bordered_rect()
    };
    assert!(outlined.fill_distance(50.0, 40.0).is_none());
    assert!(outlined.stroke_distance(20.0, 40.0).is_some());
    assert!(
        outlined.coverage(50.0, 40.0) < 0.1,
        "the middle stays empty"
    );
}

#[test]
fn a_line_is_always_stroked_and_never_filled() {
    let line = Shape {
        tool: Tool::Line,
        start: (0.0, 0.0),
        end: (100.0, 0.0),
        half_width: 2.0,
        fill: true,
        stroke: false,
    };
    assert!(line.fill_distance(50.0, 0.0).is_none());
    assert!(line.stroke_distance(50.0, 0.0).unwrap() < 0.0);
}

#[test]
fn a_shape_with_neither_part_draws_nothing() {
    let blank = Shape {
        fill: false,
        stroke: false,
        ..bordered_rect()
    };
    assert_eq!(blank.coverage(50.0, 40.0), 0.0);
    assert_eq!(blank.coverage(20.0, 40.0), 0.0);
}

#[test]
fn a_fill_with_no_stroke_pads_only_the_antialiased_pixel() {
    let filled = Shape {
        stroke: false,
        ..bordered_rect()
    };
    assert_eq!(filled.padding(), 1.0);
    assert_eq!(bordered_rect().padding(), 3.0);
}

#[test]
fn ink_sample_scales_alpha_by_coverage_and_drops_what_it_cannot_see() {
    assert_eq!(
        ink_sample(Some(-5.0), [10, 20, 30, 200]),
        Some([10, 20, 30, 200])
    );
    assert_eq!(ink_sample(Some(5.0), [10, 20, 30, 200]), None);
    assert_eq!(ink_sample(None, [10, 20, 30, 200]), None);
}
