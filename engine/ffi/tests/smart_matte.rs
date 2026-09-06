//! Smart Matte needs no parameters, so unlike Upscale and Seam Carve it has no bespoke entry
//! point — the generic `calm_engine_run_op` reaches it. That is the thing worth pinning here.

use calumma_ffi::*;
use std::ffi::CString;

struct TestEngine {
    ptr: *mut CalmEngine,
    _dir: tempfile::TempDir,
}

impl TestEngine {
    fn new(w: u32, h: u32) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = CString::new(dir.path().join("matte.sqlite").to_str().unwrap()).unwrap();
        let ptr = unsafe { calm_engine_new(path.as_ptr()) };
        assert!(!ptr.is_null());
        let name = CString::new("Matte").unwrap();
        let id = unsafe { calm_project_create(ptr, name.as_ptr(), w, h) };
        assert!(!id.is_null());
        unsafe { calm_string_free(id) };
        Self { ptr, _dir: dir }
    }
}

impl Drop for TestEngine {
    fn drop(&mut self) {
        unsafe { calm_engine_free(self.ptr) };
    }
}

const SMART_MATTE_KIND: u32 = 6;

/// Unlike Vision's Remove Background, this one is available with no platform vtable attached at
/// all — that is the whole point of it being `Backend::Core`.
#[test]
fn smart_matte_is_available_without_platform_ops() {
    let e = TestEngine::new(64, 64);
    unsafe {
        assert!(calm_engine_op_available(e.ptr, SMART_MATTE_KIND));
    }
}

/// Layer 0 of a fresh project is Paper — a flat white field with no subject in it — so the op
/// finds no foreground and reports a clean failure rather than matting the whole layer away.
#[test]
fn a_featureless_paper_layer_reports_failure_rather_than_erasing_everything() {
    let e = TestEngine::new(64, 64);
    unsafe {
        assert_eq!(
            calm_engine_run_op(e.ptr, SMART_MATTE_KIND, 0),
            CalmStatus::Error
        );
    }
}

#[test]
fn smart_matte_with_no_project_open_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("empty.sqlite").to_str().unwrap()).unwrap();
    let e = unsafe { calm_engine_new(path.as_ptr()) };
    assert!(!e.is_null());
    unsafe {
        assert_eq!(
            calm_engine_run_op(e, SMART_MATTE_KIND, 0),
            CalmStatus::Error
        );
        calm_engine_free(e);
    }
}

#[test]
fn smart_matte_on_an_out_of_range_layer_is_an_error() {
    let e = TestEngine::new(64, 64);
    unsafe {
        assert_eq!(
            calm_engine_run_op(e.ptr, SMART_MATTE_KIND, 99),
            CalmStatus::Error
        );
    }
}

// --- Seeded by what the user drew ---

