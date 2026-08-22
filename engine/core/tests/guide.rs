use calumma_core::document::*;
use calumma_core::limits::*;
use calumma_core::*;

const DOC: u32 = 400;

/// Zoom 1 with no pan, so a document pixel is a screen pixel and every threshold in
/// `GUIDE_*_PX` reads directly as a document distance.
fn doc_at_unit_zoom() -> Document {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.resize_viewport(DOC as f32, DOC as f32, 1.0);
    doc.camera.zoom = 1.0;
    doc.camera.pan_x = 0.0;
    doc.camera.pan_y = 0.0;
    doc
}

fn paint(doc: &mut Document, index: usize, rect: DocRect) {
    doc.layers[index]
        .tiles_mut()
        .unwrap()
        .paint_rect(rect, |_, _, _| Some([10, 20, 30, 255]));
}

#[test]
fn a_new_document_has_no_guides() {
    assert!(Document::new("p".into(), "t", 64, 64).guides().is_empty());
}

#[test]
fn add_guide_refuses_a_duplicate_and_returns_the_one_already_there() {
    let mut doc = doc_at_unit_zoom();
    let first = doc.add_guide(GuideAxis::Vertical, 100.0).unwrap();
    let again = doc
        .add_guide(GuideAxis::Vertical, 100.0 + GUIDE_MIN_SEPARATION * 0.5)
        .unwrap();
    assert_eq!(first, again);
    assert_eq!(doc.guides().len(), 1);
    doc.add_guide(GuideAxis::Horizontal, 100.0).unwrap();
    assert_eq!(doc.guides().len(), 2);
}

#[test]
fn add_guide_refuses_a_non_finite_position_and_respects_the_ceiling() {
    let mut doc = doc_at_unit_zoom();
    assert!(doc.add_guide(GuideAxis::Vertical, f32::NAN).is_none());
    for i in 0..GUIDES_LIMIT {
        assert!(doc.add_guide(GuideAxis::Vertical, i as f32 * 2.0).is_some());
    }
    assert!(doc.add_guide(GuideAxis::Vertical, 9999.0).is_none());
    assert_eq!(doc.guides().len(), GUIDES_LIMIT);
}

#[test]
fn clear_guides_reports_whether_it_had_anything_to_clear() {
    let mut doc = doc_at_unit_zoom();
    assert!(!doc.clear_guides());
    doc.add_guide(GuideAxis::Horizontal, 10.0);
    assert!(doc.clear_guides());
    assert!(doc.guides().is_empty());
}

#[test]
fn set_guides_truncates_to_the_ceiling() {
    let mut doc = doc_at_unit_zoom();
    let many: Vec<Guide> = (0..GUIDES_LIMIT + 20)
        .map(|i| Guide {
            axis: GuideAxis::Vertical,
            position: i as f32,
        })
        .collect();
    doc.set_guides(many);
    assert_eq!(doc.guides().len(), GUIDES_LIMIT);
}

#[test]
fn guide_at_finds_the_nearest_within_screen_slack() {
    let mut doc = doc_at_unit_zoom();
    doc.add_guide(GuideAxis::Vertical, 100.0);
    doc.add_guide(GuideAxis::Vertical, 300.0);
    assert_eq!(doc.guide_at(100.0, 50.0), Some(0));
    assert_eq!(
        doc.guide_at(100.0 + GUIDE_PICK_SLACK_PX - 0.5, 50.0),
        Some(0)
    );
    assert_eq!(doc.guide_at(100.0 + GUIDE_PICK_SLACK_PX + 1.0, 50.0), None);
    assert_eq!(doc.guide_at(299.0, 50.0), Some(1));
}

#[test]
fn pick_slack_is_screen_space_so_zooming_in_does_not_widen_it() {
    let mut doc = doc_at_unit_zoom();
    doc.camera.zoom = 8.0;
    doc.add_guide(GuideAxis::Vertical, 100.0);
    let on_guide = doc.camera.to_screen(100.0, 50.0);
    assert_eq!(doc.guide_at(on_guide.0, on_guide.1), Some(0));
    assert_eq!(
        doc.guide_at(on_guide.0 + GUIDE_PICK_SLACK_PX + 1.0, on_guide.1),
        None
    );
    // A document pixel away is well outside the slack at 8x, but would be inside it at 1x.
    let one_doc_pixel_away = doc.camera.to_screen(101.0, 50.0);
    assert_eq!(
        doc.guide_at(one_doc_pixel_away.0, one_doc_pixel_away.1),
        None
    );
}

