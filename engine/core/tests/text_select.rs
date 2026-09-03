//! Selection inside a live text session, and what a styled range does to the run.
//!
//! The three things worth guarding: the anchor moves only when a motion says to extend, an
//! edit with a range selected replaces it, and a style knob turned with a selection writes a
//! span instead of the whole block.

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

fn drag(doc: &mut Document, x: f32, y: f32) {
    let (sx, sy) = doc.camera.to_screen(x, y);
    doc.pointer_move(sx, sy);
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

fn run(doc: &Document) -> &TextRun {
    doc.active_text_run().expect("a run")
}

#[test]
fn a_fresh_session_has_a_caret_and_no_selection() {
    let doc = typed("hello world");
    assert_eq!(doc.text_caret(), Some(11));
    assert_eq!(doc.text_selection(), None);
    assert!(doc.text_selection_rows().is_empty());
}

#[test]
fn a_shift_extended_step_leaves_the_anchor_and_a_plain_one_does_not() {
    let mut doc = typed("hello");
    doc.text_step_caret(Step::Left, true);
    doc.text_step_caret(Step::Left, true);
    assert_eq!(doc.text_selection(), Some((3, 5)));
    assert!(!doc.text_selection_rows().is_empty(), "and it is drawn");

    doc.text_step_caret(Step::Left, false);
    assert_eq!(doc.text_selection(), None, "a plain step cancels it");
}

/// The arrow key that cancels a selection must not also step past it — it collapses to the end
/// it points at, which is what every text editor does.
#[test]
fn a_plain_arrow_collapses_a_selection_to_the_end_it_points_at() {
    let mut doc = typed("hello");
    doc.text_step_caret(Step::DocStart, false);
    doc.text_step_caret(Step::DocEnd, true);
    assert_eq!(doc.text_selection(), Some((0, 5)));

    doc.text_step_caret(Step::Left, false);
    assert_eq!(doc.text_caret(), Some(0));

    doc.text_step_caret(Step::DocStart, false);
    doc.text_step_caret(Step::DocEnd, true);
    doc.text_step_caret(Step::Right, false);
    assert_eq!(doc.text_caret(), Some(5));
}

#[test]
fn a_word_step_crosses_a_whole_word() {
    let mut doc = typed("hello world");
    doc.text_step_caret(Step::WordLeft, false);
    assert_eq!(doc.text_caret(), Some(6));
    doc.text_step_caret(Step::WordLeft, true);
    assert_eq!(doc.text_selection(), Some((0, 6)));
}

#[test]
fn typing_replaces_the_selection() {
    let mut doc = typed("hello world");
    doc.text_step_caret(Step::WordLeft, true);
    doc.text_insert("there");
    assert_eq!(run(&doc).text, "hello there");
    assert_eq!(doc.text_caret(), Some(11));
    assert_eq!(doc.text_selection(), None);
}

#[test]
fn backspace_and_delete_take_the_selection_rather_than_a_character() {
    let mut doc = typed("hello world");
    doc.text_step_caret(Step::WordLeft, true);
    doc.text_backspace();
    assert_eq!(run(&doc).text, "hello ");

    doc.text_step_caret(Step::DocStart, false);
    doc.text_step_caret(Step::WordRight, true);
    doc.text_delete_forward();
    assert_eq!(run(&doc).text, " ");
}

#[test]
fn select_all_while_typing_takes_the_text_and_not_the_canvas() {
    let mut doc = typed("hello world");
    doc.select_all();
    assert_eq!(doc.text_selection(), Some((0, 11)));
    assert!(
        doc.selection.is_none(),
        "the canvas selection was left alone"
    );
    assert!(doc.text_editing(), "and the session is still open");
}

#[test]
fn select_all_with_no_session_still_selects_the_canvas() {
    let mut doc = board();
    doc.select_all();
    assert!(doc.selection.is_some());
}

#[test]
fn a_double_click_selects_the_word_under_it() {
    let mut doc = typed("hello world");
    let (x, y) = (run(&doc).origin.0 + 8.0, run(&doc).origin.1 + 8.0);
    doc.text_select_word_at(x, y);
    assert_eq!(doc.text_selection(), Some((0, 5)));
}

#[test]
fn a_triple_click_selects_the_paragraph_under_it() {
    let mut doc = typed("one two\nthree");
    let run_origin = run(&doc).origin;
    doc.text_select_paragraph_at(run_origin.0 + 8.0, run_origin.1 + 8.0);
    assert_eq!(doc.text_selection(), Some((0, 7)));
}

/// Pointer-down anchors, pointer-move extends: the whole of drag-select, and the reason the
/// board forwards a Text-tool drag at all.
#[test]
fn dragging_inside_existing_text_sweeps_a_selection() {
    let mut doc = typed("hello world");
    let box_rect = doc.text_box().expect("a text box");
    doc.commit_text();

    press(&mut doc, box_rect.0 + 2.0, box_rect.1 + 8.0);
    assert_eq!(doc.text_selection(), None, "the press only set a caret");
    drag(&mut doc, box_rect.2 - 2.0, box_rect.1 + 8.0);
    let swept = doc.text_selection().expect("a selection");
    assert!(swept.1 > swept.0);
    release(&mut doc, box_rect.2 - 2.0, box_rect.1 + 8.0);
    assert_eq!(
        doc.text_selection(),
        Some(swept),
        "releasing keeps what was swept"
    );
}

#[test]
fn bold_with_a_selection_writes_a_span_and_leaves_the_block_alone() {
    let mut doc = typed("hello world");
    doc.text_step_caret(Step::WordLeft, true);
    doc.set_text_bold(true);
    let run = run(&doc);
    assert!(!run.bold, "the block itself did not turn bold");
    assert_eq!(run.spans.len(), 1);
    assert_eq!((run.spans[0].start, run.spans[0].end), (6, 11));
    assert!(run.style_at(8).bold);
    assert!(!run.style_at(2).bold);
}

#[test]
fn a_knob_turned_with_nothing_selected_still_takes_the_whole_block() {
    let mut doc = typed("hello world");
    doc.text_step_caret(Step::WordLeft, true);
    doc.set_text_bold(true);
    doc.text_step_caret(Step::DocEnd, false);
    doc.set_text_bold(false);
    let run = run(&doc);
    assert!(!run.bold);
    assert!(
        run.spans.is_empty(),
        "a leftover span would read as the setting having failed"
    );
}

#[test]
fn the_shell_reads_back_the_style_of_the_selection() {
    let mut doc = typed("hello world");
    doc.text_step_caret(Step::WordLeft, true);
    doc.set_text_size(96.0);
    assert_eq!(doc.active_text_style().size, 96.0);

    doc.text_step_caret(Step::DocStart, false);
    assert_eq!(
        doc.active_text_style().size,
        48.0,
        "the caret in the unstyled half reports the block's size"
    );
}

#[test]
fn a_styled_range_re_rasterizes_and_survives_the_session() {
    let mut doc = typed("hello world");
    let layer = doc.active_layer;
    doc.select_all();
    doc.set_text_size(96.0);
    doc.commit_text();
    let run = doc.layers[layer].run().expect("the run");
    assert_eq!(run.style_at(0).size, 96.0);
    assert!(doc.layers[layer]
        .tiles()
        .is_some_and(|g| g.coords().count() > 0));
}

/// A style change lands inside the typing session, so one `⌘Z` puts the whole session back —
/// spans included, because the run is what the project stores.
#[test]
fn undo_puts_the_spans_back_with_the_session() {
    let mut doc = typed("hello world");
    let layer = doc.active_layer;
    doc.select_all();
    doc.set_text_bold(true);
    doc.commit_text();
    assert!(doc.layers[layer].run().expect("run").style_at(0).bold);

    doc.undo();
    let layers = doc.layers.len();
    assert!(
        doc.layers
            .get(layer)
            .and_then(|l| l.run())
            .is_none_or(|run| run.text.is_empty()),
        "the whole session went, all {layers} layers considered"
    );
}

#[test]
fn a_selection_never_lands_inside_a_codepoint() {
    let mut doc = typed("a🙂b");
    doc.select_all();
    let (start, end) = doc.text_selection().expect("a selection");
    let run = run(&doc);
    assert!(run.text.is_char_boundary(start));
    assert!(run.text.is_char_boundary(end));
    assert_eq!(end, run.text.len());
}

#[test]
fn a_composition_replaces_the_selection_before_it_is_shown() {
    let mut doc = typed("hello world");
    doc.text_step_caret(Step::WordLeft, true);
    doc.text_set_marked("ん");
    let run = run(&doc);
    assert_eq!(run.text, "hello ", "the selected word went first");
    assert_eq!(run.display_text(), "hello ん");
    assert_eq!(doc.text_selection(), None);
}

/// Shift-click extends rather than re-anchoring, which means it is the one Text-tool press
/// that must not commit the session it lands in.
#[test]
fn a_shift_click_extends_the_open_session() {
    let mut doc = typed("hello world");
    let layer = doc.active_layer;
    doc.text_step_caret(Step::DocStart, false);
    let (x0, y0, x1, _) = doc.text_box().expect("a box");

    doc.set_shift_held(true);
    press(&mut doc, x1 - 2.0, y0 + 8.0);
    release(&mut doc, x1 - 2.0, y0 + 8.0);
    assert_eq!(
        doc.text_edit_layer(),
        Some(layer),
        "the session stayed open"
    );
    let extended = doc.text_selection().expect("a selection");
    assert_eq!(extended.0, 0, "the anchor stayed at the start");
    assert!(extended.1 > 0);

    doc.set_shift_held(false);
    press(&mut doc, x0 + 2.0, y0 + 8.0);
    release(&mut doc, x0 + 2.0, y0 + 8.0);
    assert_eq!(doc.text_selection(), None, "a plain click re-anchors");
}

/// Undo and redo both carry the run, not only the pixels — a project stores the run, so a
/// redone style change that came back as bare tiles would be gone on the next open.
#[test]
fn redo_brings_the_spans_back_again() {
    let mut doc = typed("hello world");
    let layer = doc.active_layer;
    doc.select_all();
    doc.set_text_bold(true);
    doc.commit_text();

    doc.undo();
    doc.redo();
    let run = doc.layers[layer].run().expect("the run");
    assert_eq!(run.text, "hello world");
    assert!(run.style_at(0).bold);
}

#[test]
fn deleting_all_the_text_takes_every_span_with_it() {
    let mut doc = typed("hello world");
    doc.select_all();
    doc.set_text_bold(true);
    assert!(!run(&doc).spans.is_empty());

    doc.select_all();
    doc.text_backspace();
    let run = run(&doc);
    assert!(run.text.is_empty());
    assert!(run.spans.is_empty(), "a span over nothing is not a span");
}

/// The pending-input rule end to end: style a word, put the caret at its far edge, and the next
/// keystroke joins it.
#[test]
fn typing_at_the_end_of_a_styled_range_continues_it() {
    let mut doc = typed("hello world");
    doc.text_step_caret(Step::WordLeft, true);
    doc.set_text_bold(true);
    doc.text_step_caret(Step::DocEnd, false);
    doc.text_insert("!");
    let run = run(&doc);
    assert_eq!(run.text, "hello world!");
    assert!(run.style_at(11).bold, "the ! took the bold word's style");
    assert!(!run.style_at(2).bold);
}

/// `text_style` is the template the next layer starts from, and it carries fields, never
/// ranges — byte offsets into a string that layer does not have would be nonsense.
#[test]
fn the_next_text_layer_starts_unstyled() {
    let mut doc = typed("hello world");
    doc.select_all();
    doc.set_text_bold(true);
    doc.commit_text();

    press(&mut doc, 40.0, 300.0);
    release(&mut doc, 40.0, 300.0);
    doc.text_insert("second");
    let run = run(&doc);
    assert!(run.spans.is_empty(), "spans do not travel between layers");
    assert!(run.bold, "but the run-level style does");
}

#[test]
fn select_all_on_an_empty_run_selects_nothing() {
    let mut doc = board();
    press(&mut doc, 40.0, 100.0);
    release(&mut doc, 40.0, 100.0);
    assert!(doc.text_select_all(), "there is a session to act on");
    assert_eq!(doc.text_selection(), None);
    assert!(doc.text_selection_rows().is_empty());
}

#[test]
fn the_style_readout_falls_back_to_the_documents_own_defaults() {
    let mut doc = board();
    doc.text_style.size = 72.0;
    doc.text_style.bold = true;
    let style = doc.active_text_style();
    assert_eq!(style.size, 72.0);
    assert!(style.bold);
}

/// Rasterizing is the one-way door out of a text layer. It keeps the pixels the spans produced
/// and drops the run that produced them.
#[test]
fn rasterizing_keeps_the_styled_pixels_and_drops_the_run() {
    let mut doc = typed("hello");
    let layer = doc.active_layer;
    doc.select_all();
    doc.set_text_size(96.0);
    doc.commit_text();
    let inked = doc.layers[layer].content_bounds().expect("glyphs");

    assert!(doc.rasterize_layer(layer));
    assert!(doc.layers[layer].run().is_none(), "the run is gone");
    assert_eq!(
        doc.layers[layer].content_bounds().expect("pixels"),
        inked,
        "the pixels the spans produced stayed exactly where they were"
    );
}
