use calumma_ffi::*;
use std::ffi::CString;
use std::ptr;

const SIDE: u32 = 400;

fn engine() -> (tempfile::TempDir, *mut CalmEngine) {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("cam.sqlite").to_str().unwrap()).unwrap();
    let ptr = unsafe { calm_engine_new(path.as_ptr()) };
    assert!(!ptr.is_null());
    let name = CString::new("Camera").unwrap();
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
fn desk_metrics_and_fit_helpers_answer_without_a_project() {
    let mut metrics = CalmDeskMetrics {
        cell: 0.0,
        line_width: 0.0,
        cross_arm: 0.0,
        cross_line_width: 0.0,
        line_alpha: 0.0,
    };
    assert_eq!(unsafe { calm_desk_metrics(&mut metrics) }, CalmStatus::Ok);
    assert!(metrics.cell > 0.0);

    let mut w = 0f32;
    let mut h = 0f32;
    assert_eq!(
        unsafe { calm_fit_size(800.0, 600.0, SIDE as f32, SIDE as f32, &mut w, &mut h) },
        CalmStatus::Ok
    );
    assert!(w > 0.0 && h > 0.0);

    let mut zoom = 0f32;
    let mut pan_x = 0f32;
    let mut pan_y = 0f32;
    assert_eq!(
        unsafe {
            calm_fit_camera(
                800.0,
                600.0,
                SIDE as f32,
                SIDE as f32,
                &mut zoom,
                &mut pan_x,
                &mut pan_y,
            )
        },
        CalmStatus::Ok
    );
    assert!(zoom > 0.0);
}

#[test]
fn viewport_scroll_and_end_camera_motion_flow_through_the_bridge() {
    let (_dir, e) = engine();
    let mut vw = 0f32;
    let mut vh = 0f32;
    unsafe {
        assert_eq!(calm_engine_viewport(e, &mut vw, &mut vh), CalmStatus::Ok);
        assert_eq!(vw, SIDE as f32);

        assert_eq!(calm_engine_pan_scroll(e, 0.0, -10.0, 1), CalmStatus::Ok);
        assert_eq!(
            calm_engine_zoom_scroll(e, 200.0, 200.0, 1.0, 1),
            CalmStatus::Ok
        );
        assert_eq!(calm_engine_render(e), CalmStatus::Ok);
        assert_eq!(calm_engine_end_camera_motion(e), CalmStatus::Ok);
        assert_eq!(calm_engine_set_clone_aligned(e, 0), CalmStatus::Ok);
        assert_eq!(calm_engine_set_clone_aligned(e, 1), CalmStatus::Ok);
        calm_engine_free(e);
    }
}

#[test]
fn fit_and_zoom_helpers_reject_null_outputs() {
    assert_eq!(
        unsafe {
            calm_fit_size(
                800.0,
                600.0,
                SIDE as f32,
                SIDE as f32,
                ptr::null_mut(),
                &mut 0f32,
            )
        },
        CalmStatus::Null
    );
    assert_eq!(
        unsafe {
            calm_fit_camera(
                800.0,
                600.0,
                SIDE as f32,
                SIDE as f32,
                ptr::null_mut(),
                &mut 0f32,
                &mut 0f32,
            )
        },
        CalmStatus::Null
    );
    assert_eq!(
        unsafe { calm_desk_metrics(ptr::null_mut()) },
        CalmStatus::Null
    );
}
