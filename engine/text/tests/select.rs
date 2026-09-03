//! Selection geometry and granularity, asked of the shaped layout.
//!
//! These are the answers a double-click, a triple-click and a highlight need. Everything here
//! goes through real shaping with real system fonts, so the assertions are about *structure*
//! — how many rows, which byte ranges, which row is taller — never about exact advances,
//! which are the font's business and differ per machine.

use calumma_text::{paragraph_range, selection_rects, word_range, SpanStyle, TextRun};

fn run(text: &str) -> TextRun {
    TextRun {
        text: text.to_string(),
        size: 32.0,
        ..TextRun::default()
    }
    .clamped()
}

#[test]
fn nothing_selected_draws_nothing() {
    let r = run("hello");
    assert!(selection_rects(&r, 2, 2).is_empty());
    assert!(
        selection_rects(&r, 4, 4).is_empty(),
        "an inverted-but-equal range is still empty"
    );
}

#[test]
fn a_range_inside_one_line_is_one_row() {
    let r = run("hello world");
    let rects = selection_rects(&r, 0, 5);
    assert_eq!(rects.len(), 1);
    assert!(rects[0].width > 0.0);
    assert!(rects[0].height > 0.0);
    assert_eq!(rects[0].y, r.origin.1, "the first row sits at the origin");
}

#[test]
fn a_wider_range_covers_more_ground_than_a_narrower_one() {
    let r = run("hello world");
    let short = selection_rects(&r, 0, 2)[0].width;
    let long = selection_rects(&r, 0, 8)[0].width;
    assert!(long > short, "{long} should exceed {short}");
}

#[test]
fn a_range_reversed_selects_the_same_thing() {
    let r = run("hello world");
    assert_eq!(selection_rects(&r, 2, 7), selection_rects(&r, 7, 2));
}

#[test]
fn a_range_across_a_newline_is_one_row_per_line() {
    let r = run("one\ntwo\nthree");
    let rects = selection_rects(&r, 0, r.text.len());
    assert_eq!(rects.len(), 3);
    assert!(
        rects[1].y > rects[0].y && rects[2].y > rects[1].y,
        "rows come back top to bottom"
    );
}

/// A selection that swallows a line break has to look like it did — the break has no glyph, so
/// the row it ends is drawn a little wider than its last letter.
#[test]
fn a_row_whose_break_is_selected_reaches_past_its_last_glyph() {
    let r = run("ab\ncd");
    let without = selection_rects(&r, 0, 2);
    let with_break = selection_rects(&r, 0, 3);
    assert_eq!(without.len(), 1);
    assert_eq!(with_break.len(), 1);
    assert!(with_break[0].width > without[0].width);
}

#[test]
fn a_selected_empty_line_still_gets_a_row() {
    let r = run("a\n\nb");
    let rects = selection_rects(&r, 0, r.text.len());
    assert_eq!(rects.len(), 3, "the blank line in the middle counts");
    assert!(rects[1].width > 0.0, "and is visible");
}

/// A wrapped paragraph is one `BufferLine` laid out as several rows, so the highlight has to
/// come from the layout and not from counting newlines.
#[test]
fn a_wrapped_paragraph_highlights_per_visual_row() {
    let mut r = run("wrapping words onto several separate rows entirely");
    r.wrap_width = Some(120.0);
    let rects = selection_rects(&r, 0, r.text.len());
    assert!(
        rects.len() > 1,
        "expected several rows, got {}",
        rects.len()
    );
    for pair in rects.windows(2) {
        assert!(pair[1].y > pair[0].y);
    }
}

/// A larger span makes its own row taller. The row height comes from the shaped metrics, so
/// this is the one place the highlight proves spans reached layout at all.
#[test]
fn a_row_holding_a_bigger_span_is_taller() {
    let plain = selection_rects(&run("hello"), 0, 5)[0].height;
    let mut r = run("hello");
    r.apply_style(
        0,
        5,
        &SpanStyle {
            size: Some(96.0),
            ..SpanStyle::default()
        },
    );
    let styled = selection_rects(&r, 0, 5)[0].height;
    assert!(styled > plain, "{styled} should exceed {plain}");
}

