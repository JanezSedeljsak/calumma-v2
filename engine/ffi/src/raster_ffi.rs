use crate::engine::{cstring, with_inner, write_boxed, CalmEngine, CalmStatus, Inner};
use anyhow::{bail, Context};
use calumma_core::paste::{PasteImage, PasteOutcome};
use calumma_io::{decode_encoded, encode_rgba, RasterFormat};
use parking_lot::Mutex;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

#[repr(C)]
pub struct CalmEncodedImage {
    pub name: *const c_char,
    pub bytes: *const u8,
    pub len: usize,
}

struct DecodedImage {
    name: String,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn decode_bytes(bytes: *const u8, len: usize) -> Option<(u32, u32, Vec<u8>)> {
    if bytes.is_null() || len == 0 {
        return None;
    }
    decode_encoded(unsafe { std::slice::from_raw_parts(bytes, len) })
}

fn name_from_ptr(name: *const c_char) -> String {
    if name.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(name) }
        .to_str()
        .unwrap_or_default()
        .to_string()
}

fn decode_list(images: *const CalmEncodedImage, count: usize) -> Option<Vec<DecodedImage>> {
    if count == 0 || images.is_null() {
        return None;
    }
    let slice = unsafe { std::slice::from_raw_parts(images, count) };
    let mut out = Vec::with_capacity(count);
    for raw in slice {
        let (width, height, rgba) = decode_bytes(raw.bytes, raw.len)?;
        out.push(DecodedImage {
            name: name_from_ptr(raw.name),
            width,
            height,
            rgba,
        });
    }
    Some(out)
}

fn export_encoded(
    engine: *mut CalmEngine,
    layer_index: Option<u32>,
    format: u32,
    out_bytes: *mut *mut u8,
    out_len: *mut usize,
) -> CalmStatus {
    if out_bytes.is_null() || out_len.is_null() {
        return CalmStatus::Null;
    }
    unsafe {
        *out_bytes = ptr::null_mut();
        *out_len = 0;
    }
    let Some(format) = RasterFormat::from_u32(format) else {
        return CalmStatus::Error;
    };
    with_inner(engine, |inner| {
        let doc = inner.doc.as_ref().context("no project is open")?;
        let (width, height, rgba) = match layer_index {
            Some(index) => doc
                .layer_rgba(index as usize)
                .context("layer has no raster content")?,
            None => doc.composite_rgba(),
        };
        let bytes =
            encode_rgba(&rgba, width, height, format).context("encoding the raster failed")?;
        write_boxed(bytes, out_bytes, out_len);
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_image_decode(
    bytes: *const u8,
    len: usize,
    out_rgba: *mut *mut u8,
    out_len: *mut usize,
    out_w: *mut u32,
    out_h: *mut u32,
) -> CalmStatus {
    if bytes.is_null()
        || out_rgba.is_null()
        || out_len.is_null()
        || out_w.is_null()
        || out_h.is_null()
    {
        return CalmStatus::Null;
    }
    unsafe {
        *out_rgba = ptr::null_mut();
        *out_len = 0;
        *out_w = 0;
        *out_h = 0;
    }
    let Some((width, height, rgba)) = decode_bytes(bytes, len) else {
        return CalmStatus::Error;
    };
    unsafe {
        *out_w = width;
        *out_h = height;
    }
    write_boxed(rgba, out_rgba, out_len);
    CalmStatus::Ok
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_export_image(
    engine: *mut CalmEngine,
    format: u32,
    out_bytes: *mut *mut u8,
    out_len: *mut usize,
) -> CalmStatus {
    export_encoded(engine, None, format, out_bytes, out_len)
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_export_layer_image(
    engine: *mut CalmEngine,
    layer_index: u32,
    format: u32,
    out_bytes: *mut *mut u8,
    out_len: *mut usize,
) -> CalmStatus {
    export_encoded(engine, Some(layer_index), format, out_bytes, out_len)
}

#[no_mangle]
pub unsafe extern "C" fn calm_project_create_from_encoded(
    engine: *mut CalmEngine,
    name: *const c_char,
    bytes: *const u8,
    len: usize,
) -> *mut c_char {
    let image = CalmEncodedImage {
        name: ptr::null(),
        bytes,
        len,
    };
    calm_project_create_from_encoded_images(engine, name, &image, 1)
}

#[no_mangle]
pub unsafe extern "C" fn calm_project_create_from_encoded_images(
    engine: *mut CalmEngine,
    name: *const c_char,
    images: *const CalmEncodedImage,
    count: usize,
) -> *mut c_char {
    if engine.is_null() || name.is_null() {
        return ptr::null_mut();
    }
    let Some(decoded) = decode_list(images, count) else {
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

#[no_mangle]
pub unsafe extern "C" fn calm_engine_paste_encoded(
    engine: *mut CalmEngine,
    bytes: *const u8,
    len: usize,
    out_outcome: *mut u32,
) -> CalmStatus {
    let image = CalmEncodedImage {
        name: ptr::null(),
        bytes,
        len,
    };
    calm_engine_paste_encoded_images(engine, &image, 1, ptr::null_mut(), out_outcome)
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_paste_encoded_images(
    engine: *mut CalmEngine,
    images: *const CalmEncodedImage,
    count: usize,
    out_count: *mut u32,
    out_outcome: *mut u32,
) -> CalmStatus {
    if engine.is_null() {
        return CalmStatus::Null;
    }
    let Some(decoded) = decode_list(images, count) else {
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
            bail!("pasting encoded image(s) as new layer(s) failed");
        }
        inner.edited();
        Ok(())
    })
}
