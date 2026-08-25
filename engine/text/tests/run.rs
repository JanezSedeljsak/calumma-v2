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
    assert_eq!(r.display_text(), "hello");
    for i in 0..=r.text.len() {
        assert_eq!(r.display_index(i), i);
    }
}

#[test]
fn a_composition_is_spliced_in_at_the_caret() {
    let r = TextRun {
        marked: "みょ".to_string(),
        marked_at: 2,
        ..run("abcd")
    };
    assert_eq!(r.display_text(), "abみょcd");
}

/// The caret has to stay on the same character once the composition is spliced in: offsets at
/// or before the composition are unmoved, offsets after it shift by its byte length.
#[test]
fn display_index_maps_the_caret_across_the_composition() {
    let marked = "xy";
    let r = TextRun {
        marked: marked.to_string(),
        marked_at: 2,
        ..run("abcd")
    };
    assert_eq!(r.display_index(0), 0);
    assert_eq!(r.display_index(2), 2, "at the splice point, unmoved");
    assert_eq!(r.display_index(3), 3 + marked.len());
    assert_eq!(r.display_index(4), 4 + marked.len());
    assert_eq!(
        r.display_index(999),
        r.text.len() + marked.len(),
        "an offset past the end clamps before it is mapped"
    );
}

/// A composition anchored mid-codepoint would slice a string in half. `display_text` clamps
/// the anchor rather than trusting it, so a stale `marked_at` cannot bring the app down.
#[test]
fn a_composition_anchored_inside_a_codepoint_does_not_split_it() {
    for at in 0..=6 {
        let r = TextRun {
            marked: "!".to_string(),
            marked_at: at,
            ..run(EMOJI)
        };
        let shown = r.display_text();
        assert!(shown.contains('!'), "at={at}");
        assert!(shown.contains('🙂'), "the emoji survives whole: at={at}");
        assert_eq!(shown.chars().count(), 4, "at={at}");
    }
}

#[test]
fn a_run_is_empty_only_when_it_has_neither_text_nor_composition() {
    assert!(run("").is_empty());
    assert!(!run("a").is_empty());
    assert!(!TextRun {
        marked: "a".to_string(),
        ..run("")
    }
    .is_empty());
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

#[test]
fn clamping_is_idempotent() {
    let once = TextRun {
        size: f32::NEG_INFINITY,
        line_height: f32::NAN,
        family: String::new(),
        origin: (f32::NAN, 3.0),
        wrap_width: Some(2.0),
        marked_at: 500,
        ..run(EMOJI)
    }
    .clamped();
    assert_eq!(once.clone().clamped(), once);
}

/// A clamped anchor has to land on a boundary, not merely inside the string — the same rule
/// `clamp_index` enforces, applied to the value that is persisted.
#[test]
fn clamping_moves_a_composition_anchor_onto_a_char_boundary() {
    let repaired = TextRun {
        marked_at: 3,
        ..run(EMOJI)
    }
    .clamped();
    assert_eq!(repaired.marked_at, 1);
    assert!(repaired.text.is_char_boundary(repaired.marked_at));
}
