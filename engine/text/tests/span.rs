//! Style spans: what an override covers, and what happens to it when the text underneath
//! moves.
//!
//! The boundary arithmetic is the whole risk here. A span is a byte range into a string that
//! is being edited in the middle of, so every insert and delete moves it — and a boundary
//! that drifts is a silent formatting corruption, never a crash.

use calumma_text::{SpanStyle, StyleSpan, TextRun};

const EMOJI: &str = "a🙂b";

fn run(text: &str) -> TextRun {
    TextRun {
        text: text.to_string(),
        ..TextRun::default()
    }
}

fn bold() -> SpanStyle {
    SpanStyle {
        bold: Some(true),
        ..SpanStyle::default()
    }
}

fn red() -> SpanStyle {
    SpanStyle {
        color: Some([255, 0, 0, 255]),
        ..SpanStyle::default()
    }
}

#[test]
fn an_empty_span_list_is_the_runs_own_style_everywhere() {
    let r = run("hello");
    assert!(r.spans.is_empty());
    let style = r.style_at(2);
    assert_eq!(style.bold, r.bold);
    assert_eq!(style.size, r.size);
    assert_eq!(style.family, r.family);
    assert_eq!(style.color, r.color);
}

#[test]
fn a_span_answers_only_inside_its_range() {
    let mut r = run("hello world");
    r.apply_style(6, 11, &bold());
    assert!(!r.style_at(0).bold, "before the span the run answers");
    assert!(r.style_at(8).bold, "inside it the span does");
    assert_eq!(
        r.style_at(8).size,
        r.size,
        "a span states only what it overrides"
    );
}

#[test]
fn overlapping_applies_merge_rather_than_fight() {
    let mut r = run("hello world");
    r.apply_style(0, 11, &bold());
    r.apply_style(6, 11, &red());
    assert!(r.style_at(2).bold);
    assert_eq!(
        r.style_at(2).color,
        r.color,
        "the first half stays uncoloured"
    );
    assert!(r.style_at(8).bold, "bold survives underneath the colour");
    assert_eq!(r.style_at(8).color, [255, 0, 0, 255]);
}

#[test]
fn neighbours_that_say_the_same_thing_become_one_span() {
    let mut r = run("hello world");
    r.apply_style(0, 5, &bold());
    r.apply_style(5, 11, &bold());
    assert_eq!(r.spans.len(), 1);
    assert_eq!(r.spans[0].start, 0);
    assert_eq!(r.spans[0].end, 11);
}

#[test]
fn typing_at_a_spans_end_extends_it() {
    let mut r = run("ab");
    r.apply_style(0, 1, &bold());
    r.replace_range(1, 1, "X");
    assert_eq!(r.text, "aXb");
    assert_eq!((r.spans[0].start, r.spans[0].end), (0, 2));
    assert!(r.style_at(0).bold, "the a it was applied to");
    assert!(r.style_at(1).bold, "the X typed at its end took its style");
    assert!(
        !r.style_at(3).bold,
        "past the b the run's own style is back"
    );
}

/// The pending-input rule, stated on its own because it is the one place `style_at` is not
/// simply "the character here": a caret parked on a span's far edge reads that span, so the
/// next keystroke continues it — the same edge `shift` moves.
#[test]
fn a_caret_on_a_spans_far_edge_reads_that_span() {
    let mut r = run("ab");
    r.apply_style(0, 1, &bold());
    assert!(r.style_at(1).bold);
    assert!(!r.style_at(2).bold);
}

#[test]
fn typing_before_a_span_pushes_it_along_without_joining_it() {
    let mut r = run("ab");
    r.apply_style(1, 2, &bold());
    r.replace_range(1, 1, "X");
    assert_eq!(r.text, "aXb");
    assert_eq!(r.spans.len(), 1);
    assert_eq!((r.spans[0].start, r.spans[0].end), (2, 3));
    assert!(!r.style_at(1).bold, "the inserted X did not become bold");
    assert!(r.style_at(2).bold, "the b it was applied to still is");
}

