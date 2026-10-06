//! The Crop tool's rectangle, driven the way the shell drives it: pointer events in screen
//! coordinates with `Tool::Crop` selected. Most tests pin the camera to identity once the
//! session has started, so a document coordinate is exactly the screen coordinate the pointer
//! sends and a rect can be compared exactly; the camera tests leave Crop's own zoom alone.

use calumma_core::{CropOverlayStyle, Document, Tool};

const CROP_MIN_SIZE: f32 = 8.0;

fn doc() -> Document {
    let mut d = Document::new("p".into(), "t", 200, 100);
    d.resize_viewport(200.0, 100.0, 1.0);
    d.fit_to_view();
    d
}

fn identity_camera(d: &mut Document) {
    d.camera.zoom = 1.0;
    d.camera.pan_x = 0.0;
    d.camera.pan_y = 0.0;
}

fn cropping() -> Document {
    let mut d = doc();
    d.set_tool(Tool::Crop);
    identity_camera(&mut d);
    d
}

fn restart(d: &mut Document) {
    d.exit_crop();
    d.enter_crop();
    identity_camera(d);
}

fn press(d: &mut Document, x: f32, y: f32) {
    let (sx, sy) = d.camera.to_screen(x, y);
    d.pointer_down(sx, sy);
}

fn drag(d: &mut Document, x: f32, y: f32) {
    let (sx, sy) = d.camera.to_screen(x, y);
    d.pointer_move(sx, sy);
}

fn release(d: &mut Document, x: f32, y: f32) {
    let (sx, sy) = d.camera.to_screen(x, y);
    d.pointer_up(sx, sy);
}

fn ratio(rect: (f32, f32, f32, f32)) -> f32 {
    let (x0, y0, x1, y1) = rect;
    (x1 - x0) / (y1 - y0)
}

#[test]
fn entering_crop_starts_at_the_full_canvas() {
    let mut d = doc();
    d.set_tool(Tool::Crop);
    assert_eq!(d.crop_overlay_rect(), Some((0.0, 0.0, 200.0, 100.0)));
}

#[test]
fn exiting_crop_discards_the_rect() {
    let mut d = cropping();
    d.set_tool(Tool::Pen);
    assert_eq!(d.crop_overlay_rect(), None);
}

#[test]
fn dragging_the_bottom_right_corner_keeps_the_top_left_fixed() {
    let mut d = cropping();
    press(&mut d, 200.0, 100.0);
    drag(&mut d, 150.0, 260.0);
    assert_eq!(d.crop_overlay_rect(), Some((0.0, 0.0, 150.0, 260.0)));
}

#[test]
fn dragging_the_top_left_corner_keeps_the_bottom_right_fixed() {
    let mut d = cropping();
    press(&mut d, 0.0, 0.0);
    drag(&mut d, 40.0, 20.0);
    assert_eq!(d.crop_overlay_rect(), Some((40.0, 20.0, 200.0, 100.0)));
}

#[test]
fn dragging_the_top_right_and_bottom_left_corners_keeps_their_opposite_fixed() {
    let mut d = cropping();
    press(&mut d, 200.0, 0.0);
    drag(&mut d, 150.0, 40.0);
    assert_eq!(d.crop_overlay_rect(), Some((0.0, 40.0, 150.0, 100.0)));

    restart(&mut d);
    press(&mut d, 0.0, 100.0);
    drag(&mut d, 50.0, 60.0);
    assert_eq!(d.crop_overlay_rect(), Some((50.0, 0.0, 200.0, 60.0)));
}

#[test]
fn a_corner_drag_may_expand_the_canvas_past_its_own_edge() {
    let mut d = cropping();
    press(&mut d, 200.0, 100.0);
    drag(&mut d, 260.0, 140.0);
    assert_eq!(d.crop_overlay_rect(), Some((0.0, 0.0, 260.0, 140.0)));
}

/// The pointer reaches further on x than a 2:1 box would need for that y, so x drives.
#[test]
fn locked_aspect_keeps_every_corner_drag_on_ratio() {
    let mut d = cropping();
    d.crop_aspect_lock = Some(2.0);
    press(&mut d, 200.0, 100.0);
    drag(&mut d, 300.0, 130.0);
    let rect = d.crop_overlay_rect().unwrap();
    assert_eq!((rect.0, rect.1), (0.0, 0.0));
    assert!((ratio(rect) - 2.0).abs() < 1e-4);
}

/// The other branch of the corner lock: a 2:1 box reaching y = 120 would need x = 240, but the
/// pointer only reaches 210, so y drives and the width follows from the height.
#[test]
fn a_locked_corner_drag_can_have_the_vertical_axis_drive() {
    let mut d = cropping();
    d.crop_aspect_lock = Some(2.0);
    press(&mut d, 200.0, 100.0);
    drag(&mut d, 210.0, 120.0);
    let rect = d.crop_overlay_rect().unwrap();
    assert_eq!((rect.0, rect.1), (0.0, 0.0));
    assert_eq!(rect.3, 120.0);
    assert!((ratio(rect) - 2.0).abs() < 1e-4);
}

