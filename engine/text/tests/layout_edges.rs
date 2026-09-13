//! Caret and hit-testing behavior at the edges of a run: stepping off the first or last line,
//! clicking outside the laid-out block entirely, and moving the caret through text that is
//! still being composed by an IME. `text.rs` covers the interior; this covers the boundary the
//! interior tests never reach.

use calumma_text::{index_at_point, rasterize, step_index, Step, TextRun};

fn run(text: &str) -> TextRun {
    TextRun {
        text: text.to_string(),
        size: 32.0,
        origin: (100.0, 50.0),
        ..TextRun::default()
    }
    .clamped()
}

#[test]
fn stepping_up_from_the_first_line_clamps_instead_of_leaving_the_run() {
    let r = run("first\nsecond");
    assert_eq!(step_index(&r, 1, Step::Up), 0);
}

#[test]
fn stepping_down_from_the_last_line_clamps_instead_of_leaving_the_run() {
    let r = run("first\nsecond");
    assert_eq!(step_index(&r, 9, Step::Down), r.text.len());
}

/// `index_at_point` is what a click on the board maps to, and a click does not stop landing on
/// the text layer just because it missed every row — it lands above the first one or below the
/// last, and the caret has to go somewhere sane rather than nowhere.
#[test]
fn a_click_above_the_text_lands_at_the_start() {
    let r = run("hello world");
    assert_eq!(index_at_point(&r, 150.0, r.origin.1 - 1000.0), 0);
}

#[test]
fn a_click_below_the_text_lands_at_the_end() {
    let r = run("hello world");
    assert_eq!(index_at_point(&r, 150.0, r.origin.1 + 1000.0), r.text.len());
}

/// `TEXT_RASTER_MAX_SIDE` exists so a pathological run cannot demand an unbounded bitmap —
/// thousands of wrapped lines at a real size blow past it, and rasterizing has to refuse rather
/// than allocate however many gigabytes that would be.
#[test]
fn a_run_too_tall_to_rasterize_is_refused_rather_than_allocated() {
    let huge = TextRun {
        text: "a\n".repeat(4000),
        size: 40.0,
        ..TextRun::default()
    }
    .clamped();
    assert!(rasterize(&huge).is_none());
}
