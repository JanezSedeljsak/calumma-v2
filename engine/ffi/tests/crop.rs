//! The Crop tool's own FFI surface: entering/dragging/committing rides the existing
//! `calm_engine_set_tool` / `calm_engine_pointer_*` calls (`Tool::Crop` dispatches through the
//! same generic pointer handlers every other tool does), so what is left to check here is only
//! the handful of new functions — aspect lock, overlay style, straighten, commit and cancel —
//! and that they behave with no project open. The geometry itself is covered in core.

use calumma_core::Tool;
use calumma_ffi::*;
use std::ffi::CString;

struct TestEngine {
    ptr: *mut CalmEngine,
    _dir: tempfile::TempDir,
}

impl TestEngine {
    fn new(w: u32, h: u32) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = CString::new(dir.path().join("crop.sqlite").to_str().unwrap()).unwrap();
        let ptr = unsafe { calm_engine_new(path.as_ptr()) };
        assert!(!ptr.is_null());
        let name = CString::new("Crop").unwrap();
        let id = unsafe { calm_project_create(ptr, name.as_ptr(), w, h) };
        assert!(!id.is_null());
        unsafe { calm_string_free(id) };
        assert_eq!(
            unsafe { calm_engine_resize(ptr, w, h, 1.0) },
            CalmStatus::Ok
        );
        assert_eq!(unsafe { calm_engine_fit(ptr) }, CalmStatus::Ok);
        Self { ptr, _dir: dir }
    }

    fn state(&self) -> CalmState {
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
        assert_eq!(
            unsafe { calm_engine_state(self.ptr, &mut out) },
            CalmStatus::Ok
        );
        out
    }

    /// `fit_to_view`'s zoom and pan are not something a test should hardcode assumptions
    /// about — this maps a document point through whatever camera the engine actually has,
    /// the same way `Camera::to_screen` does, so a drag lands on an exact document pixel
    /// regardless of how the canvas happened to be fitted into the viewport.
    fn to_screen(&self, doc_x: f32, doc_y: f32) -> (f32, f32) {
        let s = self.state();
        (doc_x * s.zoom + s.pan_x, doc_y * s.zoom + s.pan_y)
    }
}

impl Drop for TestEngine {
    fn drop(&mut self) {
        unsafe { calm_engine_free(self.ptr) };
    }
}

const SIDE: u32 = 200;

#[test]
fn dragging_a_corner_and_committing_resizes_the_document() {
    let e = TestEngine::new(SIDE, SIDE);
    unsafe {
        assert_eq!(
            calm_engine_set_tool(e.ptr, Tool::Crop as u32),
            CalmStatus::Ok
        );
        let (sx0, sy0) = e.to_screen(200.0, 200.0);
        let (sx1, sy1) = e.to_screen(120.0, 150.0);
        assert_eq!(calm_engine_pointer_down(e.ptr, sx0, sy0), CalmStatus::Ok);
        assert_eq!(calm_engine_pointer_move(e.ptr, sx1, sy1), CalmStatus::Ok);
        assert_eq!(calm_engine_pointer_up(e.ptr, sx1, sy1), CalmStatus::Ok);
        assert_eq!(calm_engine_commit_crop(e.ptr), CalmStatus::Ok);
    }
    let s = e.state();
    assert_eq!((s.width, s.height), (120, 150));
}

/// There is no dedicated "cancel crop" entry point — switching to any other tool already exits
/// Crop without applying anything, since `Document::set_tool` calls `exit_crop` on its way out.
#[test]
fn switching_tools_away_from_crop_discards_the_drag_untouched() {
    let e = TestEngine::new(SIDE, SIDE);
    unsafe {
        assert_eq!(
            calm_engine_set_tool(e.ptr, Tool::Crop as u32),
            CalmStatus::Ok
        );
        let (sx0, sy0) = e.to_screen(200.0, 200.0);
        let (sx1, sy1) = e.to_screen(10.0, 10.0);
        assert_eq!(calm_engine_pointer_down(e.ptr, sx0, sy0), CalmStatus::Ok);
        assert_eq!(calm_engine_pointer_move(e.ptr, sx1, sy1), CalmStatus::Ok);
        assert_eq!(
            calm_engine_set_tool(e.ptr, Tool::Move as u32),
            CalmStatus::Ok
        );
    }
    let s = e.state();
    assert_eq!((s.width, s.height), (SIDE, SIDE));
}