#[test]
fn deleting_inside_a_span_shrinks_it_and_deleting_all_of_it_drops_it() {
    let mut r = run("hello world");
    r.apply_style(6, 11, &bold());
    r.replace_range(8, 10, "");
    assert_eq!(r.text, "hello wod");
    assert_eq!((r.spans[0].start, r.spans[0].end), (6, 9));

    r.replace_range(6, 9, "");
    assert_eq!(r.text, "hello ");
    assert!(r.spans.is_empty(), "a span with nothing left under it goes");
}

#[test]
fn a_replacement_that_straddles_a_span_keeps_only_the_surviving_half() {
    let mut r = run("abcdef");
    r.apply_style(2, 5, &bold());
    r.replace_range(3, 6, "Z");
    assert_eq!(r.text, "abcZ");
    assert_eq!(
        (r.spans[0].start, r.spans[0].end),
        (2, 3),
        "only the c the replacement did not reach is left bold"
    );
}

#[test]
fn span_boundaries_never_land_inside_a_codepoint() {
    let mut r = run(EMOJI);
    r.apply_style(2, 4, &bold());
    for span in &r.spans {
        assert!(r.text.is_char_boundary(span.start));
        assert!(r.text.is_char_boundary(span.end));
    }
}

#[test]
fn a_span_past_the_end_of_the_text_is_clipped_away() {
    let mut r = run("hi");
    r.spans = vec![StyleSpan {
        start: 1,
        end: 99,
        style: bold(),
    }];
    r = r.clamped();
    assert_eq!((r.spans[0].start, r.spans[0].end), (1, 2));
}

#[test]
fn clearing_an_override_takes_that_field_and_leaves_the_others() {
    let mut r = run("hello world");
    r.apply_style(0, 5, &bold());
    r.apply_style(0, 5, &red());
    r.clear_span_overrides(&bold());
    assert!(!r.style_at(2).bold, "bold is gone");
    assert_eq!(r.style_at(2).color, [255, 0, 0, 255], "the colour stayed");

    r.clear_span_overrides(&red());
    assert!(
        r.spans.is_empty(),
        "a span left stating nothing is not a span"
    );
}

#[test]
fn an_ime_composition_sits_inside_the_span_that_holds_the_caret() {
    let mut r = run("ab");
    r.apply_style(0, 1, &bold());
    r.marked = "ん".to_string();
    r.marked_at = 1;
    let display = r.display_spans();
    assert_eq!(display.len(), 1);
    assert_eq!(
        (display[0].start, display[0].end),
        (0, 1 + "ん".len()),
        "the composition takes the style of the span it was typed at the end of"
    );
}

#[test]
fn a_composition_before_a_span_does_not_adopt_it() {
    let mut r = run("ab");
    r.apply_style(1, 2, &bold());
    r.marked = "ん".to_string();
    r.marked_at = 1;
    let display = r.display_spans();
    let shift = "ん".len();
    assert_eq!((display[0].start, display[0].end), (1 + shift, 2 + shift));
}

#[test]
fn applying_nothing_over_a_range_leaves_the_run_alone() {
    let mut r = run("hello");
    r.apply_style(0, 5, &SpanStyle::default());
    assert!(
        r.spans.is_empty(),
        "a style that states nothing is not a span"
    );
}

#[test]
fn applying_a_style_over_an_empty_range_leaves_the_run_alone() {
    let mut r = run("hello");
    r.apply_style(2, 2, &bold());
    r.apply_style(4, 1, &bold());
    assert!(r.spans.is_empty());
}

/// `bold`/`color` are the two fields every other test in this file reaches for; `family` and
/// `italic` and `size` get exactly the same overlay treatment in `SpanStyle::overlay` and the
/// same per-field handling in `TextRun::style_at`/`clear_span_overrides`, but nothing here had
/// exercised them on their own before this.
#[test]
fn family_italic_and_size_overlay_independently_of_bold_and_colour() {
    let mut r = run("hello world");
    r.apply_style(
        0,
        11,
        &SpanStyle {
            family: Some("Menlo".to_string()),
            italic: Some(true),
            size: Some(20.0),
            ..SpanStyle::default()
        },
    );
    let style = r.style_at(5);
    assert_eq!(style.family, "Menlo");
    assert!(style.italic);
    assert_eq!(style.size, 20.0);
    assert!(
        !style.bold,
        "bold was never stated, so the run's own answer stands"
    );
}

