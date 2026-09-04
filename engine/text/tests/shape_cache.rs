//! `buffer::ensure_shaped` caches the last shaped buffer so a caret move, a selection drag or a
//! hit-test does not re-shape a run that has not changed. Caching is meant to be invisible from
//! outside the crate — these tests exist to catch the one way it would not be: a stale shape
//! answered back after the run it describes has actually changed, or one run's shape leaking
//! into another's answer through the single cache slot.

use calumma_text::{caret_rect, measure, SpanStyle, TextRun};

fn run(text: &str) -> TextRun {
    TextRun {
        text: text.to_string(),
        size: 32.0,
        ..TextRun::default()
    }
    .clamped()
}

#[test]
fn a_text_change_is_measured_fresh_not_from_the_cached_shape() {
    let r = run("a");
    let narrow = measure(&r).0;

    let mut r = r;
    r.text = "a much longer piece of text than the one just measured".to_string();
    let wide = measure(&r).0;

    assert!(
        wide > narrow,
        "measuring after a text change must not reuse the shape cached for the old text: \
         {narrow} then {wide}"
    );
}

#[test]
fn a_wrap_width_change_is_measured_fresh() {
    let r = run("one two three four five six seven eight");
    let (_, unwrapped_height) = measure(&r);

    let mut r = r;
    r.wrap_width = Some(120.0);
    let r = r.clamped();
    let (_, wrapped_height) = measure(&r);

    assert!(
        wrapped_height > unwrapped_height,
        "wrapping has to actually apply, not answer with the cached unwrapped shape: \
         {unwrapped_height} then {wrapped_height}"
    );
}

/// A span carries its own size, which changes layout height the run's own fields do not
/// describe — `ShapeKey` has to compare `spans`, not just the run-level fields, or a styled
/// run would measure as if the style were never applied.
#[test]
fn a_span_style_change_is_measured_fresh() {
    let r = run("hello");
    let (_, before) = measure(&r);

    let mut r = r;
    r.apply_style(
        0,
        5,
        &SpanStyle {
            size: Some(96.0),
            ..SpanStyle::default()
        },
    );
    let (_, after) = measure(&r);

    assert!(
        after > before,
        "a span enlarging the whole run's only text must grow the measured height: \
         {before} then {after}"
    );
}

/// The cache is one slot, not one per run, because `TextEngine` is a single process-wide
/// instance. Alternating between two runs has to keep answering each one correctly rather than
/// the previous caller's shape leaking through.
#[test]
fn alternating_between_two_runs_never_answers_with_the_others_shape() {
    let short = run("hi");
    let long = run("a substantially longer run to measure against the short one");

    for _ in 0..4 {
        let short_width = measure(&short).0;
        let long_width = measure(&long).0;
        assert!(
            long_width > short_width,
            "each call has to answer for its own run: short={short_width} long={long_width}"
        );
    }
}

#[test]
fn caret_rect_reflects_a_text_change_between_calls() {
    let r = run("abc");
    let end_of_short = caret_rect(&r, 3).x;

    let mut r = r;
    r.text = "abcdef".to_string();
    let end_of_long = caret_rect(&r, 6).x;

    assert!(
        end_of_long > end_of_short,
        "the caret at the end of a longer string must sit further along, not where the \
         cached shorter shape left it"
    );
}
