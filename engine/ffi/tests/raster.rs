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

/// `create_from_encoded_opens_a_png` and `decode_rejects_garbage` exercise `calm_image_decode`
/// through its failure path only (bad bytes) and indirectly (through project creation). This is
/// the direct round trip: real bytes in, the same pixels out, freed the way the shell would.
#[test]
fn decode_round_trips_a_real_png() {
    let want = [12u8, 34, 56, 255].repeat(9);
    let png = encode_rgba(&want, 3, 3, RasterFormat::Png).unwrap();
    let mut rgba = ptr::null_mut();
    let mut len = 0usize;
    let mut w = 0u32;
    let mut h = 0u32;
    assert_eq!(
        unsafe { calm_image_decode(png.as_ptr(), png.len(), &mut rgba, &mut len, &mut w, &mut h) },
        CalmStatus::Ok
    );
    assert_eq!((w, h), (3, 3));
    assert_eq!(len, want.len());
    let got = unsafe { std::slice::from_raw_parts(rgba, len) };
    assert_eq!(got, want.as_slice());
    unsafe { calm_buffer_free(rgba, len) };
}

#[test]
fn decode_guards_every_pointer() {
    let png = encode_rgba(&[1u8, 2, 3, 255], 1, 1, RasterFormat::Png).unwrap();
    let (mut rgba, mut len, mut w, mut h) = (ptr::null_mut(), 0usize, 0u32, 0u32);
    let ok = |r: &mut *mut u8, l: &mut usize, w: &mut u32, h: &mut u32| {
        (r as *mut _, l as *mut _, w as *mut _, h as *mut _)
    };
    let (r, l, wp, hp) = ok(&mut rgba, &mut len, &mut w, &mut h);
    unsafe {
        assert_eq!(
            calm_image_decode(ptr::null(), png.len(), r, l, wp, hp),
            CalmStatus::Null,
            "null bytes"
        );
        assert_eq!(
            calm_image_decode(png.as_ptr(), png.len(), ptr::null_mut(), l, wp, hp),
            CalmStatus::Null,
            "null out_rgba"
        );
        assert_eq!(
            calm_image_decode(png.as_ptr(), png.len(), r, ptr::null_mut(), wp, hp),
            CalmStatus::Null,
            "null out_len"
        );
        assert_eq!(
            calm_image_decode(png.as_ptr(), png.len(), r, l, ptr::null_mut(), hp),
            CalmStatus::Null,
            "null out_w"
        );
        assert_eq!(
            calm_image_decode(png.as_ptr(), png.len(), r, l, wp, ptr::null_mut()),
            CalmStatus::Null,
            "null out_h"
        );
    }
}

/// `calm_engine_export_layer_image` is never called by any other test — every other export
/// test asks for the composite (`layer_index: None` inside `export_encoded`). This is its own
/// code path (`Some(index)` branch, `doc.layer_rgba`), success and failure both.
#[test]
fn export_layer_image_writes_just_that_layer() {
    let (_dir, e) = engine();
    let name = CString::new("Layer").unwrap();
    let id = unsafe { calm_project_create(e, name.as_ptr(), 8, 8) };
    assert!(!id.is_null());
    unsafe { calm_string_free(id) };
    let active = unsafe {
        let mut state = std::mem::zeroed::<CalmState>();
        assert_eq!(calm_engine_state(e, &mut state), CalmStatus::Ok);
        state.active_layer
    };

    let mut bytes: *mut u8 = ptr::null_mut();
    let mut len = 0usize;
    assert_eq!(
        unsafe {
            calm_engine_export_layer_image(
                e,
                active,
                RasterFormat::Png as u32,
                &mut bytes,
                &mut len,
            )
        },
        CalmStatus::Ok
    );
    assert!(len > 8);
    let head = unsafe { std::slice::from_raw_parts(bytes, 8) };
    assert_eq!(&head[..4], b"\x89PNG");
    unsafe { calm_buffer_free(bytes, len) };
    unsafe { calm_engine_free(e) };
}

#[test]
fn export_refuses_an_unknown_format_and_null_out_params() {
    let (_dir, e) = engine();
    let name = CString::new("Formats").unwrap();
    let id = unsafe { calm_project_create(e, name.as_ptr(), 4, 4) };
    assert!(!id.is_null());
    unsafe { calm_string_free(id) };

    let mut bytes: *mut u8 = ptr::null_mut();
    let mut len = 0usize;
    assert_eq!(
        unsafe { calm_engine_export_image(e, 999, &mut bytes, &mut len) },
        CalmStatus::Error,
        "no such RasterFormat"
    );
    assert_eq!(
        unsafe { calm_engine_export_image(e, RasterFormat::Png as u32, ptr::null_mut(), &mut len) },
        CalmStatus::Null
    );
    assert_eq!(
        unsafe {
            calm_engine_export_image(e, RasterFormat::Png as u32, &mut bytes, ptr::null_mut())
        },
        CalmStatus::Null
    );
    unsafe { calm_engine_free(e) };
}

