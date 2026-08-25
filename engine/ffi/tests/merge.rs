//! Clip to Layer Below across the boundary.
//!
//! Core owns whether a clip is legal; what can only go wrong here is the two halves
//! disagreeing — the shell greys the button out on `calm_engine_layer_can_clip_down` and the
//! engine refuses the call on `Document::can_clip_layer_down`, so a case where the predicate
//! says yes and the call says no would put a live button in front of a no-op.

use calumma_ffi::*;
use std::ffi::CString;

fn engine_with_layers(extra: usize) -> (tempfile::TempDir, *mut CalmEngine) {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("t.sqlite").to_str().unwrap()).unwrap();
    let e = unsafe { calm_engine_new(path.as_ptr()) };
    assert!(!e.is_null());
    let name = CString::new("Clip").unwrap();
    let id = unsafe { calm_project_create(e, name.as_ptr(), 32, 32) };
    assert!(!id.is_null());
    unsafe { calm_string_free(id) };
    for _ in 0..extra {
        assert_eq!(unsafe { calm_engine_add_layer(e) }, CalmStatus::Ok);
    }
    (dir, e)
}

fn layer_count(e: *mut CalmEngine) -> u32 {
    let mut state = unsafe { std::mem::zeroed::<CalmState>() };
    assert_eq!(unsafe { calm_engine_state(e, &mut state) }, CalmStatus::Ok);
    state.layer_count
}

#[test]
fn clipping_removes_the_source_layer() {
    let (_dir, e) = engine_with_layers(2);
    let before = layer_count(e);
    let top = before - 1;
    assert_eq!(calm_engine_layer_can_clip_down(e, top), 1);
    assert_eq!(calm_engine_clip_layer_down(e, top), CalmStatus::Ok);
    assert_eq!(layer_count(e), before - 1);
}

#[test]
fn the_predicate_and_the_call_refuse_the_same_cases() {
    let (_dir, e) = engine_with_layers(1);
    for index in 0..layer_count(e) + 2 {
        let allowed = calm_engine_layer_can_clip_down(e, index) == 1;
        let (_dir2, twin) = engine_with_layers(1);
        let ok = calm_engine_clip_layer_down(twin, index) == CalmStatus::Ok;
        assert_eq!(allowed, ok, "at index {index}");
        unsafe { calm_engine_free(twin) };
    }
    unsafe { calm_engine_free(e) };
}

/// Paper is index 0, so the layer directly above it can be merged nowhere and clipped nowhere.
#[test]
fn clipping_into_paper_is_refused_across_the_boundary() {
    let (_dir, e) = engine_with_layers(0);
    assert_eq!(calm_engine_layer_can_clip_down(e, 1), 0);
    assert_eq!(calm_engine_clip_layer_down(e, 1), CalmStatus::Error);
    assert_eq!(layer_count(e), 2, "nothing was removed");
    unsafe { calm_engine_free(e) };
}
