use calumma_core::limits::*;
use calumma_core::size_curve::*;
use calumma_core::{TEXT_SIZE_MAX, TEXT_SIZE_MIN};

#[test]
fn the_ends_of_the_travel_are_the_ends_of_the_range() {
    assert_eq!(brush_size_from_unit(0.0), BRUSH_SIZE_MIN);
    assert_eq!(brush_size_from_unit(1.0), BRUSH_SIZE_MAX);
    assert_eq!(text_size_from_unit(0.0), TEXT_SIZE_MIN);
    assert_eq!(text_size_from_unit(1.0), TEXT_SIZE_MAX);
}

/// The slider only ever produces whole pixels, so the number the panel prints is the number
/// the field types back and the thumb does not drift off the value it was given.
#[test]
fn the_slider_lands_on_whole_pixels() {
    for step in 0..=100 {
        let size = brush_size_from_unit(step as f32 / 100.0);
        assert_eq!(size, size.round(), "{size} is not a whole pixel");
    }
    assert_eq!(text_size_from_unit(0.37), text_size_from_unit(0.37).round());
}

#[test]
fn unit_and_size_round_trip() {
    for size in [
        BRUSH_SIZE_MIN,
        9.0,
        12.0,
        96.0,
        250.0,
        999.0,
        BRUSH_SIZE_MAX,
    ] {
        let back = brush_size_from_unit(brush_size_unit(size));
        assert!((back - size).abs() < 0.05, "{size} came back as {back}");
    }
}

/// Never backwards. Not *strictly* increasing at the bottom of the track — rounding puts the
/// first few percent of travel all on `BRUSH_SIZE_MIN`, which is the price of whole-pixel sizes
/// and is what the key steps are for down there.
#[test]
fn the_curve_never_goes_backwards() {
    let mut previous = f32::MIN;
    for step in 0..=1000 {
        let size = brush_size_from_unit(step as f32 / 1000.0);
        assert!(size >= previous, "{size} came after {previous}");
        previous = size;
    }
    assert_eq!(previous, BRUSH_SIZE_MAX);
}

/// The whole point of the exponent: the sizes a pen actually uses have to be reachable with
/// the first half of a 96pt-wide slider, not the first tenth of it.
#[test]
fn half_the_travel_stays_in_the_low_quarter_of_the_range() {
    let half = brush_size_from_unit(0.5);
    let quarter_of_range = BRUSH_SIZE_MIN + (BRUSH_SIZE_MAX - BRUSH_SIZE_MIN) * 0.25;
    assert!(
        (half - quarter_of_range).abs() <= 0.5,
        "half the travel is {half}"
    );
    // A tenth of the track is still a fine brush — nearer the floor than the halfway size.
    assert!(brush_size_from_unit(0.1) < 24.0);
}

#[test]
fn out_of_range_input_is_clamped_not_extrapolated() {
    assert_eq!(brush_size_from_unit(-1.0), BRUSH_SIZE_MIN);
    assert_eq!(brush_size_from_unit(4.0), BRUSH_SIZE_MAX);
    assert_eq!(brush_size_unit(-50.0), 0.0);
    assert_eq!(brush_size_unit(9999.0), 1.0);
}

#[test]
fn a_degenerate_range_answers_instead_of_dividing_by_zero() {
    assert_eq!(size_from_unit(0.5, 8.0, 8.0), 8.0);
    assert_eq!(unit_from_size(8.0, 8.0, 8.0), 0.0);
}

#[test]
fn a_step_moves_a_fine_brush_by_a_whole_pixel() {
    // Ten percent of a fine brush rounds to nothing, so the step falls back to a whole pixel.
    assert_eq!(step_brush_size(BRUSH_SIZE_MIN, true), BRUSH_SIZE_MIN + 1.0);
    assert_eq!(step_brush_size(9.0, true), 10.0);
    assert_eq!(step_brush_size(10.0, false), 9.0);
    assert_eq!(
        step_brush_size(BRUSH_SIZE_MIN, false),
        BRUSH_SIZE_MIN,
        "the floor holds"
    );
}

/// A brush below the floor is not a smaller brush, it is not a brush — every way in clamps.
#[test]
fn nothing_lands_under_the_floor() {
    assert_eq!(brush_size_from_unit(0.0), BRUSH_SIZE_MIN);
    assert_eq!(step_brush_size(1.0, false), BRUSH_SIZE_MIN);
    assert_eq!(step_brush_size(0.0, true), BRUSH_SIZE_MIN + 1.0);
    assert_eq!(brush_size_unit(1.0), 0.0);
}

#[test]
fn a_step_grows_with_the_brush() {
    assert!(step_brush_size(500.0, true) - 500.0 > 1.0);
    assert!(step_brush_size(500.0, true) <= BRUSH_SIZE_MAX);
}

#[test]
fn stepping_stays_inside_the_range() {
    assert_eq!(step_brush_size(BRUSH_SIZE_MIN, false), BRUSH_SIZE_MIN);
    assert_eq!(step_brush_size(BRUSH_SIZE_MAX, true), BRUSH_SIZE_MAX);
    assert_eq!(step_brush_size(-10.0, false), BRUSH_SIZE_MIN);
    assert_eq!(step_brush_size(f32::MAX, true), BRUSH_SIZE_MAX);
}

/// Sixty presses is the budget the ratio was chosen for — a key you have to hold for a
/// thousand presses is not a control.
#[test]
fn the_whole_range_is_reachable_by_key_in_under_sixty_presses() {
    let mut size = BRUSH_SIZE_MIN;
    let mut presses = 0;
    while size < BRUSH_SIZE_MAX && presses < 1000 {
        size = step_brush_size(size, true);
        presses += 1;
    }
    assert_eq!(size, BRUSH_SIZE_MAX);
    assert!(presses < 60, "took {presses} presses");
}