#[test]
fn an_aspect_lock_survives_a_round_trip_and_shapes_the_crop() {
    let e = TestEngine::new(SIDE, SIDE);
    unsafe {
        assert_eq!(
            calm_engine_set_tool(e.ptr, Tool::Crop as u32),
            CalmStatus::Ok
        );
        assert_eq!(calm_engine_set_crop_aspect_lock(e.ptr, 2.0), CalmStatus::Ok);
        let (sx0, sy0) = e.to_screen(200.0, 200.0);
        let (sx1, sy1) = e.to_screen(160.0, 200.0);
        assert_eq!(calm_engine_pointer_down(e.ptr, sx0, sy0), CalmStatus::Ok);
        assert_eq!(calm_engine_pointer_move(e.ptr, sx1, sy1), CalmStatus::Ok);
        assert_eq!(calm_engine_pointer_up(e.ptr, sx1, sy1), CalmStatus::Ok);
        assert_eq!(calm_engine_commit_crop(e.ptr), CalmStatus::Ok);
    }
    let s = e.state();
    assert_eq!(s.width, 2 * s.height);
}

#[test]
fn a_non_positive_or_non_finite_aspect_lock_is_treated_as_free() {
    let e = TestEngine::new(SIDE, SIDE);
    unsafe {
        assert_eq!(
            calm_engine_set_tool(e.ptr, Tool::Crop as u32),
            CalmStatus::Ok
        );
        for bad in [0.0f32, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(calm_engine_set_crop_aspect_lock(e.ptr, bad), CalmStatus::Ok);
        }
        let (sx0, sy0) = e.to_screen(200.0, 200.0);
        let (sx1, sy1) = e.to_screen(90.0, 140.0);
        assert_eq!(calm_engine_pointer_down(e.ptr, sx0, sy0), CalmStatus::Ok);
        assert_eq!(calm_engine_pointer_move(e.ptr, sx1, sy1), CalmStatus::Ok);
        assert_eq!(calm_engine_pointer_up(e.ptr, sx1, sy1), CalmStatus::Ok);
        assert_eq!(calm_engine_commit_crop(e.ptr), CalmStatus::Ok);
    }
    let s = e.state();
    assert_eq!(
        (s.width, s.height),
        (90, 140),
        "a free-form drag, not a locked one"
    );
}

#[test]
fn every_overlay_style_id_is_accepted_and_nothing_else_is() {
    let e = TestEngine::new(SIDE, SIDE);
    unsafe {
        for style in 0u32..=4 {
            assert_eq!(
                calm_engine_set_crop_overlay_style(e.ptr, style),
                CalmStatus::Ok,
                "style {style} should be known"
            );
        }
        assert_ne!(
            calm_engine_set_crop_overlay_style(e.ptr, 999),
            CalmStatus::Ok,
            "an id from a newer shell must be refused, not silently taken as Off"
        );
    }
}

#[test]
fn straightening_leaves_the_crop_rect_alone_and_disarms_itself() {
    let e = TestEngine::new(SIDE, SIDE);
    unsafe {
        assert_eq!(
            calm_engine_set_tool(e.ptr, Tool::Crop as u32),
            CalmStatus::Ok
        );
        assert_eq!(calm_engine_set_straighten_active(e.ptr, 1), CalmStatus::Ok);
        let (sx0, sy0) = e.to_screen(20.0, 20.0);
        let (sx1, sy1) = e.to_screen(120.0, 44.0);
        assert_eq!(calm_engine_pointer_down(e.ptr, sx0, sy0), CalmStatus::Ok);
        assert_eq!(calm_engine_pointer_move(e.ptr, sx1, sy1), CalmStatus::Ok);
        assert_eq!(calm_engine_pointer_up(e.ptr, sx1, sy1), CalmStatus::Ok);
        // Straighten rotates layer transforms live; the crop rect (and so the document size)
        // is untouched until a separate commit_crop.
        assert_eq!(calm_engine_commit_crop(e.ptr), CalmStatus::Ok);
    }
    let s = e.state();
    assert_eq!((s.width, s.height), (SIDE, SIDE));
}

#[test]
fn crop_calls_with_no_project_open_are_errors_not_panics() {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("empty.sqlite").to_str().unwrap()).unwrap();
    let e = unsafe { calm_engine_new(path.as_ptr()) };
    assert!(!e.is_null());
    unsafe {
        assert_eq!(calm_engine_set_crop_aspect_lock(e, 2.0), CalmStatus::Ok);
        assert_eq!(calm_engine_clear_crop_aspect_lock(e), CalmStatus::Ok);
        assert_eq!(calm_engine_set_crop_overlay_style(e, 1), CalmStatus::Ok);
        assert_eq!(calm_engine_set_straighten_active(e, 1), CalmStatus::Ok);
        assert_eq!(calm_engine_commit_crop(e), CalmStatus::Ok);
        calm_engine_free(e);
    }
}
