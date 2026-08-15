use calumma_core::camera::*;
use calumma_core::limits::*;

fn cam(vw: f32, vh: f32) -> Camera {
    Camera {
        viewport_width: vw,
        viewport_height: vh,
        dpr: 2.0,
        ..Default::default()
    }
}

#[test]
fn fit_zoom_is_above_min_floor() {
    let mut c = cam(800.0, 600.0);
    c.fit(1920.0, 1080.0);
    let floor = c.min_zoom(1920.0, 1080.0);
    let fit = c.fit_zoom(1920.0, 1080.0);
    assert!(fit > floor);
    assert!((c.zoom - fit).abs() < 1e-5);
    c.zoom = floor * 0.5;
    c.clamp_to_board(1920.0, 1080.0);
    assert!((c.zoom - floor).abs() < 1e-5);
}

#[test]
fn min_zoom_fills_half_viewport() {
    let c = cam(1000.0, 800.0);
    let z = c.min_zoom(2000.0, 1000.0);
    let paper_w = 2000.0 * z;
    let paper_h = 1000.0 * z;
    let fill_w = paper_w / 1000.0;
    let fill_h = paper_h / 800.0;
    assert!((fill_w.max(fill_h) - MIN_ZOOM_FILL).abs() < 1e-4);
}

#[test]
fn max_zoom_is_ten_times_min_when_detail_allows() {
    let c = cam(2000.0, 2000.0);
    let min = c.min_zoom(3000.0, 3000.0);
    let max = c.max_zoom(3000.0, 3000.0);
    assert!((max - min * MAX_ZOOM_IN_FACTOR).abs() < 1e-3);
}

#[test]
fn max_zoom_respects_visible_doc_side() {
    let c = cam(800.0, 600.0);
    let max = c.max_zoom(4000.0, 4000.0);
    let visible = 600.0 / max;
    assert!(visible + 1e-3 >= MIN_VISIBLE_DOC_SIDE.min(4000.0));
}

#[test]
fn screen_doc_round_trip() {
    let mut c = cam(1000.0, 800.0);
    c.fit(2000.0, 1500.0);
    c.zoom_at(400.0, 300.0, c.zoom * 2.0, 2000.0, 1500.0);
    let (dx, dy) = c.to_doc(412.0, 288.0);
    let (sx, sy) = c.to_screen(dx, dy);
    assert!((sx - 412.0).abs() < 1e-3);
    assert!((sy - 288.0).abs() < 1e-3);
}

#[test]
fn fit_nearly_fills_the_viewport() {
    let mut c = cam(1000.0, 800.0);
    c.fit(2000.0, 1000.0);
    let filled = 2000.0 * c.zoom / 1000.0;
    assert!(filled > 0.95, "paper only filled {filled} of the viewport");
}

#[test]
fn zoom_unit_round_trips_through_the_log_scale() {
    let mut c = cam(1000.0, 800.0);
    c.fit(2000.0, 1500.0);
    for unit in [0.0, 0.25, 0.5, 0.9, 1.0] {
        let zoom = c.zoom_from_unit(unit, 2000.0, 1500.0);
        c.zoom_to_center(zoom, 2000.0, 1500.0);
        assert!((c.zoom_unit(2000.0, 1500.0) - unit).abs() < 1e-3);
    }
}

#[test]
fn step_zoom_stays_inside_the_camera_range() {
    let mut c = cam(1000.0, 800.0);
    c.fit(2000.0, 1500.0);
    for _ in 0..40 {
        c.step_zoom(true, 2000.0, 1500.0);
    }
    assert!((c.zoom - c.max_zoom(2000.0, 1500.0)).abs() < 1e-4);
    for _ in 0..80 {
        c.step_zoom(false, 2000.0, 1500.0);
    }
    assert!((c.zoom - c.min_zoom(2000.0, 1500.0)).abs() < 1e-4);
}

#[test]
fn fit_centers_the_paper() {
    let mut c = cam(1000.0, 800.0);
    c.fit(100.0, 100.0);
    let pw = 100.0 * c.zoom;
    let ph = 100.0 * c.zoom;
    assert!((c.pan_x - (1000.0 - pw) * 0.5).abs() < 1e-3);
    assert!((c.pan_y - (800.0 - ph) * 0.5).abs() < 1e-3);
}