#[test]
fn dragging_off_a_ruler_leaves_a_guide_where_it_was_released() {
    let mut doc = doc_at_unit_zoom();
    assert!(doc.begin_guide_drag_from_ruler(GuideAxis::Horizontal, 40.0, -12.0));
    assert!(doc.is_dragging_guide());
    assert_eq!(doc.dragged_guide(), Some(0));
    assert_eq!(doc.guides()[0].position, -12.0);
    doc.update_guide_drag(40.0, 150.0);
    assert_eq!(doc.guides()[0].position, 150.0);
    assert!(doc.end_guide_drag());
    assert!(!doc.is_dragging_guide());
    assert_eq!(doc.guides().len(), 1);
    assert_eq!(doc.guides()[0].position, 150.0);
}

#[test]
fn releasing_a_guide_outside_the_paper_throws_it_away() {
    let mut doc = doc_at_unit_zoom();
    doc.begin_guide_drag_from_ruler(GuideAxis::Horizontal, 40.0, -12.0);
    doc.update_guide_drag(40.0, 150.0);
    doc.update_guide_drag(40.0, -3.0);
    doc.end_guide_drag();
    assert!(doc.guides().is_empty());

    doc.begin_guide_drag_from_ruler(GuideAxis::Vertical, -8.0, 40.0);
    doc.update_guide_drag(DOC as f32 + 20.0, 40.0);
    doc.end_guide_drag();
    assert!(doc.guides().is_empty());
}

#[test]
fn a_ruler_click_that_never_reaches_the_board_leaves_nothing_behind() {
    let mut doc = doc_at_unit_zoom();
    doc.begin_guide_drag_from_ruler(GuideAxis::Horizontal, 40.0, -5.0);
    doc.end_guide_drag();
    assert!(doc.guides().is_empty());
}

#[test]
fn the_move_tool_grabs_a_guide_before_anything_under_it() {
    let mut doc = doc_at_unit_zoom();
    paint(&mut doc, 1, DocRect::new(0, 0, 399, 399));
    doc.set_tool(Tool::Move);
    doc.add_guide(GuideAxis::Horizontal, 120.0);

    doc.pointer_down(200.0, 120.0);
    assert!(doc.is_dragging_guide());
    doc.pointer_move(200.0, 260.0);
    assert_eq!(doc.guides()[0].position, 260.0);
    doc.pointer_up(200.0, 260.0);
    assert!(!doc.is_dragging_guide());
    assert_eq!(doc.guides()[0].position, 260.0);
    assert!(doc.layers[1].transform.map_or(true, |t| t.is_identity()));
}

#[test]
fn a_pen_stroke_draws_straight_through_a_guide() {
    let mut doc = doc_at_unit_zoom();
    doc.set_tool(Tool::Pen);
    doc.add_guide(GuideAxis::Horizontal, 120.0);
    doc.pointer_down(200.0, 118.0);
    assert!(!doc.is_dragging_guide());
    assert!(doc.stroke_active);
    doc.pointer_up(240.0, 118.0);
}

#[test]
fn a_shape_drag_snaps_both_ends_to_nearby_guides() {
    let mut doc = doc_at_unit_zoom();
    doc.set_tool(Tool::Rect);
    doc.add_guide(GuideAxis::Vertical, 100.0);
    doc.add_guide(GuideAxis::Horizontal, 300.0);

    doc.pointer_down(100.0 + GUIDE_SNAP_PX - 1.0, 50.0);
    doc.pointer_move(250.0, 300.0 - GUIDE_SNAP_PX + 1.0);
    let shape = doc.preview_shape().unwrap();
    assert_eq!(shape.start.0, 100.0);
    assert_eq!(shape.start.1, 50.0);
    assert_eq!(shape.end.0, 250.0);
    assert_eq!(shape.end.1, 300.0);
}

