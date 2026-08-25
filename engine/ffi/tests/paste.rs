//! Pasting across the boundary.
//!
//! Two things can only go wrong here rather than in core. The bytes arrive **premultiplied**
//! and have to be unpremultiplied before anything looks at them, and the outcome the shell
//! needs (so it can say "scaled to fit" and offer the other mode) has to survive the trip out
//! through a raw pointer that is allowed to be null.

use calumma_core::paste::{PasteFit, PasteOutcome};
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
fn an_oversized_image_scales_by_default_and_says_so() {
    let (_dir, e) = engine_with_project();
    assert_eq!(state(e).paste_fit, PasteFit::ScaleToFit.into());
    let (status, outcome) = paste(e, 400, 400);
    assert_eq!(status, CalmStatus::Ok);
    assert_eq!(outcome, PasteOutcome::Scaled.into());
    assert_eq!(state(e).width, SIDE, "scaling never touches the paper");
    unsafe { calm_engine_free(e) };
}

#[test]
fn switching_the_knob_grows_the_paper_instead() {
    let (_dir, e) = engine_with_project();
    assert_eq!(
        calm_engine_set_paste_fit(e, PasteFit::GrowCanvas.into()),
        CalmStatus::Ok
    );
    assert_eq!(state(e).paste_fit, PasteFit::GrowCanvas.into());
    let (status, outcome) = paste(e, 200, 200);
    assert_eq!(status, CalmStatus::Ok);
    assert_eq!(outcome, PasteOutcome::Grown.into());
    let after = state(e);
    assert_eq!((after.width, after.height), (200, 200));
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

#[test]
fn an_unknown_paste_fit_is_an_error_not_a_silent_default() {
    let (_dir, e) = engine_with_project();
    assert_eq!(calm_engine_set_paste_fit(e, 99), CalmStatus::Error);
    assert_eq!(
        state(e).paste_fit,
        PasteFit::ScaleToFit.into(),
        "and the knob is untouched"
    );
    assert_eq!(
        calm_engine_set_paste_fit(ptr::null_mut(), 0),
        CalmStatus::Null
    );
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
