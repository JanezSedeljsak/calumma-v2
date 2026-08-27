//! The brush cursor is a promise: a ring is drawn exactly where and when a stamp would land.
//! These are the cases where that promise has to be withheld.

use calumma_core::limits::BRUSH_MIN_SCREEN_PX;
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
    assert!(
        doc.brush_ring().is_some(),
        "a vector layer pins vector mode on, so the pen still lands something"
    );

    doc.tool = Tool::Eraser;
    assert_eq!(
        doc.brush_ring(),
        None,
        "an eraser has no vector form and no pixels to take away"
    );
}

/// Vector mode draws into a layer of its own, so the active layer's refusal does not apply —
/// and the ring still has to show, because a stroke really will land.
#[test]
fn vector_mode_keeps_the_ring_over_a_layer_that_could_not_take_pixels() {
    let mut doc = board();
    hover(&mut doc, 40.0, 40.0);
    doc.begin_text_at(20.0, 20.0);
    doc.text_insert("hi");
    doc.commit_text();
    doc.tool = Tool::Pen;
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

/// A 4K board fitted to a window puts a whole document pixel well under a screen one, so the
/// finest brush would be invisible — and a stroke you cannot see is one you cannot aim. The
/// brush carries a second floor in screen pixels that rises as the board is zoomed out.
#[test]
fn the_brush_never_falls_under_three_screen_pixels() {
    let mut doc = Document::new("p".into(), "t", 4096, 4096);
    doc.resize_viewport(800.0, 600.0, 1.0);
    doc.fit_to_view();
    doc.brush_size = BRUSH_SIZE_MIN;

    let zoom = doc.camera.zoom;
    assert!(
        BRUSH_SIZE_MIN * zoom < BRUSH_MIN_SCREEN_PX,
        "the finest brush is under the floor at this zoom, or the test proves nothing"
    );
    let on_screen = doc.effective_brush_size() * zoom;
    assert!(
        (on_screen - BRUSH_MIN_SCREEN_PX).abs() < 1e-3,
        "brush drew {on_screen} screen px"
    );
}

/// The floor is on what can be seen. Zoomed in the brush can already be seen, so it stays the
/// size that was asked for and never grows past it.
#[test]
fn zooming_in_leaves_the_chosen_size_alone() {
    let mut doc = Document::new("p".into(), "t", 4096, 4096);
    doc.resize_viewport(800.0, 600.0, 1.0);
    doc.camera.zoom = 4.0;
    doc.brush_size = BRUSH_SIZE_MIN;
    assert_eq!(doc.effective_brush_size(), BRUSH_SIZE_MIN);

    doc.brush_size = 96.0;
    assert_eq!(doc.effective_brush_size(), 96.0);
}

/// Zoomed out, a brush already wider than the floor is untouched — the floor lifts the fine end
/// only, it does not push every brush around.
#[test]
fn a_wide_brush_is_not_moved_by_the_floor() {
    let mut doc = Document::new("p".into(), "t", 4096, 4096);
    doc.resize_viewport(800.0, 600.0, 1.0);
    doc.fit_to_view();
    doc.brush_size = 500.0;
    assert_eq!(doc.effective_brush_size(), 500.0);
}

/// A vector stroke keeps its width in the item and is redrawn at every zoom, so folding the
/// current camera into it would bake today's zoom into the document.
#[test]
fn vector_mode_is_exempt_from_the_screen_floor() {
    let mut doc = Document::new("p".into(), "t", 4096, 4096);
    doc.resize_viewport(800.0, 600.0, 1.0);
    doc.fit_to_view();
    doc.brush_size = BRUSH_SIZE_MIN;
    doc.set_vector_mode(true);
    assert_eq!(doc.effective_brush_size(), BRUSH_SIZE_MIN);
}

/// The ring promises a stamp, so it stops where the stamps do. Over the desk on an ordinary
/// layer there is nothing to paint — and the shell hides its own cursor for exactly as long as
/// there is a ring, so saying "yes" out here left the pointer invisible over the canvas island.
#[test]
fn no_ring_off_the_end_of_the_layer() {
    let mut doc = board();
    doc.set_tool(Tool::Pen);

    hover(&mut doc, 40.0, 40.0);
    assert!(doc.brush_ring().is_some(), "on the paper there is a ring");

    let (w, h) = (doc.width as f32, doc.height as f32);
    hover(&mut doc, w + 40.0, h / 2.0);
    assert_eq!(doc.brush_ring(), None, "past the right edge there is not");
    hover(&mut doc, -40.0, h / 2.0);
    assert_eq!(doc.brush_ring(), None, "nor past the left");
}

/// A pasted image that overflows the paper takes paint out over the desk, so the ring has to
/// reach there too — the layer's extent is what decides, not the document.
#[test]
fn the_ring_follows_a_pasted_layer_past_the_paper() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    doc.resize_viewport(400.0, 400.0, 1.0);
    doc.camera.zoom = 1.0;
    doc.camera.pan_x = 0.0;
    doc.camera.pan_y = 0.0;
    doc.set_tool(Tool::Pen);

    let side = 200usize;
    let image = vec![255u8; side * side * 4];
    doc.paste_image_as_layer("Pasted", &image, side as u32, side as u32);

    // Centred on a 64px board, so the layer runs from -68 to 131 — well past the paper.
    doc.set_pointer_hover(110.0, 32.0);
    assert!(
        doc.brush_ring().is_some(),
        "the overflow takes paint, so it takes a ring"
    );

    doc.set_pointer_hover(400.0, 32.0);
    assert_eq!(doc.brush_ring(), None, "but not past the layer itself");
}