/// Two identical red subjects on a blue field, as a project built straight from pixels.
fn engine_with_two_subjects(w: u32, h: u32) -> (tempfile::TempDir, *mut CalmEngine) {
    let mut rgba = [20u8, 40, 200, 255].repeat((w * h) as usize);
    for y in h / 3..h * 2 / 3 {
        for x in w / 8..w * 3 / 8 {
            let i = ((y * w + x) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&[220, 30, 30, 255]);
        }
        for x in w * 5 / 8..w * 7 / 8 {
            let i = ((y * w + x) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&[220, 30, 30, 255]);
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("drawn.sqlite").to_str().unwrap()).unwrap();
    let ptr = unsafe { calm_engine_new(path.as_ptr()) };
    assert!(!ptr.is_null());
    let name = CString::new("Drawn").unwrap();
    let id = unsafe {
        calm_project_create_from_image(ptr, name.as_ptr(), w, h, rgba.as_ptr(), rgba.len())
    };
    assert!(!id.is_null());
    unsafe { calm_string_free(id) };
    assert_eq!(
        unsafe { calm_engine_resize(ptr, w, h, 1.0) },
        CalmStatus::Ok
    );
    assert_eq!(unsafe { calm_engine_fit(ptr) }, CalmStatus::Ok);
    (dir, ptr)
}

fn state_of(ptr: *mut CalmEngine) -> CalmState {
    let mut out = CalmState {
        width: 0,
        height: 0,
        zoom: 0.0,
        min_zoom: 0.0,
        max_zoom: 0.0,
        pan_x: 0.0,
        pan_y: 0.0,
        active_layer: 0,
        layer_count: 0,
        can_undo: 0,
        can_redo: 0,
        stroke_active: 0,
        dark_theme: 0,
        accent: 0,
        zoom_unit: 0.0,
        last_shape_tool: 0,
        last_select_tool: 0,
        is_fit: 0,
        transform_active: 0,
    };
    assert_eq!(unsafe { calm_engine_state(ptr, &mut out) }, CalmStatus::Ok);
    out
}

/// Drags a rectangular marquee across the given document-space box, the way a hand would.
fn draw_marquee(ptr: *mut CalmEngine, from: (f32, f32), to: (f32, f32)) {
    let s = state_of(ptr);
    let screen = |p: (f32, f32)| (p.0 * s.zoom + s.pan_x, p.1 * s.zoom + s.pan_y);
    let (x0, y0) = screen(from);
    let (x1, y1) = screen(to);
    unsafe {
        // 6 == Tool::SelectRect
        assert_eq!(calm_engine_set_tool(ptr, 6), CalmStatus::Ok);
        assert_eq!(calm_engine_pointer_down(ptr, x0, y0), CalmStatus::Ok);
        calm_engine_pointer_move(ptr, x1, y1);
        assert_eq!(calm_engine_pointer_up(ptr, x1, y1), CalmStatus::Ok);
        assert!(
            calm_engine_has_selection(ptr) != 0,
            "the drag left a selection"
        );
    }
}

#[test]
fn drawing_around_a_subject_mattes_it_and_cuts_the_one_outside() {
    let (w, h) = (96u32, 96u32);
    let (_dir, e) = engine_with_two_subjects(w, h);
    // Loop around the left subject only (it spans x 12..36, y 32..64).
    draw_marquee(e, (6.0, 26.0), (42.0, 70.0));
    // `create_from_image` puts the artwork on the paint layer above Paper, not on Paper itself.
    let layer = state_of(e).active_layer;
    unsafe {
        assert_eq!(calm_engine_smart_matte(e, layer), CalmStatus::Ok);
    }

    // The matte is baked into the layer's alpha, so read it back through the composite — with
    // Paper hidden, since an opaque white sheet underneath would make everything read as 255.
    assert_eq!(
        unsafe { calm_engine_set_layer_visible(e, 0, 0) },
        CalmStatus::Ok
    );
    let (cw, rgba) = {
        let mut data: *mut u8 = std::ptr::null_mut();
        let mut out_w = 0u32;
        let mut out_h = 0u32;
        assert_eq!(
            unsafe { calm_engine_composite_rgba(e, &mut data, &mut out_w, &mut out_h) },
            CalmStatus::Ok
        );
        assert!(!data.is_null());
        let len = (out_w as usize) * (out_h as usize) * 4;
        let slice = unsafe { std::slice::from_raw_parts(data, len) }.to_vec();
        unsafe { calm_buffer_free(data, len) };
        (out_w, slice)
    };
    let alpha_at = |x: u32, y: u32| rgba[((y * cw + x) * 4 + 3) as usize];

    assert_eq!(
        alpha_at(24, 48),
        255,
        "the subject inside the marquee survives"
    );
    assert_eq!(
        alpha_at(72, 48),
        0,
        "the identical subject outside it is cut — the drawn region reached the op"
    );
    unsafe { calm_engine_free(e) };
}

/// With nothing selected the same call still works, on the automatic seeding — drawing is an
/// improvement on the one-click path, not a precondition for it.
#[test]
fn with_no_selection_it_falls_back_to_automatic_seeding() {
    let (w, h) = (96u32, 96u32);
    let (_dir, e) = engine_with_two_subjects(w, h);
    let layer = state_of(e).active_layer;
    unsafe {
        assert!(calm_engine_has_selection(e) == 0);
        assert_eq!(calm_engine_smart_matte(e, layer), CalmStatus::Ok);
        calm_engine_free(e);
    }
}

#[test]
fn smart_matte_entry_point_guards_the_same_way_run_op_does() {
    let (w, h) = (64u32, 64u32);
    let (_dir, e) = engine_with_two_subjects(w, h);
    unsafe {
        assert_eq!(calm_engine_smart_matte(e, 99), CalmStatus::Error);
        calm_engine_free(e);
    }
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("none.sqlite").to_str().unwrap()).unwrap();
    let empty = unsafe { calm_engine_new(path.as_ptr()) };
    unsafe {
        assert_eq!(calm_engine_smart_matte(empty, 0), CalmStatus::Error);
        calm_engine_free(empty);
    }
}