#[test]
fn a_shape_drag_outside_the_snap_threshold_is_left_alone() {
    let mut doc = doc_at_unit_zoom();
    doc.set_tool(Tool::Rect);
    doc.add_guide(GuideAxis::Vertical, 100.0);
    let free = 100.0 + GUIDE_SNAP_PX + 2.0;
    doc.pointer_down(free, 50.0);
    doc.pointer_move(250.0, 260.0);
    assert_eq!(doc.preview_shape().unwrap().start.0, free);
}

#[test]
fn shift_still_wins_over_a_snapped_corner() {
    let mut doc = doc_at_unit_zoom();
    doc.set_tool(Tool::Rect);
    doc.add_guide(GuideAxis::Vertical, 100.0);
    doc.set_shift_held(true);
    doc.pointer_down(100.0, 100.0);
    doc.pointer_move(180.0, 300.0);
    let shape = doc.preview_shape().unwrap();
    assert_eq!(
        (shape.end.0 - shape.start.0).abs(),
        (shape.end.1 - shape.start.1).abs()
    );
}

#[test]
fn snapping_is_a_no_op_without_guides() {
    let mut doc = doc_at_unit_zoom();
    doc.set_tool(Tool::Rect);
    doc.pointer_down(101.0, 51.0);
    doc.pointer_move(249.0, 299.0);
    let shape = doc.preview_shape().unwrap();
    assert_eq!(shape.start, (101.0, 51.0));
    assert_eq!(shape.end, (249.0, 299.0));
}

#[test]
fn moving_a_layer_lands_its_edge_on_a_guide_rather_than_the_pointer() {
    let mut doc = doc_at_unit_zoom();
    paint(&mut doc, 1, DocRect::new(0, 0, 99, 99));
    doc.set_tool(Tool::Move);
    let (x0, _, x1, _) = doc.layers[1].content_bounds().unwrap();
    doc.add_guide(GuideAxis::Vertical, x1 + 20.0);

    // Grab well away from the edge, then stop a couple of pixels short of the guide: the
    // pointer keeps its grip, and the layer's right edge is what lands.
    doc.pointer_down(50.0, 50.0);
    doc.pointer_move(50.0 + 18.0, 50.0);
    doc.pointer_up(50.0 + 18.0, 50.0);

    let t = doc.layers[1].transform.unwrap();
    let moved_right = x1 + t.offset_x;
    assert!((moved_right - (x1 + 20.0)).abs() < 1e-3, "{moved_right}");
    assert!((x0 + t.offset_x - x0 - 20.0).abs() < 1e-3);
}

#[test]
fn a_move_far_from_every_guide_tracks_the_pointer_exactly() {
    let mut doc = doc_at_unit_zoom();
    paint(&mut doc, 1, DocRect::new(0, 0, 99, 99));
    doc.set_tool(Tool::Move);
    doc.add_guide(GuideAxis::Vertical, 380.0);
    doc.pointer_down(50.0, 50.0);
    doc.pointer_move(90.0, 70.0);
    doc.pointer_up(90.0, 70.0);
    let t = doc.layers[1].transform.unwrap();
    assert_eq!((t.offset_x, t.offset_y), (40.0, 20.0));
}

#[test]
fn snap_threshold_is_screen_space_so_it_shrinks_as_the_board_zooms_in() {
    let mut doc = doc_at_unit_zoom();
    doc.camera.zoom = 8.0;
    doc.set_tool(Tool::Rect);
    doc.add_guide(GuideAxis::Vertical, 100.0);
    // Two document pixels off is 16 screen pixels at 8x — outside the threshold, though it
    // would have been well inside it at 1x.
    let start = doc.camera.to_screen(102.0, 50.0);
    doc.pointer_down(start.0, start.1);
    doc.pointer_move(start.0 + 50.0, start.1 + 50.0);
    assert_eq!(doc.preview_shape().unwrap().start.0, 102.0);
}

#[test]
fn removing_a_guide_drops_a_drag_that_was_holding_it() {
    let mut doc = doc_at_unit_zoom();
    doc.begin_guide_drag_from_ruler(GuideAxis::Vertical, 40.0, 40.0);
    assert!(doc.remove_guide(0));
    assert!(!doc.is_dragging_guide());
    assert!(!doc.remove_guide(0));
}
