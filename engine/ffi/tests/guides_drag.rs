use calumma_ffi::*;
use std::ffi::CString;

const SIDE: u32 = 400;

fn engine() -> (tempfile::TempDir, *mut CalmEngine) {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("gd.sqlite").to_str().unwrap()).unwrap();
    let ptr = unsafe { calm_engine_new(path.as_ptr()) };
    assert!(!ptr.is_null());
    let name = CString::new("GuideDrag").unwrap();
    let id = unsafe { calm_project_create(ptr, name.as_ptr(), SIDE, SIDE) };
    assert!(!id.is_null());
    unsafe { calm_string_free(id) };
    assert_eq!(
        unsafe { calm_engine_resize(ptr, SIDE, SIDE, 1.0) },
        CalmStatus::Ok
    );
    assert_eq!(unsafe { calm_engine_fit(ptr) }, CalmStatus::Ok);
    (dir, ptr)
}

#[test]
fn dragged_guide_reports_axis_position_and_screen_point() {
    let (_dir, e) = engine();
    unsafe {
        assert_eq!(calm_engine_add_guide(e, 0, 80.0), CalmStatus::Ok);
        assert_eq!(calm_engine_guide_drag_from_ruler(e, 0, 200.0, -8.0), CalmStatus::Ok);
        assert_eq!(calm_engine_guide_drag_update(e, 200.0, 120.0), CalmStatus::Ok);

        let mut axis = 0u8;
        let mut position = 0f32;
        let mut screen = [0f32; 2];
        assert_eq!(
            calm_engine_dragged_guide(e, &mut axis, &mut position, &mut screen[0]),
            1
        );
        assert_eq!(axis, 0);
        assert!(position > 0.0);

        assert_eq!(calm_engine_guide_drag_end(e, 200.0, 120.0), CalmStatus::Ok);
        assert_eq!(
            calm_engine_dragged_guide(e, &mut axis, &mut position, &mut screen[0]),
            0
        );
        assert!(calm_guides_limit() > 0);
        calm_engine_free(e);
    }
}

#[test]
fn dragged_guide_rejects_null_outputs() {
    let (_dir, e) = engine();
    unsafe {
        assert_eq!(calm_engine_guide_drag_from_ruler(e, 1, -8.0, 200.0), CalmStatus::Ok);
        assert_eq!(
            calm_engine_dragged_guide(e, std::ptr::null_mut(), &mut 0f32, &mut 0f32),
            0
        );
        calm_engine_free(e);
    }
}