#[test]
fn clearing_family_and_italic_overrides_leaves_size_untouched() {
    let mut r = run("hello world");
    r.apply_style(
        0,
        5,
        &SpanStyle {
            family: Some("Menlo".to_string()),
            italic: Some(true),
            size: Some(20.0),
            ..SpanStyle::default()
        },
    );
    r.clear_span_overrides(&SpanStyle {
        family: Some(String::new()),
        italic: Some(false),
        ..SpanStyle::default()
    });
    let style = r.style_at(2);
    assert_eq!(style.family, r.family, "family override is gone");
    assert_eq!(style.italic, r.italic, "italic override is gone");
    assert_eq!(style.size, 20.0, "size was never asked to clear");
}

/// `SpanStyle::clamped` (run through `normalize` on every mutation) repairs the two fields a
/// caller could hand it garbage for: a non-finite size falls back to the same default a run's
/// own `size` field falls back to, and a family that is only whitespace is dropped rather than
/// kept as a span nobody could ever match against a real font name.
#[test]
fn a_non_finite_span_size_falls_back_to_the_text_default() {
    let mut r = run("hello world");
    r.apply_style(
        0,
        5,
        &SpanStyle {
            size: Some(f32::NAN),
            ..SpanStyle::default()
        },
    );
    assert_eq!(r.spans[0].style.size, Some(48.0));
}

#[test]
fn a_whitespace_only_span_family_leaves_no_span_behind() {
    let mut r = run("hello world");
    r.apply_style(
        0,
        5,
        &SpanStyle {
            family: Some("   ".to_string()),
            ..SpanStyle::default()
        },
    );
    assert!(
        r.spans.is_empty(),
        "a family that trims to nothing states nothing, so the span it would have been is gone"
    );
}

/// Two spans with a gap between them, then one style applied across all of it: the parts under
/// the existing spans merge with them, and the gap — covered by neither — becomes a brand new
/// span carrying only what was just applied. `span::apply`'s gap-filling path, never exercised
/// by an overlapping-pair test where the range covers every byte in between.
#[test]
fn applying_a_style_across_a_gap_between_two_spans_fills_the_gap_with_a_fresh_span() {
    let mut r = run("abcdefghij");
    r.apply_style(0, 2, &bold());
    r.apply_style(5, 7, &bold());
    r.apply_style(0, 7, &red());

    assert_eq!(
        r.spans.len(),
        3,
        "left span, gap span, right span: {:?}",
        r.spans
    );
    let gap = &r.spans[1];
    assert_eq!((gap.start, gap.end), (2, 5));
    assert!(!gap.style.bold.unwrap_or(false), "the gap was never bold");
    assert_eq!(gap.style.color, Some([255, 0, 0, 255]));
    assert_eq!(
        r.spans[0].style.color,
        Some([255, 0, 0, 255]),
        "left span picked up the colour too"
    );
    assert_eq!(
        r.spans[2].style.color,
        Some([255, 0, 0, 255]),
        "right span picked up the colour too"
    );
}

/// A run that carries spans and a run that does not have to shape identically where the spans
/// say nothing new — otherwise adding one would move the text that is not in it.
#[test]
fn a_span_restating_the_runs_own_style_changes_no_measurement() {
    use calumma_text::measure;
    let plain = run("hello world");
    let mut spanned = run("hello world");
    spanned.apply_style(
        0,
        5,
        &SpanStyle {
            bold: Some(plain.bold),
            italic: Some(plain.italic),
            size: Some(plain.size),
            ..SpanStyle::default()
        },
    );
    let (pw, ph) = measure(&plain);
    let (sw, sh) = measure(&spanned);
    assert!((pw - sw).abs() < 0.01, "{pw} vs {sw}");
    assert!((ph - sh).abs() < 0.01, "{ph} vs {sh}");
}