#[test]
fn dragging_the_left_edge_without_a_lock_moves_only_that_edge() {
    let mut d = cropping();
    press(&mut d, 0.0, 50.0);
    drag(&mut d, 30.0, 999.0);
    assert_eq!(d.crop_overlay_rect(), Some((30.0, 0.0, 200.0, 100.0)));
}

#[test]
fn dragging_the_top_and_bottom_edges_moves_only_that_edge() {
    let mut d = cropping();
    press(&mut d, 100.0, 0.0);
    drag(&mut d, 999.0, 20.0);
    assert_eq!(d.crop_overlay_rect(), Some((0.0, 20.0, 200.0, 100.0)));

    restart(&mut d);
    press(&mut d, 100.0, 100.0);
    drag(&mut d, 999.0, 70.0);
    assert_eq!(d.crop_overlay_rect(), Some((0.0, 0.0, 200.0, 70.0)));
}

#[test]
fn dragging_an_edge_with_a_locked_ratio_grows_the_other_axis_around_the_center() {
    let mut d = cropping();
    d.crop_aspect_lock = Some(2.0);
    press(&mut d, 200.0, 50.0);
    drag(&mut d, 300.0, 999.0);
    let rect = d.crop_overlay_rect().unwrap();
    assert_eq!((rect.0, rect.2), (0.0, 300.0));
    assert!(
        ((rect.1 + rect.3) * 0.5 - 50.0).abs() < 1e-4,
        "the vertical center must not move"
    );
    assert!((ratio(rect) - 2.0).abs() < 1e-4);
}

#[test]
fn dragging_a_horizontal_edge_with_a_locked_ratio_grows_the_other_axis_around_the_center() {
    let mut d = cropping();
    d.crop_aspect_lock = Some(2.0);
    press(&mut d, 100.0, 100.0);
    drag(&mut d, 999.0, 130.0);
    let rect = d.crop_overlay_rect().unwrap();
    assert_eq!((rect.1, rect.3), (0.0, 130.0));
    assert!(
        ((rect.0 + rect.2) * 0.5 - 100.0).abs() < 1e-4,
        "the horizontal center must not move"
    );
    assert!((ratio(rect) - 2.0).abs() < 1e-4);
}

#[test]
fn dragging_inside_the_rect_moves_it_without_resizing() {
    let mut d = cropping();
    press(&mut d, 100.0, 50.0);
    drag(&mut d, 130.0, 70.0);
    assert_eq!(d.crop_overlay_rect(), Some((30.0, 20.0, 230.0, 120.0)));
}

#[test]
fn a_handle_cannot_drag_the_rect_through_itself() {
    let mut d = cropping();
    press(&mut d, 200.0, 100.0);
    drag(&mut d, -500.0, -500.0);
    let (x0, y0, x1, y1) = d.crop_overlay_rect().unwrap();
    assert!(x1 - x0 >= CROP_MIN_SIZE - 1e-4);
    assert!(y1 - y0 >= CROP_MIN_SIZE - 1e-4);
}

#[test]
fn dragging_the_right_or_bottom_edge_through_the_opposite_edge_is_clamped() {
    let mut d = cropping();
    press(&mut d, 200.0, 50.0);
    drag(&mut d, -500.0, 50.0);
    let (x0, _, x1, _) = d.crop_overlay_rect().unwrap();
    assert!((x1 - x0 - CROP_MIN_SIZE).abs() < 1e-4);

    restart(&mut d);
    press(&mut d, 100.0, 100.0);
    drag(&mut d, 100.0, -500.0);
    let (_, y0, _, y1) = d.crop_overlay_rect().unwrap();
    assert!((y1 - y0 - CROP_MIN_SIZE).abs() < 1e-4);
}

#[test]
fn a_vertical_edge_also_cannot_drag_through_itself() {
    let mut d = cropping();
    press(&mut d, 0.0, 50.0);
    drag(&mut d, 500.0, 50.0);
    let (x0, _, x1, _) = d.crop_overlay_rect().unwrap();
    assert!(x1 - x0 >= CROP_MIN_SIZE - 1e-4);
}

#[test]
fn clicking_outside_the_rect_grabs_nothing() {
    let mut d = cropping();
    press(&mut d, -50.0, -50.0);
    drag(&mut d, 10.0, 10.0);
    assert_eq!(d.crop_overlay_rect(), Some((0.0, 0.0, 200.0, 100.0)));
}

