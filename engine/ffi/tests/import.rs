//! Multi-image project creation across the paste FFI boundary.

use calumma_core::limits::IMPORT_MAX_SIDE;
use calumma_ffi::*;
use std::ffi::{CStr, CString};
use std::ptr;

fn engine() -> (tempfile::TempDir, *mut CalmEngine) {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("import.sqlite").to_str().unwrap()).unwrap();
    let ptr = unsafe { calm_engine_new(path.as_ptr()) };
    assert!(!ptr.is_null());
    (dir, ptr)
}

fn solid(w: u32, h: u32, px: [u8; 4]) -> Vec<u8> {
    let mut out = vec![0u8; (w * h * 4) as usize];
    for chunk in out.chunks_exact_mut(4) {
        chunk.copy_from_slice(&px);
    }
    out
}

fn image_payload(name: &str, rgba: &[u8], w: u32, h: u32) -> (CString, CalmPasteImage) {
    let name_c = CString::new(name).unwrap();
    let payload = CalmPasteImage {
        name: name_c.as_ptr(),
        premultiplied_rgba: rgba.as_ptr(),
        len: rgba.len(),
        width: w,
        height: h,
    };
    (name_c, payload)
}

#[test]
fn create_from_images_sizes_the_canvas_to_the_largest_in_the_batch() {
    let (_dir, e) = engine();
    let wide = solid(24, 8, [255, 0, 0, 255]);
    let tall = solid(8, 20, [0, 255, 0, 255]);
    let (name_a, img_a) = image_payload("wide", &wide, 24, 8);
    let (name_b, img_b) = image_payload("tall", &tall, 8, 20);
    let images = [img_a, img_b];
    let project = CString::new("Batch").unwrap();
    let id_ptr = unsafe {
        calm_project_create_from_images(e, project.as_ptr(), images.as_ptr(), images.len())
    };
    assert!(!id_ptr.is_null());
    let id = unsafe { CStr::from_ptr(id_ptr) }
        .to_str()
        .unwrap()
        .to_string();
    unsafe { calm_string_free(id_ptr) };
    let _ = id;

    let mut state = unsafe { std::mem::zeroed::<CalmState>() };
    assert_eq!(unsafe { calm_engine_state(e, &mut state) }, CalmStatus::Ok);
    assert_eq!((state.width, state.height), (24, 20));
    assert_eq!(state.layer_count, 3, "paper, first image, second image");

    let name_ptr = unsafe { calm_engine_layer_name(e, 2) };
    assert!(!name_ptr.is_null());
    assert_eq!(
        unsafe { CStr::from_ptr(name_ptr) }.to_str().unwrap(),
        "tall"
    );
    unsafe { calm_string_free(name_ptr) };

    drop(name_a);
    drop(name_b);
    unsafe { calm_engine_free(e) };
}

#[test]
fn create_from_images_rejects_bad_payloads() {
    let (_dir, e) = engine();
    let project = CString::new("Bad").unwrap();
    assert!(
        unsafe { calm_project_create_from_images(e, project.as_ptr(), ptr::null(), 1) }.is_null()
    );
    assert!(unsafe { calm_project_create_from_images(e, ptr::null(), ptr::null(), 0) }.is_null());

    let rgba = solid(4, 4, [255, 0, 0, 255]);
    let short = CalmPasteImage {
        name: ptr::null(),
        premultiplied_rgba: rgba.as_ptr(),
        len: rgba.len() - 1,
        width: 4,
        height: 4,
    };
    assert!(unsafe { calm_project_create_from_images(e, project.as_ptr(), &short, 1) }.is_null());

    let over = IMPORT_MAX_SIDE + 1;
    let big = solid(over, 4, [255, 0, 0, 255]);
    let huge = CalmPasteImage {
        name: ptr::null(),
        premultiplied_rgba: big.as_ptr(),
        len: big.len(),
        width: over,
        height: 4,
    };
    assert!(unsafe { calm_project_create_from_images(e, project.as_ptr(), &huge, 1) }.is_null());
    unsafe { calm_engine_free(e) };
}
