//! The three ways a click that looked like it landed on a layer used to be rejected.
//!
//! Each test here is one row of the 2026-08-24 audit that reopened this: a 1px stroke clicked
//! two document pixels off centre missed, a pixel at alpha 1 claimed the click, and a locked
//! layer swallowed it in silence.

use calumma_core::document::*;
use calumma_core::shape::Tool;
use calumma_core::tool_gate::ToolBlock;
use calumma_core::*;

const DOC: u32 = 200;

fn doc_with_viewport() -> Document {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.resize_viewport(DOC as f32, DOC as f32, 1.0);
    doc.fit_to_view();
    doc
}

fn paint(doc: &mut Document, index: usize, rect: DocRect, rgba: [u8; 4]) {
    doc.layers[index]
        .tiles_mut()
        .unwrap()
        .paint_rect(rect, |_, _, _| Some(rgba));
}

/// A vertical hairline — the shape the whole plan is about.
fn hairline(doc: &mut Document, index: usize) {
    paint(
        doc,
        index,
        DocRect::new(100, 40, 100, 160),
        [10, 10, 10, 255],
    );
}

#[test]
fn a_click_two_pixels_off_a_hairline_still_picks_it() {
    let mut doc = doc_with_viewport();
    hairline(&mut doc, 1);
    assert_eq!(doc.layer_at(100.5, 100.0), Some(1), "dead centre");
    assert_eq!(doc.layer_at(102.5, 100.0), Some(1), "two px right");
    assert_eq!(doc.layer_at(98.0, 100.0), Some(1), "two px left");
}

#[test]
fn slack_runs_out_eventually() {
    let mut doc = doc_with_viewport();
    hairline(&mut doc, 1);
    assert_eq!(
        doc.layer_at(140.0, 100.0),
        None,
        "forty px is not a near miss"
    );
}

/// Slack is measured in *screen* pixels, so the document-space reach has to grow as the board
/// zooms out — which is exactly when a hairline is hardest to hit.
#[test]
fn the_slack_is_screen_measured_so_it_widens_as_you_zoom_out() {
    let mut doc = doc_with_viewport();
    hairline(&mut doc, 1);
    doc.camera.zoom = 4.0;
    let close = doc.layer_at(101.0, 100.0).is_some();
    let far = doc.layer_at(106.0, 100.0).is_some();
    assert!(close, "a near miss still hits when zoomed in");
    assert!(!far, "but the reach is tight at 4x");

    doc.camera.zoom = 0.25;
    assert_eq!(
        doc.layer_at(106.0, 100.0),
        Some(1),
        "the same point is within reach at 0.25x"
    );
}

#[test]
fn an_almost_invisible_pixel_no_longer_claims_the_click() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(40, 40, 60, 60), [255, 0, 0, 1]);
    assert_eq!(doc.layer_at(50.0, 50.0), None);
    paint(&mut doc, 1, DocRect::new(40, 40, 60, 60), [255, 0, 0, 40]);
    assert_eq!(doc.layer_at(50.0, 50.0), Some(1));
}

/// The threshold is on the *composited* alpha, so a layer nobody can see because of its
/// opacity is as hard to grab as one nobody can see because of its pixels.
#[test]
fn a_nearly_transparent_layer_is_as_hard_to_grab_as_it_is_to_see() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(40, 40, 60, 60), [255, 0, 0, 255]);
    doc.set_layer_opacity(1, 0.01);
    assert_eq!(doc.layer_at(50.0, 50.0), None);
    doc.set_layer_opacity(1, 0.5);
    assert_eq!(doc.layer_at(50.0, 50.0), Some(1));
}

#[test]
fn a_locked_layer_stays_unpickable_and_falls_through() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(10, 10, 90, 90), [0, 0, 255, 255]);
    doc.add_layer("Top");
    let top = doc.active_layer;
    paint(
        &mut doc,
        top,
        DocRect::new(10, 10, 90, 90),
        [255, 0, 0, 255],
    );
    doc.layers[top].locked = true;
    assert_eq!(
        doc.layer_at(50.0, 50.0),
        Some(1),
        "the click falls through to the layer below, the way Photoshop does"
    );
    assert_eq!(doc.locked_layer_at(50.0, 50.0), Some(top));
}

#[test]
fn a_click_swallowed_by_a_locked_layer_says_so_once() {
    let mut doc = doc_with_viewport();
    let only = doc.active_layer;
    paint(
        &mut doc,
        only,
        DocRect::new(10, 10, 90, 90),
        [255, 0, 0, 255],
    );
    doc.layers[only].locked = true;
    doc.tool = Tool::Move;

    assert!(!doc.begin_move_at(50.0, 50.0));
    assert_eq!(doc.take_tool_block_notice(), Some(ToolBlock::LayerLocked));

    assert!(!doc.begin_move_at(52.0, 52.0));
    assert_eq!(
        doc.take_tool_block_notice(),
        None,
        "saying it again on every press would be nagging, not feedback"
    );
}

#[test]
fn a_click_on_nothing_at_all_says_nothing() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(10, 10, 40, 40), [255, 0, 0, 255]);
    doc.tool = Tool::Move;
    assert!(!doc.begin_move_at(150.0, 150.0));
    assert_eq!(doc.take_tool_block_notice(), None);
}

/// `active_layer_covers` decides whether a click inside the transform box belongs to the
/// active layer or is offered to the stack, so it has to answer with the same slack the stack
/// walk uses — otherwise a near miss keeps the layer *and* would have picked it.
#[test]
fn the_transform_retarget_path_uses_the_same_slack() {
    let mut doc = doc_with_viewport();
    hairline(&mut doc, 1);
    doc.add_layer("Above");
    let above = doc.active_layer;
    paint(
        &mut doc,
        above,
        DocRect::new(10, 10, 30, 30),
        [0, 255, 0, 255],
    );
    doc.set_active_layer(1);
    doc.enter_transform();
    let (sx, sy) = doc.camera.to_screen(102.5, 100.0);
    doc.pointer_down(sx, sy);
    assert_eq!(
        doc.active_layer, 1,
        "a two-pixel miss on the hairline is still the hairline"
    );
}
