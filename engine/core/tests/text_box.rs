//! Wrapped text boxes: the gesture that finally reaches `TextRun::wrap_width`.
//!
//! Nothing in layout changed for this — the engine has honoured a wrap width since text
//! existed. What is under test is the reach: a drag with the Text tool sweeps a box, a click
//! still places a point, and the width the run keeps is the clamped one.

use calumma_core::*;

fn board() -> Document {
    let mut doc = Document::new("p".into(), "t", 512, 512);
    doc.resize_viewport(512.0, 512.0, 1.0);
    doc.fit_to_view();
    doc.tool = Tool::Text;
    doc.text_style.size = 24.0;
    doc
}

fn press(doc: &mut Document, x: f32, y: f32) {
    let (sx, sy) = doc.camera.to_screen(x, y);
    doc.pointer_down(sx, sy);
}

fn drag(doc: &mut Document, x: f32, y: f32) {
    let (sx, sy) = doc.camera.to_screen(x, y);
    doc.pointer_move(sx, sy);
}

fn release(doc: &mut Document, x: f32, y: f32) {
    let (sx, sy) = doc.camera.to_screen(x, y);
    doc.pointer_up(sx, sy);
}

fn run(doc: &Document) -> &TextRun {
    doc.active_text_run().expect("a run")
}

/// Points make the round trip through the camera to reach the engine, so a doc coordinate
/// comes back a fraction off what went in. Every geometry assertion here is about the box the
/// drag described, never about the last bit of a float.
#[track_caller]
fn near(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.01,
        "{actual} should be about {expected}"
    );
}

#[track_caller]
fn near_wrap(doc: &Document, expected: f32) {
    near(run(doc).wrap_width.expect("a wrapped box"), expected);
}

#[test]
fn a_click_still_places_a_point_that_grows_with_its_text() {
    let mut doc = board();
    press(&mut doc, 40.0, 100.0);
    release(&mut doc, 40.0, 100.0);
    assert_eq!(run(&doc).wrap_width, None);
}

#[test]
fn dragging_with_the_text_tool_sweeps_a_wrap_box() {
    let mut doc = board();
    press(&mut doc, 40.0, 60.0);
    drag(&mut doc, 240.0, 160.0);
    release(&mut doc, 240.0, 160.0);
    near_wrap(&doc, 200.0);
    let origin = run(&doc).origin;
    near(origin.0, 40.0);
    near(origin.1, 60.0);
}

#[test]
fn a_box_dragged_up_and_to_the_left_is_the_same_box() {
    let mut doc = board();
    press(&mut doc, 240.0, 160.0);
    drag(&mut doc, 40.0, 60.0);
    release(&mut doc, 40.0, 60.0);
    near_wrap(&doc, 200.0);
    let origin = run(&doc).origin;
    near(origin.0, 40.0);
    near(origin.1, 60.0);
}

/// A drag that never gets wide enough is a click. Below `TEXT_WRAP_MIN_WIDTH` every word lands
/// on its own line, which is never what a small wobble of the mouse meant.
#[test]
fn a_drag_too_narrow_to_wrap_falls_back_to_a_point() {
    let mut doc = board();
    press(&mut doc, 40.0, 100.0);
    drag(&mut doc, 44.0, 104.0);
    release(&mut doc, 44.0, 104.0);
    let origin = run(&doc).origin;
    assert_eq!(run(&doc).wrap_width, None);
    near(origin.0, 40.0);
    near(origin.1, 100.0 - 24.0 * 0.5);
}

#[test]
fn a_box_wraps_its_text_over_several_rows() {
    let mut doc = board();
    press(&mut doc, 40.0, 60.0);
    drag(&mut doc, 140.0, 200.0);
    release(&mut doc, 140.0, 200.0);
    doc.text_insert("wrapping words onto several rows");
    let (_, y0, _, y1) = doc.text_box().expect("a box");
    assert!(
        y1 - y0 > run(&doc).line_spacing() * 2.0,
        "a 100pt-wide box should have taken more than two rows"
    );
    doc.text_step_caret(Step::DocStart, false);
    doc.text_step_caret(Step::DocEnd, true);
    assert!(
        doc.text_selection_rows().len() > 2,
        "and the highlight follows the visual rows"
    );
}

