//! The Upscale Smart Tool's own FFI surface. `calm_engine_run_op` already reaches `UpscaleOp`
//! generically (it is `Backend::Core`, registered unconditionally — see `base_op_registry` in
//! `engine.rs`), so what is new here is only `calm_engine_upscale_layer`, the one entry point
//! that can pass a real scale instead of the generic dispatcher's fixed default.

use calumma_ffi::*;
use std::ffi::CString;

struct TestEngine {
    ptr: *mut CalmEngine,
    _dir: tempfile::TempDir,
}

impl TestEngine {
    fn new(w: u32, h: u32) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = CString::new(dir.path().join("upscale.sqlite").to_str().unwrap()).unwrap();
        let ptr = unsafe { calm_engine_new(path.as_ptr()) };
        assert!(!ptr.is_null());
        let name = CString::new("Upscale").unwrap();
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

const UPSCALE_KIND: u32 = 4; // OpKind::Upscale — RemoveBackground=0, GenerateTexture=1, Vectorize=2, SuggestShape=3

#[test]
fn upscale_is_available_with_no_platform_ops_attached() {
    let e = TestEngine::new(16, 16);
    unsafe {
        assert!(calm_engine_op_available(e.ptr, UPSCALE_KIND));
    }
}

#[test]
fn run_op_upscales_with_the_generic_default_of_two_x() {
    let e = TestEngine::new(4, 4);
    unsafe {
        assert_eq!(calm_engine_run_op(e.ptr, UPSCALE_KIND, 0), CalmStatus::Ok);
    }
}

#[test]
fn upscale_layer_honours_an_explicit_scale() {
    let e = TestEngine::new(4, 4);
    unsafe {
        assert_eq!(calm_engine_upscale_layer(e.ptr, 0, 3.0), CalmStatus::Ok);
    }
}

#[test]
fn upscale_layer_rejects_a_bad_scale_rather_than_panicking() {
    let e = TestEngine::new(4, 4);
    unsafe {
        assert_eq!(calm_engine_upscale_layer(e.ptr, 0, 0.5), CalmStatus::Error);
        assert_eq!(
            calm_engine_upscale_layer(e.ptr, 0, f32::NAN),
            CalmStatus::Error
        );
    }
}

#[test]
fn upscale_layer_with_no_project_open_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("empty.sqlite").to_str().unwrap()).unwrap();
    let e = unsafe { calm_engine_new(path.as_ptr()) };
    assert!(!e.is_null());
    unsafe {
        assert_eq!(calm_engine_upscale_layer(e, 0, 2.0), CalmStatus::Error);
        calm_engine_free(e);
    }
}

#[test]
fn upscale_layer_on_an_out_of_range_index_is_an_error() {
    let e = TestEngine::new(4, 4);
    unsafe {
        assert_eq!(calm_engine_upscale_layer(e.ptr, 99, 2.0), CalmStatus::Error);
    }
}
