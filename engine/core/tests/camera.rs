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
