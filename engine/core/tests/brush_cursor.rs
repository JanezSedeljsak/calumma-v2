//! The brush cursor is a promise: a ring is drawn exactly where and when a stamp would land.
//! These are the cases where that promise has to be withheld.

use calumma_core::*;

const DOC: u32 = 256;

fn board() -> Document {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.resize_viewport(DOC as f32, DOC as f32, 1.0);
    doc.camera.zoom = 1.0;
    doc.camera.pan_x = 0.0;
    doc.camera.pan_y = 0.0;
    doc.add_layer("Paint");
    doc.tool = Tool::Pen;
    doc.brush_size = 20.0;
    doc
}

fn hover(doc: &mut Document, x: f32, y: f32) {
    let (sx, sy) = doc.camera.to_screen(x, y);
    doc.set_pointer_hover(sx, sy);
}

#[test]
fn there_is_no_ring_until_the_pointer_is_over_the_board() {
    let doc = board();
    assert_eq!(doc.brush_ring(), None);
}

#[test]
fn the_ring_sits_on_the_pointer_at_half_the_brush_size() {
    let mut doc = board();
    hover(&mut doc, 40.0, 60.0);

    let ((cx, cy), radius) = doc.brush_ring().expect("ring");

    assert!((cx - 40.0).abs() < 1e-3);
    assert!((cy - 60.0).abs() < 1e-3);
    assert_eq!(radius, 10.0, "a 20px brush is a 20px circle");
}

/// The shell hands the engine screen coordinates, exactly as it does for a click, so the ring
/// has to survive a camera that is panned and zoomed rather than assuming 1:1.
#[test]
fn the_ring_is_placed_through_the_camera_not_beside_it() {
    let mut doc = board();
    doc.camera.zoom = 2.0;
    doc.camera.pan_x = -30.0;
    doc.camera.pan_y = 12.0;
    hover(&mut doc, 40.0, 60.0);

    let ((cx, cy), radius) = doc.brush_ring().expect("ring");

    assert!((cx - 40.0).abs() < 1e-3, "{cx}");
    assert!((cy - 60.0).abs() < 1e-3, "{cy}");
    assert_eq!(radius, 10.0, "the radius stays in document units");
}

#[test]
fn every_brush_shaped_tool_gets_a_ring_and_nothing_else_does() {
    let mut doc = board();
    hover(&mut doc, 40.0, 40.0);

    for tool in [Tool::Pen, Tool::Eraser, Tool::Blur] {
        doc.tool = tool;
        assert!(doc.brush_ring().is_some(), "{tool:?}");
    }
    for tool in [
        Tool::Rect,
        Tool::Line,
        Tool::SelectRect,
        Tool::SelectLasso,
        Tool::MagicWand,
        Tool::Fill,
        Tool::Eyedropper,
        Tool::Text,
        Tool::Move,
    ] {
        doc.tool = tool;
        assert_eq!(doc.brush_ring(), None, "{tool:?}");
    }
}

/// No ring is the honest signal that a stroke would do nothing: the engine already refuses to
/// paint on these layers, silently, and drawing the cursor anyway would promise otherwise.
#[test]
fn a_layer_that_refuses_paint_shows_no_ring() {
    let mut doc = board();
    hover(&mut doc, 40.0, 40.0);
    assert!(doc.brush_ring().is_some());

    doc.set_layer_locked(doc.active_layer, true);
    assert_eq!(doc.brush_ring(), None, "locked");

    doc.set_layer_locked(doc.active_layer, false);
    assert!(doc.brush_ring().is_some(), "unlocking gives it back");

    doc.add_vector_layer(
        "V",
        VectorItem::Path(VectorPath {
            points: vec![(0.0, 0.0), (10.0, 10.0)],
            closed: false,
            fill: false,
            color: [0, 0, 0, 255],
            stroke: true,
            stroke_color: [0, 0, 0, 255],
            stroke_width: 2.0,
        }),
    );
    assert_eq!(doc.brush_ring(), None, "a vector layer has no pixels");
}

/// Vector mode draws into a layer of its own, so the active layer's refusal does not apply —
/// and the ring still has to show, because a stroke really will land.
#[test]
fn vector_mode_keeps_the_ring_over_a_layer_that_could_not_take_pixels() {
    let mut doc = board();
    hover(&mut doc, 40.0, 40.0);
    doc.add_vector_layer(
        "V",
        VectorItem::Path(VectorPath {
            points: vec![(0.0, 0.0), (10.0, 10.0)],
            closed: false,
            fill: false,
            color: [0, 0, 0, 255],
            stroke: true,
            stroke_color: [0, 0, 0, 255],
            stroke_width: 2.0,
        }),
    );
    assert_eq!(doc.brush_ring(), None);

    doc.set_vector_mode(true);

    assert!(doc.brush_ring().is_some());
}

#[test]
fn transform_owns_the_pointer_so_the_brush_stands_down() {
    let mut doc = board();
    doc.layers[doc.active_layer]
        .tiles_mut()
        .unwrap()
        .set_pixel(10, 10, [1, 2, 3, 255]);
    hover(&mut doc, 40.0, 40.0);
    assert!(doc.enter_transform());

    assert_eq!(doc.brush_ring(), None);

    doc.exit_transform();
    assert!(doc.brush_ring().is_some());
}

#[test]
fn leaving_the_board_takes_the_ring_with_it() {
    let mut doc = board();
    hover(&mut doc, 40.0, 40.0);
    assert!(doc.brush_ring().is_some());

    doc.clear_pointer_hover();

    assert_eq!(doc.brush_ring(), None);
}

/// Painting moves the pointer too, and the ring has to keep up with it — a cursor that froze
/// where the stroke began would be worse than none.
#[test]
fn the_ring_follows_the_pointer_through_a_stroke() {
    let mut doc = board();
    let (sx, sy) = doc.camera.to_screen(30.0, 30.0);
    doc.pointer_down(sx, sy);
    assert_eq!(doc.brush_ring().expect("ring").0 .0, 30.0);

    let (mx, my) = doc.camera.to_screen(90.0, 30.0);
    doc.pointer_move(mx, my);

    assert_eq!(doc.brush_ring().expect("ring").0 .0, 90.0);
}

#[test]
fn a_brush_with_no_size_has_no_ring_to_draw() {
    let mut doc = board();
    hover(&mut doc, 40.0, 40.0);
    doc.brush_size = 0.0;

    assert_eq!(doc.brush_ring(), None);
}
