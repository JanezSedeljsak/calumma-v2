use crate::engine::{cstring, with_inner, CalmEngine, CalmStatus, Inner};
use anyhow::{bail, Context};
use calumma_core::limits::IMPORT_MAX_SIDE;
use calumma_core::paste::{PasteImage, PasteOutcome};
use calumma_core::unpremultiply_rgba;
use parking_lot::Mutex;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

#[repr(C)]
pub struct CalmPasteImage {
    pub name: *const c_char,
    pub premultiplied_rgba: *const u8,
    pub len: usize,
    pub width: u32,
    pub height: u32,
}

struct DecodedPasteImage {
    name: String,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

fn decode_paste_image(raw: &CalmPasteImage) -> Option<DecodedPasteImage> {
    if raw.premultiplied_rgba.is_null() {
        return None;
    }
    let expected = (raw.width as usize)
        .checked_mul(raw.height as usize)
        .and_then(|pixels| pixels.checked_mul(4));
    if raw.width == 0
        || raw.height == 0
        || raw.width > IMPORT_MAX_SIDE
        || raw.height > IMPORT_MAX_SIDE
        || expected != Some(raw.len)
    {
        return None;
    }
    let mut rgba = unsafe { std::slice::from_raw_parts(raw.premultiplied_rgba, raw.len) }.to_vec();
    unpremultiply_rgba(&mut rgba);
    let name = if raw.name.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(raw.name) }
            .to_str()
            .unwrap_or_default()
            .to_string()
    };
    Some(DecodedPasteImage {
        name,
        rgba,
        width: raw.width,
        height: raw.height,
    })
}

fn decode_paste_images(
    images: *const CalmPasteImage,
    count: usize,
) -> Option<Vec<DecodedPasteImage>> {
    if count == 0 || images.is_null() {
        return None;
    }
    let slice = unsafe { std::slice::from_raw_parts(images, count) };
    let mut out = Vec::with_capacity(count);
    for raw in slice {
        out.push(decode_paste_image(raw)?);
    }
    Some(out)
}

#[no_mangle]
pub extern "C" fn calm_paste_stagger_px() -> u32 {
    calumma_core::limits::PASTE_STAGGER_PX as u32
}

/// Pastes an image as a new layer at native size, reporting whether it fit on the paper.
///
/// `out_outcome` is optional and receives a `calumma_core::paste::PasteOutcome` discriminant.
/// The shell needs it because an image that overflows is worth saying out loud — half of it is
/// off the canvas — and because it must never work that out by comparing sizes itself.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_paste_image(
    engine: *mut CalmEngine,
    premultiplied_rgba: *const u8,
    len: usize,
    width: u32,
    height: u32,
    out_outcome: *mut u32,
) -> CalmStatus {
    if engine.is_null() || premultiplied_rgba.is_null() {
        return CalmStatus::Null;
    }
    let image = CalmPasteImage {
        name: ptr::null(),
        premultiplied_rgba,
        len,
        width,
        height,
    };
    calm_engine_paste_images(engine, &image, 1, ptr::null_mut(), out_outcome)
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_paste_images(
    engine: *mut CalmEngine,
    images: *const CalmPasteImage,
    count: usize,
    out_count: *mut u32,
    out_outcome: *mut u32,
) -> CalmStatus {
    if engine.is_null() {
        return CalmStatus::Null;
    }
    let Some(decoded) = decode_paste_images(images, count) else {
        if !out_outcome.is_null() {
            unsafe { out_outcome.write(PasteOutcome::Failed.into()) };
        }
        return CalmStatus::Error;
    };
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        let payloads: Vec<PasteImage<'_>> = decoded
            .iter()
            .map(|image| PasteImage {
                name: image.name.as_str(),
                rgba: &image.rgba,
                width: image.width,
                height: image.height,
            })
            .collect();
        let (pasted, outcome) = doc.paste_images_as_layers(&payloads);
        if !out_count.is_null() {
            unsafe { out_count.write(pasted as u32) };
        }
        if !out_outcome.is_null() {
            unsafe { out_outcome.write(outcome.into()) };
        }
        if pasted == 0 {
            bail!("pasting {count} image(s) as new layer(s) failed");
        }
        inner.edited();
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_project_create_from_images(
    engine: *mut CalmEngine,
    name: *const c_char,
    images: *const CalmPasteImage,
    count: usize,
) -> *mut c_char {
    if engine.is_null() || name.is_null() {
        return ptr::null_mut();
    }
    let Some(decoded) = decode_paste_images(images, count) else {
        return ptr::null_mut();
    };
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let mut inner = mutex.lock();
        let name = unsafe { CStr::from_ptr(name) }
            .to_str()
            .unwrap_or(calumma_core::UNTITLED);
        let max_w = decoded.iter().map(|i| i.width).max().unwrap_or(0);
        let max_h = decoded.iter().map(|i| i.height).max().unwrap_or(0);
        if max_w == 0 || max_h == 0 {
            bail!("no images to import");
        }
        inner.close_document();
        let mut doc = inner
            .store
            .create(name, max_w, max_h)
            .with_context(|| format!("creating project {name} at {max_w}x{max_h}"))?;
        let payloads: Vec<PasteImage<'_>> = decoded
            .iter()
            .map(|image| PasteImage {
                name: image.name.as_str(),
                rgba: &image.rgba,
                width: image.width,
                height: image.height,
            })
            .collect();
        if doc.install_images_staggered(&payloads) == 0 {
            bail!("placing images into the new project failed");
        }
        inner.store.save(&mut doc).context("saving project")?;
        let id = doc.id.clone();
        inner.install_document(doc);
        Ok::<_, anyhow::Error>(cstring(&id))
    })) {
        Ok(Ok(p)) => p,
        _ => ptr::null_mut(),
    }
}
