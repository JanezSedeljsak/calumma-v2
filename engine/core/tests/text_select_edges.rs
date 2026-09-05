//! Two things `text_select.rs` never had a test for: `text_set_caret_at` (a plain click,
//! outside a drag or a double/triple-click) and `text_caret_color`. And one shape every method
//! in that file shares — a no-op guard for "nothing is being edited" — that no existing test
//! ever reached, because every existing test starts a session before calling into it.

use calumma_core::*;
use calumma_text::caret_rect;

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
fn a_click_places_the_caret_at_the_point_clicked() {
    let mut doc = typed("hello world");
    let start = caret_rect(doc.active_text_run().expect("a run"), 0);

    doc.text_set_caret_at(start.x, start.y + start.height * 0.5);
    assert_eq!(doc.text_caret(), Some(0));
    assert_eq!(doc.text_selection(), None, "a plain click never selects");
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

/// Every one of these has to no-op quietly rather than panic — a shortcut can fire in the gap
/// between a text session ending and the tool switching away from Text, and the engine has no
/// way to stop the shell from asking it to move a caret that is not there.
#[test]
fn every_selection_command_no_ops_with_no_active_session() {
    let mut doc = typed("hello world");
    doc.commit_text();
    assert_eq!(doc.text_caret(), None);
    assert_eq!(doc.text_range(), None);

    doc.text_step_caret(Step::Right, false);
    assert_eq!(doc.text_caret(), None);

    doc.text_set_caret_at(40.0, 100.0);
    assert_eq!(doc.text_caret(), None);

    doc.text_extend_to(40.0, 100.0);
    assert_eq!(doc.text_caret(), None);

    doc.text_select_word_at(40.0, 100.0);
    assert_eq!(doc.text_caret(), None);

    doc.text_select_paragraph_at(40.0, 100.0);
    assert_eq!(doc.text_caret(), None);

    assert!(!doc.text_select_all(), "nothing to select");
    assert_eq!(doc.text_caret(), None);

    assert!(doc.text_selection_rows().is_empty());
    assert_eq!(doc.text_caret_segment(), None);
    assert_eq!(doc.text_box(), None);
}
