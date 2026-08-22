use calumma_core::GuideAxis;
use calumma_ffi::*;
use std::ffi::CString;

const SIDE: u32 = 400;

/// Drives guides through the same C entry points the rulers and the Board menu call, so the
/// ruler drag — which never goes through `calm_engine_pointer_*` — is covered across the
/// bridge rather than only in core.
struct GuideEngine {
    ptr: *mut CalmEngine,
    _dir: tempfile::TempDir,
}

impl GuideEngine {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = CString::new(dir.path().join("g.sqlite").to_str().unwrap()).unwrap();
        let ptr = unsafe { calm_engine_new(path.as_ptr()) };
        assert!(!ptr.is_null());
        let name = CString::new("Guides").unwrap();
        let id = unsafe { calm_project_create(ptr, name.as_ptr(), SIDE, SIDE) };
        assert!(!id.is_null());
        unsafe { calm_string_free(id) };
        assert_eq!(
            unsafe { calm_engine_resize(ptr, SIDE, SIDE, 1.0) },
            CalmStatus::Ok
        );
        assert_eq!(unsafe { calm_engine_fit(ptr) }, CalmStatus::Ok);
        Self { ptr, _dir: dir }
    }

    fn drag_from_ruler(&self, axis: GuideAxis, from: (f32, f32), to: (f32, f32)) {
        assert_eq!(
            calm_engine_guide_drag_from_ruler(self.ptr, u8::from(axis), from.0, from.1),
            CalmStatus::Ok
        );
        assert_eq!(
            calm_engine_guide_drag_update(self.ptr, to.0, to.1),
            CalmStatus::Ok
        );
        assert_eq!(
            calm_engine_guide_drag_end(self.ptr, to.0, to.1),
            CalmStatus::Ok
        );
    }
}

impl Drop for GuideEngine {
    fn drop(&mut self) {
        unsafe { calm_engine_free(self.ptr) };
    }
}

#[test]
fn a_ruler_drag_onto_the_board_leaves_a_guide() {
    let e = GuideEngine::new();
    assert_eq!(calm_engine_guide_count(e.ptr), 0);
    e.drag_from_ruler(GuideAxis::Horizontal, (200.0, -10.0), (200.0, 180.0));
    assert_eq!(calm_engine_guide_count(e.ptr), 1);
    assert_eq!(calm_engine_guide_axis_at(e.ptr, 200.0, 180.0), 0);
    assert_eq!(calm_engine_guide_axis_at(e.ptr, 200.0, 40.0), -1);
}

#[test]
fn a_ruler_drag_released_back_over_the_ruler_leaves_nothing() {
    let e = GuideEngine::new();
    e.drag_from_ruler(GuideAxis::Vertical, (-10.0, 200.0), (-4.0, 200.0));
    assert_eq!(calm_engine_guide_count(e.ptr), 0);
}

#[test]
fn clear_guides_empties_the_board() {
    let e = GuideEngine::new();
    e.drag_from_ruler(GuideAxis::Horizontal, (200.0, -10.0), (200.0, 120.0));
    e.drag_from_ruler(GuideAxis::Vertical, (-10.0, 200.0), (140.0, 200.0));
    assert_eq!(calm_engine_guide_count(e.ptr), 2);
    assert_eq!(calm_engine_clear_guides(e.ptr), CalmStatus::Ok);
    assert_eq!(calm_engine_guide_count(e.ptr), 0);
}

#[test]
fn guides_survive_closing_and_reopening_the_project() {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("g.sqlite").to_str().unwrap()).unwrap();
    let ptr = unsafe { calm_engine_new(path.as_ptr()) };
    let name = CString::new("Persisted").unwrap();
    let raw_id = unsafe { calm_project_create(ptr, name.as_ptr(), SIDE, SIDE) };
    let id = unsafe { std::ffi::CStr::from_ptr(raw_id) }
        .to_str()
        .unwrap()
        .to_owned();
    unsafe { calm_string_free(raw_id) };
    unsafe { calm_engine_resize(ptr, SIDE, SIDE, 1.0) };
    unsafe { calm_engine_fit(ptr) };

    calm_engine_guide_drag_from_ruler(ptr, u8::from(GuideAxis::Horizontal), 200.0, -10.0);
    calm_engine_guide_drag_end(ptr, 200.0, 175.0);
    assert_eq!(unsafe { calm_project_save(ptr) }, CalmStatus::Ok);

    let cid = CString::new(id).unwrap();
    assert_eq!(
        unsafe { calm_project_open(ptr, cid.as_ptr()) },
        CalmStatus::Ok
    );
    assert_eq!(calm_engine_guide_count(ptr), 1);
    unsafe { calm_engine_free(ptr) };
}

#[test]
fn guide_entry_points_answer_safely_with_no_engine() {
    assert_eq!(calm_engine_guide_count(std::ptr::null_mut()), 0);
    assert_eq!(
        calm_engine_guide_axis_at(std::ptr::null_mut(), 0.0, 0.0),
        -1
    );
    assert_eq!(
        calm_engine_clear_guides(std::ptr::null_mut()),
        CalmStatus::Null
    );
    assert_eq!(
        calm_engine_guide_drag_from_ruler(std::ptr::null_mut(), 0, 0.0, 0.0),
        CalmStatus::Null
    );
}

#[test]
fn an_unknown_axis_is_refused_rather_than_guessed() {
    let e = GuideEngine::new();
    assert_eq!(
        calm_engine_guide_drag_from_ruler(e.ptr, 9, 10.0, 10.0),
        CalmStatus::Error
    );
    assert_eq!(calm_engine_guide_count(e.ptr), 0);
}