#[test]
fn the_engine_entry_takes_a_rectangle_directly() {
    let mut doc = board();
    doc.begin_text_box(200.0, 300.0, 60.0, 120.0);
    assert_eq!(run(&doc).origin, (60.0, 120.0));
    assert_eq!(run(&doc).wrap_width, Some(140.0));
}

#[test]
fn the_wrap_width_setter_clamps_and_switches_off_at_zero() {
    let mut doc = board();
    press(&mut doc, 40.0, 100.0);
    release(&mut doc, 40.0, 100.0);

    doc.set_text_wrap_width(Some(180.0));
    assert_eq!(doc.text_wrap_width(), Some(180.0));

    doc.set_text_wrap_width(Some(2.0));
    assert_eq!(
        doc.text_wrap_width(),
        None,
        "a box narrower than the floor is no box"
    );

    doc.set_text_wrap_width(Some(180.0));
    doc.set_text_wrap_width(None);
    assert_eq!(doc.text_wrap_width(), None);
}

#[test]
fn a_box_narrows_and_re_rasterizes_without_being_retyped() {
    let mut doc = board();
    press(&mut doc, 40.0, 60.0);
    release(&mut doc, 40.0, 60.0);
    doc.text_insert("wrapping words onto several rows");
    let wide = doc.text_box().expect("a box");
    doc.set_text_wrap_width(Some(80.0));
    let narrow = doc.text_box().expect("a box");
    assert!(narrow.2 - narrow.0 < wide.2 - wide.0);
    assert!(
        narrow.3 - narrow.1 > wide.3 - wide.1,
        "narrower means taller: the run re-wrapped"
    );
}

#[test]
fn a_box_survives_the_session_and_a_click_re_enters_it() {
    let mut doc = board();
    press(&mut doc, 40.0, 60.0);
    drag(&mut doc, 240.0, 160.0);
    release(&mut doc, 240.0, 160.0);
    doc.text_insert("boxed");
    let layer = doc.active_layer;
    doc.commit_text();
    near(
        doc.layers[layer]
            .run()
            .expect("run")
            .wrap_width
            .expect("a wrapped box"),
        200.0,
    );

    press(&mut doc, 60.0, 70.0);
    release(&mut doc, 60.0, 70.0);
    assert_eq!(
        doc.text_edit_layer(),
        Some(layer),
        "the same layer reopened"
    );
    near_wrap(&doc, 200.0);
}

/// `pointer_move` answers whether the *pixels* moved, and the renderer turns that into a
/// content invalidate rather than an overlay one. Re-wrapping re-rasterizes; sweeping a
/// selection only moves furniture the overlay redraws every frame anyway.
#[test]
fn a_box_drag_reports_a_content_change_and_a_select_drag_does_not() {
    let mut doc = board();
    let (sx0, sy0) = doc.camera.to_screen(40.0, 60.0);
    let (sx1, sy1) = doc.camera.to_screen(240.0, 160.0);
    doc.pointer_down(sx0, sy0);
    assert!(
        doc.pointer_move(sx1, sy1),
        "sweeping a wrap box re-rasterizes the run"
    );
    doc.pointer_up(sx1, sy1);
    doc.text_insert("boxed");
    doc.commit_text();

    let (bx0, by0, bx1, _) = {
        press(&mut doc, 60.0, 70.0);
        let boxed = doc.text_box().expect("a box");
        release(&mut doc, 60.0, 70.0);
        boxed
    };
    let (sx0, sy0) = doc.camera.to_screen(bx0 + 2.0, by0 + 8.0);
    let (sx1, sy1) = doc.camera.to_screen(bx1 - 2.0, by0 + 8.0);
    doc.pointer_down(sx0, sy0);
    assert!(
        !doc.pointer_move(sx1, sy1),
        "sweeping a selection leaves the pixels alone"
    );
    doc.pointer_up(sx1, sy1);
    assert!(doc.text_selection().is_some());
}

/// A box is a paragraph property, so a knob that acts on a *selection* must not take it.
#[test]
fn styling_a_selection_inside_a_box_leaves_the_wrap_alone() {
    let mut doc = board();
    press(&mut doc, 40.0, 60.0);
    drag(&mut doc, 200.0, 160.0);
    release(&mut doc, 200.0, 160.0);
    doc.text_insert("wrapping words onto rows");
    doc.text_step_caret(Step::WordLeft, true);
    doc.set_text_bold(true);
    near_wrap(&doc, 160.0);
    assert!(run(&doc).style_at(run(&doc).text.len() - 1).bold);
}