/// The plural encoded entry points — `..._images` — are what the singular wrappers
/// (`calm_project_create_from_encoded`, `calm_engine_paste_encoded`) forward one image to.
/// Nothing else in this suite calls them directly with more than one image, a real name, or a
/// non-null `out_count`.
#[test]
fn create_from_encoded_images_opens_a_batch_sized_to_the_largest() {
    let (_dir, e) = engine();
    let small = encode_rgba(&[10u8, 10, 10, 255].repeat(4), 2, 2, RasterFormat::Png).unwrap();
    let big = encode_rgba(&[20u8, 20, 20, 255].repeat(16), 4, 4, RasterFormat::Png).unwrap();
    let name_a = CString::new("A").unwrap();
    let name_b = CString::new("B").unwrap();
    let project_name = CString::new("Batch").unwrap();
    let images = [
        CalmEncodedImage {
            name: name_a.as_ptr(),
            bytes: small.as_ptr(),
            len: small.len(),
        },
        CalmEncodedImage {
            name: name_b.as_ptr(),
            bytes: big.as_ptr(),
            len: big.len(),
        },
    ];
    let id = unsafe {
        calm_project_create_from_encoded_images(
            e,
            project_name.as_ptr(),
            images.as_ptr(),
            images.len(),
        )
    };
    assert!(!id.is_null());
    unsafe { calm_string_free(id) };
    let mut state = unsafe { std::mem::zeroed::<CalmState>() };
    assert_eq!(unsafe { calm_engine_state(e, &mut state) }, CalmStatus::Ok);
    assert_eq!((state.width, state.height), (4, 4), "sized to the larger");
    unsafe { calm_engine_free(e) };
}

#[test]
fn create_from_encoded_images_rejects_null_engine_name_and_empty_batches() {
    let (_dir, e) = engine();
    let name = CString::new("X").unwrap();
    let png = encode_rgba(&[1u8, 2, 3, 255], 1, 1, RasterFormat::Png).unwrap();
    let image = CalmEncodedImage {
        name: ptr::null(),
        bytes: png.as_ptr(),
        len: png.len(),
    };
    assert!(unsafe {
        calm_project_create_from_encoded_images(ptr::null_mut(), name.as_ptr(), &image, 1)
    }
    .is_null());
    assert!(
        unsafe { calm_project_create_from_encoded_images(e, ptr::null(), &image, 1) }.is_null()
    );
    assert!(
        unsafe { calm_project_create_from_encoded_images(e, name.as_ptr(), ptr::null(), 1) }
            .is_null()
    );
    assert!(
        unsafe { calm_project_create_from_encoded_images(e, name.as_ptr(), &image, 0) }.is_null()
    );
    let junk = CalmEncodedImage {
        name: ptr::null(),
        bytes: [1u8, 2, 3].as_ptr(),
        len: 3,
    };
    assert!(
        unsafe { calm_project_create_from_encoded_images(e, name.as_ptr(), &junk, 1) }.is_null(),
        "undecodable bytes fail the whole batch"
    );
    unsafe { calm_engine_free(e) };
}

#[test]
fn paste_encoded_images_reports_count_and_a_failed_outcome_on_bad_bytes() {
    let (_dir, e) = engine();
    let name = CString::new("Paste").unwrap();
    let id = unsafe { calm_project_create(e, name.as_ptr(), 8, 8) };
    assert!(!id.is_null());
    unsafe { calm_string_free(id) };

    let a = encode_rgba(&[9u8, 9, 9, 255].repeat(4), 2, 2, RasterFormat::Png).unwrap();
    let b = encode_rgba(&[8u8, 8, 8, 255].repeat(4), 2, 2, RasterFormat::Png).unwrap();
    let images = [
        CalmEncodedImage {
            name: ptr::null(),
            bytes: a.as_ptr(),
            len: a.len(),
        },
        CalmEncodedImage {
            name: ptr::null(),
            bytes: b.as_ptr(),
            len: b.len(),
        },
    ];
    let mut count = 0u32;
    let mut outcome = 0u32;
    assert_eq!(
        unsafe {
            calm_engine_paste_encoded_images(
                e,
                images.as_ptr(),
                images.len(),
                &mut count,
                &mut outcome,
            )
        },
        CalmStatus::Ok
    );
    assert_eq!(count, 2, "both landed as their own layer");

    let junk = CalmEncodedImage {
        name: ptr::null(),
        bytes: [9u8, 9, 9].as_ptr(),
        len: 3,
    };
    let mut failed_outcome = 5u32;
    assert_eq!(
        unsafe {
            calm_engine_paste_encoded_images(e, &junk, 1, ptr::null_mut(), &mut failed_outcome)
        },
        CalmStatus::Error
    );
    assert_eq!(failed_outcome, 0, "PasteOutcome::Failed is 0");
    assert_eq!(
        unsafe {
            calm_engine_paste_encoded_images(
                ptr::null_mut(),
                images.as_ptr(),
                1,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        },
        CalmStatus::Null
    );
    unsafe { calm_engine_free(e) };
}
