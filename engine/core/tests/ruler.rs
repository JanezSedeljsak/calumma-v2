use calumma_core::limits::*;
use calumma_core::ruler::*;

#[test]
fn empty_when_zoom_or_viewport_is_zero() {
    assert!(ruler_ticks(0.0, 0.0, 800.0).is_empty());
    assert!(ruler_ticks(1.0, 0.0, 0.0).is_empty());
}

#[test]
fn covers_the_full_visible_span() {
    let ticks = ruler_ticks(1.0, 0.0, 800.0);
    let first = ticks.first().unwrap().doc;
    let last = ticks.last().unwrap().doc;
    assert!(first <= 0.0);
    assert!(last >= 800.0);
}

#[test]
fn zero_is_always_a_tick_when_origin_is_visible() {
    let ticks = ruler_ticks(1.0, 0.0, 800.0);
    assert!(ticks.iter().any(|t| t.doc == 0.0));
}

#[test]
fn minor_ticks_stay_at_least_the_spacing_floor_apart_on_screen() {
    for zoom in [0.05f32, 0.5, 1.0, 4.0, 32.0] {
        let ticks = ruler_ticks(zoom, 123.0, 900.0);
        for pair in ticks.windows(2) {
            let screen_gap = (pair[1].doc - pair[0].doc) * zoom;
            assert!(screen_gap + 1e-3 >= RULER_MIN_MINOR_SPACING_PX);
        }
    }
}

#[test]
fn major_ticks_stay_at_least_the_label_spacing_floor_apart_on_screen() {
    for zoom in [0.05f32, 0.5, 1.0, 4.0, 32.0] {
        let ticks = ruler_ticks(zoom, 0.0, 900.0);
        let majors: Vec<f32> = ticks.iter().filter(|t| t.major).map(|t| t.doc).collect();
        for pair in majors.windows(2) {
            let screen_gap = (pair[1] - pair[0]) * zoom;
            assert!(screen_gap + 1e-3 >= RULER_MIN_MAJOR_SPACING_PX);
        }
    }
}

#[test]
fn ticks_stay_in_lockstep_with_pan_and_zoom() {
    let ticks = ruler_ticks(2.0, -150.0, 1000.0);
    for tick in &ticks {
        let screen = tick.doc * 2.0 + -150.0;
        assert!((-1.0..=1001.0).contains(&screen));
    }
}

#[test]
fn tick_count_is_bounded_regardless_of_zoom() {
    for zoom in [0.001f32, 0.02, 1.0, 64.0] {
        let ticks = ruler_ticks(zoom, 0.0, 2000.0);
        assert!(ticks.len() < 2000);
    }
}
