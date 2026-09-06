//! `calm_engine_seam_carve_layer`'s boundary: the carving is covered in core and ops, so this
//! only checks the crossing — an explicit target size gets through, a bad one is refused, and
//! neither a missing project nor a stray layer index panics across the ABI.

use calumma_ffi::*;
use std::ffi::CString;

struct TestEngine {
    ptr: *mut CalmEngine,
    _dir: tempfile::TempDir,
}

impl TestEngine {
    fn new(w: u32, h: u32) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = CString::new(dir.path().join("carve.sqlite").to_str().unwrap()).unwrap();
        let ptr = unsafe { calm_engine_new(path.as_ptr()) };
        assert!(!ptr.is_null());
        let name = CString::new("Carve").unwrap();
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

const SEAM_CARVE_KIND: u32 = 5;

#[test]
fn seam_carve_is_available_with_no_platform_ops_attached() {
    let e = TestEngine::new(32, 32);
    unsafe {
        assert!(calm_engine_op_available(e.ptr, SEAM_CARVE_KIND));
    }
}

#[test]
fn carving_to_an_explicit_size_succeeds() {
    let e = TestEngine::new(32, 32);
    unsafe {
        assert_eq!(
            calm_engine_seam_carve_layer(e.ptr, 0, 24, 28),
            CalmStatus::Ok
        );
    }
}

/// `calm_engine_run_op` can only pass default params, and `SeamCarveOp` has no default target
/// size to fall back on — so the generic path refuses rather than guessing at one.
#[test]
fn the_generic_run_op_path_refuses_seam_carve_for_want_of_a_target_size() {
    let e = TestEngine::new(32, 32);
    unsafe {
        assert_eq!(
            calm_engine_run_op(e.ptr, SEAM_CARVE_KIND, 0),
            CalmStatus::Error
        );
    }
}

#[test]
fn a_target_outside_the_canvas_limits_is_an_error() {
    let e = TestEngine::new(32, 32);
    unsafe {
        assert_eq!(
            calm_engine_seam_carve_layer(e.ptr, 0, 2, 28),
            CalmStatus::Error
        );
        assert_eq!(
            calm_engine_seam_carve_layer(e.ptr, 0, 24, 99_999),
            CalmStatus::Error
        );
    }
}

#[test]
fn seam_carve_with_no_project_open_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("empty.sqlite").to_str().unwrap()).unwrap();
    let e = unsafe { calm_engine_new(path.as_ptr()) };
    assert!(!e.is_null());
    unsafe {
        assert_eq!(
            calm_engine_seam_carve_layer(e, 0, 24, 24),
            CalmStatus::Error
        );
        calm_engine_free(e);
    }
}

#[test]
fn seam_carve_on_an_out_of_range_layer_is_an_error() {
    let e = TestEngine::new(32, 32);
    unsafe {
        assert_eq!(
            calm_engine_seam_carve_layer(e.ptr, 99, 24, 24),
            CalmStatus::Error
        );
    }
}