#[test]
fn pan_moves_a_fitted_paper() {
    let mut c = cam(1000.0, 800.0);
    c.fit(2000.0, 1500.0);
    let before = (c.pan_x, c.pan_y);
    c.pan_by(40.0, -30.0, 2000.0, 1500.0);
    assert!((c.pan_x - (before.0 + 40.0)).abs() < 1e-3);
    assert!((c.pan_y - (before.1 - 30.0)).abs() < 1e-3);
}

#[test]
fn pan_keeps_part_of_the_paper_on_screen() {
    let mut c = cam(1000.0, 800.0);
    c.fit(2000.0, 1500.0);
    for _ in 0..200 {
        c.pan_by(500.0, 500.0, 2000.0, 1500.0);
    }
    let paper_w = 2000.0 * c.zoom;
    let paper_h = 1500.0 * c.zoom;
    let visible_w = (c.pan_x + paper_w).min(1000.0) - c.pan_x.max(0.0);
    let visible_h = (c.pan_y + paper_h).min(800.0) - c.pan_y.max(0.0);
    assert!(visible_w > 0.0 && visible_h > 0.0);
    assert!((visible_w - paper_w.min(1000.0) * PAN_KEEP_VISIBLE).abs() < 1e-2);
    assert!((visible_h - paper_h.min(800.0) * PAN_KEEP_VISIBLE).abs() < 1e-2);
}

#[test]
fn pan_bounds_are_never_inverted() {
    for (vw, vh, dw, dh, zoom) in [
        (1000.0, 800.0, 2000.0, 1500.0, 0.1),
        (1000.0, 800.0, 100.0, 100.0, 8.0),
        (300.0, 2000.0, 4000.0, 50.0, 1.0),
    ] {
        let mut c = cam(vw, vh);
        c.zoom = zoom;
        let (min_x, max_x, min_y, max_y) = c.pan_bounds(dw, dh);
        assert!(min_x <= max_x, "x bounds inverted for {vw}x{vh} {dw}x{dh}");
        assert!(min_y <= max_y, "y bounds inverted for {vw}x{vh} {dw}x{dh}");
    }
}

#[test]
fn scroll_pan_gain_is_one_at_fit_and_grows_as_you_zoom_out() {
    let mut c = cam(1000.0, 800.0);
    c.fit(2000.0, 1500.0);
    assert!((c.scroll_pan_gain(2000.0, 1500.0) - 1.0).abs() < 1e-4);

    c.zoom_to_center(c.min_zoom(2000.0, 1500.0), 2000.0, 1500.0);
    let zoomed_out = c.scroll_pan_gain(2000.0, 1500.0);
    assert!(
        zoomed_out > 1.5,
        "expected a real speed-up at min zoom, got {zoomed_out}"
    );
    assert!(zoomed_out <= SCROLL_PAN_MAX_GAIN);
}

#[test]
fn scroll_pan_gain_never_slows_panning_when_zoomed_in() {
    let mut c = cam(1000.0, 800.0);
    c.fit(2000.0, 1500.0);
    for _ in 0..20 {
        c.step_zoom(true, 2000.0, 1500.0);
    }
    assert!((c.scroll_pan_gain(2000.0, 1500.0) - 1.0).abs() < 1e-6);
}

#[test]
fn pan_by_scroll_moves_further_than_a_raw_pan_when_zoomed_out() {
    let doc = (2000.0, 1500.0);
    let mut scrolled = cam(1000.0, 800.0);
    scrolled.fit(doc.0, doc.1);
    scrolled.zoom_to_center(scrolled.min_zoom(doc.0, doc.1), doc.0, doc.1);
    let mut dragged = scrolled;

    scrolled.pan_by_scroll(30.0, 0.0, true, doc.0, doc.1);
    dragged.pan_by(30.0, 0.0, doc.0, doc.1);
    assert!(
        scrolled.pan_x > dragged.pan_x,
        "scroll {} should outrun drag {}",
        scrolled.pan_x,
        dragged.pan_x
    );
}