#[test]
fn a_double_click_inside_a_word_takes_the_whole_word() {
    let r = run("hello world");
    assert_eq!(word_range(&r, 2), (0, 5));
    assert_eq!(word_range(&r, 0), (0, 5));
    assert_eq!(word_range(&r, 8), (6, 11));
}

#[test]
fn a_double_click_just_past_a_word_takes_the_word_it_just_left() {
    let r = run("hello world");
    assert_eq!(
        word_range(&r, 5),
        (0, 5),
        "the caret sits after 'hello' and before a space"
    );
    assert_eq!(word_range(&r, 11), (6, 11), "and at the very end");
}

#[test]
fn a_double_click_in_whitespace_takes_the_whitespace() {
    let r = run("a   b");
    assert_eq!(word_range(&r, 2), (1, 4));
}

#[test]
fn punctuation_groups_with_punctuation() {
    let r = run("wait...really");
    assert_eq!(word_range(&r, 5), (4, 7));
}

#[test]
fn a_double_click_on_empty_text_selects_nothing() {
    assert_eq!(word_range(&run(""), 0), (0, 0));
}

#[test]
fn a_word_never_splits_a_codepoint() {
    let r = run("a🙂b c");
    let (start, end) = word_range(&r, 3);
    assert!(r.text.is_char_boundary(start));
    assert!(r.text.is_char_boundary(end));
}

#[test]
fn a_triple_click_takes_the_paragraph_and_not_the_break() {
    let r = run("one\ntwo\nthree");
    assert_eq!(paragraph_range(&r, 0), (0, 3));
    assert_eq!(paragraph_range(&r, 5), (4, 7));
    assert_eq!(paragraph_range(&r, 12), (8, 13));
}

/// The paragraph is deliberately *not* the visual row: on a wrapped block a triple-click takes
/// the whole thing, which is what `Step::LineStart` would not have given.
#[test]
fn a_triple_click_on_a_wrapped_paragraph_takes_all_of_it() {
    let mut r = run("wrapping words onto several separate rows entirely");
    r.wrap_width = Some(120.0);
    assert_eq!(paragraph_range(&r, 20), (0, r.text.len()));
}

/// Alignment moves the glyphs, and the highlight is measured off them — so a centred or
/// right-aligned row lands where its text does rather than at the left margin.
#[test]
fn a_centred_row_highlights_where_its_glyphs_are() {
    use calumma_text::TextAlign;
    let mut left = run("hi");
    left.wrap_width = Some(300.0);
    let mut centred = left.clone();
    centred.align = TextAlign::Center;
    let mut right = left.clone();
    right.align = TextAlign::Right;

    let l = selection_rects(&left, 0, 2)[0];
    let c = selection_rects(&centred, 0, 2)[0];
    let r = selection_rects(&right, 0, 2)[0];
    assert!(c.x > l.x, "centred starts further in than left");
    assert!(r.x > c.x, "and right further still");
    assert!((c.width - l.width).abs() < 1.0, "the same two letters");
    assert!(
        r.x + r.width <= 300.0 + right.origin.0 + 1.0,
        "inside the box"
    );
}

/// The rows are document-space geometry, so moving the run moves them by exactly the same
/// amount — the board draws them without a second offset of its own.
#[test]
fn moving_the_run_moves_its_highlight_with_it() {
    let here = run("hello");
    let mut there = here.clone();
    there.origin = (40.0, 25.0);
    let a = selection_rects(&here, 0, 5)[0];
    let b = selection_rects(&there, 0, 5)[0];
    assert!((b.x - a.x - 40.0).abs() < 0.01);
    assert!((b.y - a.y - 25.0).abs() < 0.01);
    assert!((b.width - a.width).abs() < 0.01);
}

#[test]
fn a_range_past_the_end_of_the_text_is_clamped_rather_than_panicking() {
    let r = run("hi");
    let rects = selection_rects(&r, 0, 999);
    assert_eq!(rects.len(), 1);
    assert!(rects[0].width > 0.0);
}
