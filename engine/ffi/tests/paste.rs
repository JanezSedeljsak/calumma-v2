//! Pasting across the boundary.
//!
//! Two things can only go wrong here rather than in core. The bytes arrive **premultiplied**
//! and have to be unpremultiplied before anything looks at them, and the outcome the shell
//! needs (so it can say "scaled to fit" and offer the other mode) has to survive the trip out
//! through a raw pointer that is allowed to be null.

use calumma_core::paste::PasteOutcome;
use calumma_ffi::*;
use std::ffi::CString;
use std::ptr;

const SIDE: u32 = 64;

fn engine_with_project() -> (tempfile::TempDir, *mut CalmEngine) {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("p.sqlite").to_str().unwrap()).unwrap();
    let e = unsafe { calm_engine_new(path.as_ptr()) };
    assert!(!e.is_null());
    let name = CString::new("Paste").unwrap();
    let id = unsafe { calm_project_create(e, name.as_ptr(), SIDE, SIDE) };
    assert!(!id.is_null());
    unsafe { calm_string_free(id) };
    (dir, e)
}

fn state(e: *mut CalmEngine) -> CalmState {
    let mut out = unsafe { std::mem::zeroed::<CalmState>() };
    assert_eq!(unsafe { calm_engine_state(e, &mut out) }, CalmStatus::Ok);
    out
}

fn paste(e: *mut CalmEngine, w: u32, h: u32) -> (CalmStatus, u32) {
    let rgba = [255u8, 0, 0, 255].repeat((w * h) as usize);
    let mut outcome = u32::MAX;
    let status =
        unsafe { calm_engine_paste_image(e, rgba.as_ptr(), rgba.len(), w, h, &mut outcome) };
    (status, outcome)
}

#[test]
fn an_image_that_fits_reports_native() {
    let (_dir, e) = engine_with_project();
    let (status, outcome) = paste(e, 16, 16);
    assert_eq!(status, CalmStatus::Ok);
    assert_eq!(outcome, PasteOutcome::Native.into());
    assert_eq!(state(e).width, SIDE, "the paper did not move");
    unsafe { calm_engine_free(e) };
}

#[test]
fn an_oversized_image_overflows_and_says_so() {
    let (_dir, e) = engine_with_project();
    let (status, outcome) = paste(e, 400, 400);
    assert_eq!(status, CalmStatus::Ok);
    assert_eq!(outcome, PasteOutcome::Overflowing.into());
    let after = state(e);
    assert_eq!(
        (after.width, after.height),
        (SIDE, SIDE),
        "a paste never resizes the canvas"
    );
    unsafe { calm_engine_free(e) };
}

#[test]
fn the_outcome_slot_is_allowed_to_be_null() {
    let (_dir, e) = engine_with_project();
    let rgba = [255u8, 0, 0, 255].repeat(16 * 16);
    assert_eq!(
        unsafe { calm_engine_paste_image(e, rgba.as_ptr(), rgba.len(), 16, 16, ptr::null_mut()) },
        CalmStatus::Ok
    );
    unsafe { calm_engine_free(e) };
}

/// A failed paste still writes the outcome, because "it did not work" is exactly what the
/// shell needs in order not to claim it did.
#[test]
fn a_failed_paste_reports_failed_rather_than_leaving_the_slot_alone() {
    let (_dir, e) = engine_with_project();
    let rgba = [0u8; 16];
    let mut outcome = u32::MAX;
    let status =
        unsafe { calm_engine_paste_image(e, rgba.as_ptr(), rgba.len(), 64, 64, &mut outcome) };
    assert_eq!(status, CalmStatus::Error);
    assert_eq!(outcome, PasteOutcome::Failed.into());
    unsafe { calm_engine_free(e) };
}

/// The bytes arrive premultiplied and are unpremultiplied on the way in, so a half-opaque red
/// has to come back out as full-strength red at half alpha rather than as a dark red.
#[test]
fn premultiplied_bytes_are_undone_at_the_boundary() {
    let (_dir, e) = engine_with_project();
    let rgba = [128u8, 0, 0, 128].repeat(8 * 8);
    let mut outcome = u32::MAX;
    assert_eq!(
        unsafe { calm_engine_paste_image(e, rgba.as_ptr(), rgba.len(), 8, 8, &mut outcome) },
        CalmStatus::Ok
    );
    let layer = state(e).active_layer;
    let mut buf: *mut u8 = ptr::null_mut();
    let mut w = 0u32;
    let mut h = 0u32;
    assert_eq!(
        unsafe { calm_engine_layer_rgba(e, layer, &mut buf, &mut w, &mut h) },
        CalmStatus::Ok
    );
    let pixels = unsafe { std::slice::from_raw_parts(buf, (w * h * 4) as usize) };
    let px = &pixels[0..4];
    assert!(px[0] > 250, "red came back to full strength: {px:?}");
    assert_eq!(px[3], 128);
    unsafe { calm_buffer_free(buf, (w * h * 4) as usize) };
    unsafe { calm_engine_free(e) };
}