#[test]
fn pan_by_scroll_still_respects_the_slack_clamp() {
    let doc = (2000.0, 1500.0);
    let mut c = cam(1000.0, 800.0);
    c.fit(doc.0, doc.1);
    for _ in 0..200 {
        c.pan_by_scroll(500.0, 500.0, true, doc.0, doc.1);
    }
    let (_, max_x, _, max_y) = c.pan_bounds(doc.0, doc.1);
    assert!(c.pan_x <= max_x + 1e-3);
    assert!(c.pan_y <= max_y + 1e-3);
}

#[test]
fn a_wheel_line_pans_further_than_a_trackpad_pixel() {
    let doc = (2000.0, 1500.0);
    let mut wheel = cam(1000.0, 800.0);
    wheel.fit(doc.0, doc.1);
    let mut trackpad = wheel;
    let start = wheel.pan_y;

    wheel.pan_by_scroll(0.0, 3.0, false, doc.0, doc.1);
    trackpad.pan_by_scroll(0.0, 3.0, true, doc.0, doc.1);
    let (by_line, by_pixel) = (wheel.pan_y - start, trackpad.pan_y - start);
    assert!(
        (by_line - by_pixel * SCROLL_LINE_PIXELS).abs() < 1e-3,
        "a line should be worth {SCROLL_LINE_PIXELS} pixels, got {by_line} against {by_pixel}"
    );
}

#[test]
fn scroll_zoom_follows_the_delta_and_holds_the_pointer() {
    let doc = (2000.0, 1500.0);
    let mut c = cam(1000.0, 800.0);
    c.fit(doc.0, doc.1);
    let before = c.zoom;
    let (anchor_x, anchor_y) = (300.0, 200.0);
    let under_pointer = c.to_doc(anchor_x, anchor_y);

    c.zoom_by_scroll(anchor_x, anchor_y, -3.0, false, doc.0, doc.1);
    assert!(c.zoom > before, "a negative delta should zoom in");

    let (screen_x, screen_y) = c.to_screen(under_pointer.0, under_pointer.1);
    assert!((screen_x - anchor_x).abs() < 1e-2);
    assert!((screen_y - anchor_y).abs() < 1e-2);

    c.zoom_by_scroll(anchor_x, anchor_y, 3.0, false, doc.0, doc.1);
    assert!(
        (c.zoom - before).abs() < 1e-3,
        "one notch back should undo it"
    );
}

#[test]
fn a_wheel_notch_zooms_further_than_a_trackpad_pixel() {
    let doc = (2000.0, 1500.0);
    let mut wheel = cam(1000.0, 800.0);
    wheel.fit(doc.0, doc.1);
    let mut trackpad = wheel;

    wheel.zoom_by_scroll(500.0, 400.0, -1.0, false, doc.0, doc.1);
    trackpad.zoom_by_scroll(500.0, 400.0, -1.0, true, doc.0, doc.1);
    assert!(wheel.zoom > trackpad.zoom);
}

#[test]
fn paper_scissor_covers_the_paper_in_framebuffer_pixels() {
    let c = Camera {
        zoom: 1.0,
        pan_x: 10.0,
        pan_y: 20.0,
        viewport_width: 200.0,
        viewport_height: 200.0,
        dpr: 2.0,
    };
    let (x, y, w, h) = c.paper_scissor(40.0, 30.0, 400, 400).expect("on screen");
    assert_eq!((x, y, w, h), (20, 40, 80, 60));
}

#[test]
fn paper_scissor_clips_to_the_framebuffer() {
    let c = Camera {
        zoom: 1.0,
        pan_x: -20.0,
        pan_y: -10.0,
        viewport_width: 100.0,
        viewport_height: 80.0,
        dpr: 1.0,
    };
    let (x, y, w, h) = c.paper_scissor(80.0, 50.0, 100, 80).expect("partial");
    assert_eq!(x, 0);
    assert_eq!(y, 0);
    assert_eq!(w, 60);
    assert_eq!(h, 40);
}

#[test]
fn paper_scissor_is_none_when_the_paper_is_off_screen() {
    let c = Camera {
        zoom: 1.0,
        pan_x: 400.0,
        pan_y: 400.0,
        viewport_width: 100.0,
        viewport_height: 100.0,
        dpr: 1.0,
    };
    assert!(c.paper_scissor(50.0, 50.0, 100, 100).is_none());
}
