//! `TextRun`'s own contracts: what a run *is*, before anything shapes or draws it.
//!
//! Layout and rasterizing are covered in `text.rs`. What lives here is the string handling
//! every one of those paths depends on — byte offsets that must land on char boundaries, and
//! an IME composition spliced in at the caret — because a slice off a boundary is a panic,
//! not a wrong pixel.

use calumma_text::{
    TextAlign, TextRun, TEXT_LINE_HEIGHT_DEFAULT, TEXT_LINE_HEIGHT_MAX, TEXT_LINE_HEIGHT_MIN,
    TEXT_SIZE_MAX, TEXT_SIZE_MIN,
};

/// Four bytes, one char: an index in the middle of it is not a place a string can be split.
const EMOJI: &str = "a🙂b";

fn run(text: &str) -> TextRun {
    TextRun {
        text: text.to_string(),
        ..TextRun::default()
    }
}

#[test]
fn clamp_index_walks_back_off_a_codepoint_it_landed_inside() {
    let r = run(EMOJI);
    assert_eq!(r.clamp_index(0), 0);
    assert_eq!(
        r.clamp_index(1),
        1,
        "the boundary before the emoji is valid"
    );
    for inside in 2..=4 {
        assert_eq!(
            r.clamp_index(inside),
            1,
            "byte {inside} is inside the emoji, so it walks back to its start"
        );
    }
    assert_eq!(r.clamp_index(5), 5, "the boundary after it is valid again");
}

#[test]
fn clamp_index_past_the_end_lands_on_the_end() {
    let r = run(EMOJI);
    assert_eq!(r.clamp_index(r.text.len()), r.text.len());
    assert_eq!(r.clamp_index(9_999), r.text.len());
    assert_eq!(run("").clamp_index(4), 0);
}

#[test]
fn a_run_with_no_composition_displays_its_text_unchanged() {
    let r = run("hello");
    assert_eq!(r.text.clone(), "hello");
    for i in 0..=r.text.len() {
        assert_eq!(r.clamp_index(i), i);
    }
}

#[test]
fn at_carries_the_origin_and_color_and_defaults_the_rest() {
    let r = TextRun::at((12.0, -4.0), [1, 2, 3, 4]);
    assert_eq!(r.origin, (12.0, -4.0));
    assert_eq!(r.color, [1, 2, 3, 4]);
    assert_eq!(r.align, TextAlign::Left);
    assert!(r.is_empty());
    assert_eq!(r.size, TextRun::default().size);
}

#[test]
fn align_round_trips_through_its_wire_value_and_refuses_anything_else() {
    for align in [TextAlign::Left, TextAlign::Center, TextAlign::Right] {
        assert_eq!(TextAlign::from_u32(align.as_u32()), Some(align));
    }
    assert_eq!(TextAlign::from_u32(3), None);
    assert_eq!(TextAlign::from_u32(u32::MAX), None);
    assert_eq!(TextAlign::default(), TextAlign::Left);
}

#[test]
fn line_spacing_is_the_size_times_the_multiplier() {
    let r = TextRun {
        size: 20.0,
        line_height: 1.5,
        ..TextRun::default()
    };
    assert_eq!(r.line_spacing(), 30.0);
}

/// Every knob a malformed run can carry out of range, checked at both ends. The values come
/// off SQLite and off the shell, so neither end can be assumed sane.
#[test]
fn clamping_pulls_every_knob_back_into_range() {
    let tiny = TextRun {
        size: 0.0,
        line_height: 0.0,
        ..TextRun::default()
    }
    .clamped();
    assert_eq!(tiny.size, TEXT_SIZE_MIN);
    assert_eq!(tiny.line_height, TEXT_LINE_HEIGHT_MIN);

    let huge = TextRun {
        size: 9_000.0,
        line_height: 40.0,
        ..TextRun::default()
    }
    .clamped();
    assert_eq!(huge.size, TEXT_SIZE_MAX);
    assert_eq!(huge.line_height, TEXT_LINE_HEIGHT_MAX);

    let nan_height = TextRun {
        line_height: f32::NAN,
        ..TextRun::default()
    }
    .clamped();
    assert_eq!(nan_height.line_height, TEXT_LINE_HEIGHT_DEFAULT);
}

#[test]
fn clamping_drops_a_wrap_width_that_is_not_a_number_and_floors_a_narrow_one() {
    let broken = TextRun {
        wrap_width: Some(f32::NAN),
        ..TextRun::default()
    }
    .clamped();
    assert_eq!(broken.wrap_width, None, "no wrap beats a nonsense wrap");

    let narrow = TextRun {
        wrap_width: Some(-50.0),
        ..TextRun::default()
    }
    .clamped();
    assert_eq!(narrow.wrap_width, Some(16.0));

    let unset = TextRun {
        wrap_width: None,
        ..TextRun::default()
    }
    .clamped();
    assert_eq!(unset.wrap_width, None, "clamping does not invent a wrap");
}