#[test]
fn batch_paste_adds_every_image_in_one_call() {
    let (_dir, e) = engine_with_project();
    let one = [255u8, 0, 0, 255].repeat(8 * 8);
    let two = [0u8, 255, 0, 255].repeat(8 * 8);
    let images = [
        CalmPasteImage {
            name: std::ptr::null(),
            premultiplied_rgba: one.as_ptr(),
            len: one.len(),
            width: 8,
            height: 8,
        },
        CalmPasteImage {
            name: std::ptr::null(),
            premultiplied_rgba: two.as_ptr(),
            len: two.len(),
            width: 8,
            height: 8,
        },
    ];
    let mut pasted = 0u32;
    let mut outcome = u32::MAX;
    assert_eq!(
        unsafe {
            calm_engine_paste_images(e, images.as_ptr(), images.len(), &mut pasted, &mut outcome)
        },
        CalmStatus::Ok
    );
    assert_eq!(pasted, 2);
    assert_eq!(outcome, PasteOutcome::Native.into());
    let after = state(e);
    assert_eq!(after.layer_count, 4);
    unsafe { calm_engine_free(e) };
}

#[test]
fn the_stagger_constant_is_exposed_for_the_shell() {
    assert_eq!(
        calm_paste_stagger_px(),
        calumma_core::limits::PASTE_STAGGER_PX as u32
    );
}

#[test]
fn paste_images_rejects_a_null_engine_or_empty_batch() {
    let rgba = [255u8, 0, 0, 255];
    let image = CalmPasteImage {
        name: ptr::null(),
        premultiplied_rgba: rgba.as_ptr(),
        len: rgba.len(),
        width: 1,
        height: 1,
    };
    let mut outcome = 0u32;
    assert_eq!(
        unsafe {
            calm_engine_paste_images(ptr::null_mut(), &image, 1, ptr::null_mut(), &mut outcome)
        },
        CalmStatus::Null
    );
    assert_eq!(
        unsafe {
            calm_engine_paste_images(
                ptr::null_mut(),
                ptr::null(),
                0,
                ptr::null_mut(),
                &mut outcome,
            )
        },
        CalmStatus::Null
    );

    let (_dir, e) = engine_with_project();
    assert_eq!(
        unsafe { calm_engine_paste_images(e, ptr::null(), 0, ptr::null_mut(), &mut outcome) },
        CalmStatus::Error
    );
    assert_eq!(outcome, PasteOutcome::Failed.into());
    unsafe { calm_engine_free(e) };
}

#[test]
fn paste_images_without_a_project_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("p.sqlite").to_str().unwrap()).unwrap();
    let e = unsafe { calm_engine_new(path.as_ptr()) };
    let rgba = [255u8, 0, 0, 255];
    let image = CalmPasteImage {
        name: ptr::null(),
        premultiplied_rgba: rgba.as_ptr(),
        len: rgba.len(),
        width: 1,
        height: 1,
    };
    let mut outcome = 0u32;
    assert_eq!(
        unsafe { calm_engine_paste_images(e, &image, 1, ptr::null_mut(), &mut outcome) },
        CalmStatus::Error
    );
    unsafe { calm_engine_free(e) };
}

#[test]
fn a_batch_with_one_bad_image_fails_the_whole_call() {
    let (_dir, e) = engine_with_project();
    let good = [255u8, 0, 0, 255].repeat(4);
    let bad = CalmPasteImage {
        name: ptr::null(),
        premultiplied_rgba: ptr::null(),
        len: 0,
        width: 2,
        height: 2,
    };
    let images = [
        CalmPasteImage {
            name: ptr::null(),
            premultiplied_rgba: good.as_ptr(),
            len: good.len(),
            width: 2,
            height: 2,
        },
        bad,
    ];
    let mut outcome = 0u32;
    assert_eq!(
        unsafe {
            calm_engine_paste_images(
                e,
                images.as_ptr(),
                images.len(),
                ptr::null_mut(),
                &mut outcome,
            )
        },
        CalmStatus::Error
    );
    assert_eq!(outcome, PasteOutcome::Failed.into());
    unsafe { calm_engine_free(e) };
}

#[test]
fn single_image_paste_rejects_null_pixels() {
    assert_eq!(
        unsafe { calm_engine_paste_image(ptr::null_mut(), ptr::null(), 0, 1, 1, ptr::null_mut()) },
        CalmStatus::Null
    );
    let (_dir, e) = engine_with_project();
    assert_eq!(
        unsafe { calm_engine_paste_image(e, ptr::null(), 0, 1, 1, ptr::null_mut()) },
        CalmStatus::Null
    );
    unsafe { calm_engine_free(e) };
}