#[test]
fn a_drag_with_no_session_does_nothing() {
    let mut d = doc();
    d.tool = Tool::Crop;
    press(&mut d, 100.0, 50.0);
    drag(&mut d, 10.0, 10.0);
    assert_eq!(d.crop_overlay_rect(), None);
}

#[test]
fn releasing_a_drag_lets_go_without_touching_the_rect() {
    let mut d = cropping();
    press(&mut d, 200.0, 100.0);
    release(&mut d, 200.0, 100.0);
    drag(&mut d, 0.0, 0.0);
    assert_eq!(d.crop_overlay_rect(), Some((0.0, 0.0, 200.0, 100.0)));
}

/// Mirrors committing a shape or a fill: the tool stays selected and ready to crop again, so
/// the rect comes back rather than leaving the overlay with nothing to draw.
#[test]
fn commit_crop_applies_the_rounded_rect_and_rearms_a_fresh_one() {
    let mut d = cropping();
    press(&mut d, 200.0, 100.0);
    drag(&mut d, 150.4, 80.6);
    d.commit_crop();
    assert_eq!((d.width, d.height), (150, 81));
    assert_eq!(d.crop_overlay_rect(), Some((0.0, 0.0, 150.0, 81.0)));
}

#[test]
fn committing_with_no_active_rect_does_nothing() {
    let mut d = doc();
    d.commit_crop();
    assert_eq!((d.width, d.height), (200, 100));
}

/// At a normal fit the paper already fills almost the whole viewport, leaving nowhere on
/// screen to drag a handle past the canvas edge — Crop zooms out further to make expanding
/// reachable.
#[test]
fn entering_crop_zooms_out_to_leave_room_to_expand_past_the_edge() {
    let mut d = doc();
    let fit_zoom = d.camera.zoom;
    d.set_tool(Tool::Crop);
    assert!(d.camera.zoom < fit_zoom);
}

#[test]
fn entering_crop_never_zooms_in() {
    let mut d = doc();
    d.camera.zoom = 0.1;
    d.set_tool(Tool::Crop);
    assert_eq!(d.camera.zoom, 0.1);
}

#[test]
fn exiting_crop_restores_the_camera_from_before_it_zoomed_out() {
    let mut d = doc();
    let camera_before = d.camera;
    d.set_tool(Tool::Crop);
    assert_ne!(d.camera, camera_before);
    d.set_tool(Tool::Pen);
    assert_eq!(d.camera, camera_before);
}

/// `commit_crop` re-arms a fresh session on the resized canvas, zoomed out for drag room again
/// immediately; cancelling that second session returns to a plain fit of the committed result,
/// not to the view from before the very first crop.
#[test]
fn committing_a_crop_rearms_the_camera_baseline_for_the_next_exit() {
    let mut d = doc();
    d.set_tool(Tool::Crop);
    press(&mut d, 200.0, 100.0);
    drag(&mut d, 150.4, 80.6);
    d.commit_crop();
    assert!(!d.camera.is_fit(d.width as f32, d.height as f32));
    d.set_tool(Tool::Pen);
    assert!(d.camera.is_fit(d.width as f32, d.height as f32));
}

#[test]
fn overlay_lines_are_empty_before_a_crop_session_starts() {
    assert!(doc().crop_overlay_lines().is_empty());
}

#[test]
fn overlay_lines_are_empty_when_off_and_populate_per_style() {
    let mut d = cropping();
    for (style, lines) in [
        (CropOverlayStyle::Off, 0),
        (CropOverlayStyle::RuleOfThirds, 4),
        (CropOverlayStyle::Grid, 6),
        (CropOverlayStyle::Diagonal, 2),
        (CropOverlayStyle::GoldenRatio, 4),
    ] {
        d.crop_overlay_style = style;
        assert_eq!(d.crop_overlay_lines().len(), lines, "{style:?}");
    }
}

#[test]
fn rule_of_thirds_lines_sit_at_the_thirds() {
    let mut d = cropping();
    d.crop_overlay_style = CropOverlayStyle::RuleOfThirds;
    let xs: Vec<f32> = d
        .crop_overlay_lines()
        .iter()
        .filter(|(a, b)| a.0 == b.0)
        .map(|(a, _)| a.0)
        .collect();
    assert!(xs.iter().any(|&x| (x - 200.0 / 3.0).abs() < 1e-3));
    assert!(xs.iter().any(|&x| (x - 400.0 / 3.0).abs() < 1e-3));
}

#[test]
fn overlay_style_round_trips_through_its_wire_value() {
    for style in [
        CropOverlayStyle::Off,
        CropOverlayStyle::RuleOfThirds,
        CropOverlayStyle::Grid,
        CropOverlayStyle::Diagonal,
        CropOverlayStyle::GoldenRatio,
    ] {
        assert_eq!(CropOverlayStyle::from_u32(style as u32), Some(style));
    }
    assert_eq!(CropOverlayStyle::from_u32(99), None);
}
