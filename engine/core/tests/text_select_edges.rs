//! Two things `text_select.rs` never had a test for: `text_set_caret_at` (a plain click,
//! outside a drag or a double/triple-click) and `text_caret_color`. And one shape every method
//! in that file shares — a no-op guard for "nothing is being edited" — that no existing test
//! ever reached, because every existing test starts a session before calling into it.

use calumma_core::*;

fn board() -> Document {
    let mut doc = Document::new("p".into(), "t", 512, 512);
    doc.resize_viewport(512.0, 512.0, 1.0);
    doc.fit_to_view();
    doc.tool = Tool::Text;
    doc.text_style.size = 48.0;
    doc
}

fn press(doc: &mut Document, x: f32, y: f32) {
    let (sx, sy) = doc.camera.to_screen(x, y);
    doc.pointer_down(sx, sy);
}

fn release(doc: &mut Document, x: f32, y: f32) {
    let (sx, sy) = doc.camera.to_screen(x, y);
    doc.pointer_up(sx, sy);
}

fn typed(text: &str) -> Document {
    let mut doc = board();
    press(&mut doc, 40.0, 100.0);
    release(&mut doc, 40.0, 100.0);
    doc.text_insert(text);
    doc
}

#[test]
fn caret_color_is_the_runs_own_color_while_editing_and_the_inks_otherwise() {
    let mut doc = typed("hello");
    let run_color = doc.active_text_run().expect("a run").color;
    assert_eq!(doc.text_caret_color(), run_color);

    doc.commit_text();
    assert_eq!(
        doc.text_caret_color(),
        doc.color,
        "once nothing is being edited the caret falls back to the active ink"
    );
}
