//! Clone/heal source crosshair overlay instances.

use calumma_core::{Document, Tool};
use calumma_render::compose::clone_source_overlay_instances;

fn board() -> Document {
    let mut doc = Document::new("p".into(), "Clone", 128, 128);
    doc.resize_viewport(128.0, 128.0, 1.0);
    doc.fit_to_view();
    doc
}

fn hover(doc: &mut Document, x: f32, y: f32) {
    let (sx, sy) = doc.camera.to_screen(x, y);
    doc.set_pointer_hover(sx, sy);
}

fn board_with_clone_source() -> Document {
    let mut doc = board();
    doc.tool = Tool::Clone;
    doc.set_clone_anchor(40.0, 50.0);
    hover(&mut doc, 80.0, 60.0);
    doc
}

#[test]
fn clone_source_overlay_is_empty_without_a_source_point() {
    let mut doc = board();
    doc.tool = Tool::Clone;
    hover(&mut doc, 80.0, 60.0);
    assert!(clone_source_overlay_instances(&doc).is_empty());
}

#[test]
fn clone_source_overlay_is_empty_for_other_tools() {
    let mut doc = board_with_clone_source();
    doc.tool = Tool::Pen;
    assert!(clone_source_overlay_instances(&doc).is_empty());
}

#[test]
fn clone_source_overlay_draws_a_crosshair_at_the_source() {
    let doc = board_with_clone_source();
    let instances = clone_source_overlay_instances(&doc);
    assert!(
        instances.len() >= 4,
        "a crosshair is at least two perpendicular segments per tone"
    );
    assert!(instances.iter().any(|i| i.color[3] > 0.9));
}

#[test]
fn heal_tool_shares_the_clone_source_overlay() {
    let mut doc = board_with_clone_source();
    doc.tool = Tool::Heal;
    assert!(!clone_source_overlay_instances(&doc).is_empty());
}
