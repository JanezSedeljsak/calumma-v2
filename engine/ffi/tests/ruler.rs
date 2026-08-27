//! Ruler ticks across the boundary.
//!
//! The tick maths itself is core's (`engine/core/tests/ruler.rs`). What can only go wrong
//! here is the buffer contract: the shell hands over a fixed-size array and a capacity, and
//! this side has to fill it without ever writing past `cap` — a ruler at a zoomed-out camera
//! produces far more ticks than any array the shell keeps.

use calumma_ffi::*;
use std::ffi::CString;
use std::ptr;

const WIDE: u32 = 800;
const TALL: u32 = 200;

fn engine(with_project: bool) -> (tempfile::TempDir, *mut CalmEngine) {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("r.sqlite").to_str().unwrap()).unwrap();
    let ptr = unsafe { calm_engine_new(path.as_ptr()) };
    assert!(!ptr.is_null());
    if with_project {
        let name = CString::new("Ruler").unwrap();
        let id = unsafe { calm_project_create(ptr, name.as_ptr(), WIDE, TALL) };
        assert!(!id.is_null());
        unsafe { calm_string_free(id) };
        assert_eq!(
            unsafe { calm_engine_resize(ptr, WIDE, TALL, 1.0) },
            CalmStatus::Ok
        );
        assert_eq!(unsafe { calm_engine_fit(ptr) }, CalmStatus::Ok);
    }
    (dir, ptr)
}

fn ticks_x(e: *mut CalmEngine, cap: usize) -> Vec<CalmRulerTick> {
    let mut buf = vec![CalmRulerTick { doc: 0.0, major: 0 }; cap];
    let n = unsafe { calm_engine_ruler_ticks_x(e, buf.as_mut_ptr(), cap) };
    buf.truncate(n);
    buf
}

#[test]
fn an_open_project_reports_ticks_on_both_axes() {
    let (_dir, e) = engine(true);
    let x = ticks_x(e, 512);
    assert!(!x.is_empty(), "a fitted board shows a ruler");
    assert!(
        x.iter().any(|t| t.major == 1),
        "some ticks are labeled majors"
    );
    assert!(
        x.iter().any(|t| t.major == 0),
        "and some are unlabeled minors"
    );
    assert!(
        x.windows(2).all(|p| p[1].doc > p[0].doc),
        "ticks come out in ascending document order"
    );

    let mut y = vec![CalmRulerTick { doc: 0.0, major: 0 }; 512];
    let ny = unsafe { calm_engine_ruler_ticks_y(e, y.as_mut_ptr(), 512) };
    assert!(ny > 0);
    assert!(
        ny < x.len(),
        "the short axis of an {WIDE}x{TALL} board spans fewer ticks than the long one"
    );
    unsafe { calm_engine_free(e) };
}

/// The buffer contract. `cap` is a hard ceiling, and the bytes past it belong to the caller —
/// this is the one thing a core test cannot check.
#[test]
fn a_capacity_smaller_than_the_tick_count_truncates_without_overrunning() {
    let (_dir, e) = engine(true);
    let full = ticks_x(e, 4096).len();
    assert!(full > 4, "need more ticks than the small cap to prove this");

    const CANARY: CalmRulerTick = CalmRulerTick {
        doc: -12345.0,
        major: 7,
    };
    let mut buf = vec![CANARY; 64];
    let n = unsafe { calm_engine_ruler_ticks_x(e, buf.as_mut_ptr(), 4) };
    assert_eq!(n, 4, "never more than cap");
    assert!(
        buf[..4].iter().all(|t| *t != CANARY),
        "the first four filled"
    );
    assert!(
        buf[4..].iter().all(|t| *t == CANARY),
        "nothing was written past cap"
    );
    unsafe { calm_engine_free(e) };
}

#[test]
fn a_zero_capacity_writes_nothing_and_reports_nothing() {
    let (_dir, e) = engine(true);
    let mut buf = [CalmRulerTick {
        doc: 99.0,
        major: 3,
    }];
    assert_eq!(
        unsafe { calm_engine_ruler_ticks_x(e, buf.as_mut_ptr(), 0) },
        0
    );
    assert_eq!(
        unsafe { calm_engine_ruler_ticks_y(e, buf.as_mut_ptr(), 0) },
        0
    );
    assert_eq!(buf[0].doc, 99.0, "the caller's buffer is untouched");
    unsafe { calm_engine_free(e) };
}

#[test]
fn a_null_out_pointer_is_refused_before_it_is_written_through() {
    let (_dir, e) = engine(true);
    assert_eq!(
        unsafe { calm_engine_ruler_ticks_x(e, ptr::null_mut(), 64) },
        0
    );
    assert_eq!(
        unsafe { calm_engine_ruler_ticks_y(e, ptr::null_mut(), 64) },
        0
    );
    unsafe { calm_engine_free(e) };
}

