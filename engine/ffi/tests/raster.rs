use calumma_core::Tool;
use calumma_ffi::*;
use calumma_io::{encode_rgba, RasterFormat};
use std::ffi::CString;
use std::ptr;

fn engine() -> (tempfile::TempDir, *mut CalmEngine) {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("raster.sqlite").to_str().unwrap()).unwrap();
    let ptr = unsafe { calm_engine_new(path.as_ptr()) };
    assert!(!ptr.is_null());
    (dir, ptr)
}

#[test]
fn export_image_writes_a_png() {
    let (_dir, e) = engine();
    let name = CString::new("Raster").unwrap();
    let id = unsafe { calm_project_create(e, name.as_ptr(), 16, 16) };
    assert!(!id.is_null());
    unsafe { calm_string_free(id) };
    unsafe {
        assert_eq!(calm_engine_set_tool(e, Tool::Pen as u32), CalmStatus::Ok);
        assert_eq!(calm_engine_pointer_down(e, 2.0, 2.0), CalmStatus::Ok);
        assert_eq!(calm_engine_pointer_up(e, 2.0, 2.0), CalmStatus::Ok);
    }
    let mut bytes: *mut u8 = ptr::null_mut();
    let mut len = 0usize;
    assert_eq!(
        unsafe { calm_engine_export_image(e, RasterFormat::Png as u32, &mut bytes, &mut len) },
        CalmStatus::Ok
    );
    assert!(len > 8);
    let head = unsafe { std::slice::from_raw_parts(bytes, 8) };
    assert_eq!(&head[..4], b"\x89PNG");
    unsafe { calm_buffer_free(bytes, len) };
    unsafe { calm_engine_free(e) };
}

#[test]
fn create_from_encoded_opens_a_png() {
    let (_dir, e) = engine();
    let png = encode_rgba(&[200, 40, 40, 255].repeat(16), 4, 4, RasterFormat::Png).unwrap();
    let name = CString::new("FromPng").unwrap();
    let id = unsafe { calm_project_create_from_encoded(e, name.as_ptr(), png.as_ptr(), png.len()) };
    assert!(!id.is_null());
    unsafe { calm_string_free(id) };
    let mut state = unsafe { std::mem::zeroed::<CalmState>() };
    assert_eq!(unsafe { calm_engine_state(e, &mut state) }, CalmStatus::Ok);
    assert_eq!(state.width, 4);
    assert_eq!(state.height, 4);
    let png2 = encode_rgba(&[40, 200, 40, 255].repeat(16), 4, 4, RasterFormat::Png).unwrap();
    let mut outcome = 0u32;
    assert_eq!(
        unsafe { calm_engine_paste_encoded(e, png2.as_ptr(), png2.len(), &mut outcome) },
        CalmStatus::Ok
    );
    assert_ne!(outcome, 0);
    unsafe { calm_engine_free(e) };
}

#[test]
fn decode_rejects_garbage() {
    let junk = [1u8, 2, 3, 4];
    let mut rgba = ptr::null_mut();
    let mut len = 0usize;
    let mut w = 0u32;
    let mut h = 0u32;
    assert_eq!(
        unsafe {
            calm_image_decode(
                junk.as_ptr(),
                junk.len(),
                &mut rgba,
                &mut len,
                &mut w,
                &mut h,
            )
        },
        CalmStatus::Error
    );
}