/// Landing has rulers on screen before any project exists, so asking for ticks with nothing
/// open has to be an empty answer rather than a crash.
#[test]
fn a_null_engine_or_a_closed_project_reports_no_ticks() {
    let mut buf = vec![CalmRulerTick { doc: 0.0, major: 0 }; 16];
    assert_eq!(
        unsafe { calm_engine_ruler_ticks_x(ptr::null_mut(), buf.as_mut_ptr(), 16) },
        0
    );
    assert_eq!(
        unsafe { calm_engine_ruler_ticks_y(ptr::null_mut(), buf.as_mut_ptr(), 16) },
        0
    );

    let (_dir, e) = engine(false);
    assert_eq!(ticks_x(e, 16).len(), 0, "no project, no ruler");

    let (_dir2, open) = engine(true);
    assert!(!ticks_x(open, 512).is_empty());
    assert_eq!(unsafe { calm_project_close(open) }, CalmStatus::Ok);
    assert_eq!(
        ticks_x(open, 512).len(),
        0,
        "closing takes the ruler with it"
    );

    unsafe { calm_engine_free(e) };
    unsafe { calm_engine_free(open) };
}

#[test]
fn static_ruler_ticks_answer_without_an_engine() {
    let mut buf = vec![CalmRulerTick { doc: 0.0, major: 0 }; 64];
    let n = unsafe { calm_ruler_ticks_x(1.0, 0.0, 800.0, buf.as_mut_ptr(), buf.len()) };
    assert!(n > 0);
    assert_eq!(
        unsafe { calm_ruler_ticks_x(1.0, 0.0, 800.0, ptr::null_mut(), 64) },
        0
    );
}

/// Ticks are document positions held to a *screen* spacing floor, so the step has to grow in
/// document units as the camera pulls back — driven here across the whole zoom range the
/// board allows rather than an arbitrary factor the camera would clamp.
#[test]
fn the_tick_step_widens_as_the_camera_pulls_back() {
    let (_dir, e) = engine(true);
    let span = |ticks: &[CalmRulerTick]| ticks.last().unwrap().doc - ticks.first().unwrap().doc;

    assert_eq!(unsafe { calm_engine_set_zoom_unit(e, 1.0) }, CalmStatus::Ok);
    let near = ticks_x(e, 4096);
    assert_eq!(unsafe { calm_engine_set_zoom_unit(e, 0.0) }, CalmStatus::Ok);
    let far = ticks_x(e, 4096);

    let near_step = near[1].doc - near[0].doc;
    let far_step = far[1].doc - far[0].doc;
    assert!(
        far_step > near_step,
        "pulled back, one tick is worth more document pixels: {far_step} vs {near_step}"
    );
    assert!(
        span(&far) > span(&near),
        "and the ruler covers more of the document"
    );
    assert!(
        far.len() < near.len() * 4,
        "the tick count stays bounded rather than growing with the visible span"
    );
    unsafe { calm_engine_free(e) };
}

/// Panning slides the window of document positions the ruler reports — the ticks are anchored
/// to the document, not to the viewport.
///
/// It also pins where a pan actually lands: `calm_engine_pan` only *accumulates*, and the
/// camera moves when the frame is drawn. A ruler read between the two sees the old camera,
/// which is correct — both are drawn from the same frame.
#[test]
fn panning_slides_the_window_the_ruler_covers_once_the_frame_is_drawn() {
    let (_dir, e) = engine(true);
    assert_eq!(unsafe { calm_engine_set_zoom_unit(e, 1.0) }, CalmStatus::Ok);
    let before = ticks_x(e, 4096);

    assert_eq!(unsafe { calm_engine_pan(e, -200.0, 0.0) }, CalmStatus::Ok);
    assert_eq!(
        ticks_x(e, 4096).first().unwrap().doc,
        before.first().unwrap().doc,
        "the pan is still pending, so the ruler has not moved yet"
    );

    assert_eq!(unsafe { calm_engine_render(e) }, CalmStatus::Ok);
    let after = ticks_x(e, 4096);
    assert!(
        after.first().unwrap().doc > before.first().unwrap().doc,
        "drawing the frame applies the pan and walks the ruler into the document"
    );
    unsafe { calm_engine_free(e) };
}

/// Several pan events between two frames coalesce into one camera move — the reason they are
/// accumulated rather than applied as they arrive.
#[test]
fn pans_between_frames_coalesce() {
    let (_dir, e) = engine(true);
    assert_eq!(unsafe { calm_engine_set_zoom_unit(e, 1.0) }, CalmStatus::Ok);
    let origin = ticks_x(e, 4096).first().unwrap().doc;

    for _ in 0..4 {
        assert_eq!(unsafe { calm_engine_pan(e, -50.0, 0.0) }, CalmStatus::Ok);
    }
    assert_eq!(unsafe { calm_engine_render(e) }, CalmStatus::Ok);
    let stepped = ticks_x(e, 4096).first().unwrap().doc;

    let (_dir2, once) = engine(true);
    assert_eq!(
        unsafe { calm_engine_set_zoom_unit(once, 1.0) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_pan(once, -200.0, 0.0) },
        CalmStatus::Ok
    );
    assert_eq!(unsafe { calm_engine_render(once) }, CalmStatus::Ok);
    let at_once = ticks_x(once, 4096).first().unwrap().doc;

    assert!(stepped > origin);
    assert_eq!(
        stepped, at_once,
        "four small pans land where one big one does"
    );
    unsafe { calm_engine_free(e) };
    unsafe { calm_engine_free(once) };
}
