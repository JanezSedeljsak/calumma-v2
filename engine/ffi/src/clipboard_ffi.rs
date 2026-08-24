use crate::engine::{with_inner, write_boxed, CalmEngine, CalmStatus, Inner};
use anyhow::Context;
use calumma_core::{format_hex_rgb, parse_hex_rgb, unpack_rgb, Tool};
use calumma_io::encode_png_rgba;
use parking_lot::Mutex;
use std::ffi::{c_char, CStr, CString};
use std::os::raw::c_int;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

pub const CLIPBOARD_PNG: u32 = 0;
pub const CLIPBOARD_SVG: u32 = 1;

fn copy_payload(
    doc: &calumma_core::Document,
    layer_index: Option<usize>,
) -> anyhow::Result<(Vec<u8>, u32)> {
    if let Some(index) = layer_index {
        if let Some(svg) = doc.layer_svg(index) {
            return Ok((svg.into_bytes(), CLIPBOARD_SVG));
        }
        let (w, h, rgba) = doc
            .layer_rgba(index)
            .context("layer has no raster content")?;
        let png = encode_png_rgba(&rgba, w, h)?;
        return Ok((png, CLIPBOARD_PNG));
    }
    if let Some((w, h, rgba)) = doc.selection_rgba() {
        let png = encode_png_rgba(&rgba, w, h)?;
        return Ok((png, CLIPBOARD_PNG));
    }
    let (w, h, rgba) = doc.composite_rgba();
    let png = encode_png_rgba(&rgba, w, h)?;
    Ok((png, CLIPBOARD_PNG))
}

fn write_copy(
    engine: *mut CalmEngine,
    layer_index: Option<usize>,
    cut: bool,
    out: *mut *mut u8,
    out_len: *mut usize,
    out_kind: *mut u32,
) -> CalmStatus {
    if out.is_null() || out_len.is_null() || out_kind.is_null() {
        return CalmStatus::Null;
    }
    unsafe {
        *out = ptr::null_mut();
        *out_len = 0;
        *out_kind = CLIPBOARD_PNG;
    }
    with_inner(engine, |inner| {
        let (bytes, kind, did_cut) = {
            let doc = inner.doc.as_mut().context("no project is open")?;
            if cut && doc.selection.is_none() {
                anyhow::bail!("nothing to cut");
            }
            let (bytes, kind) = copy_payload(doc, layer_index)?;
            if cut {
                doc.clear_selection_pixels();
            }
            (bytes, kind, cut)
        };
        if did_cut {
            inner.edited();
        }
        write_boxed(bytes, out, out_len);
        unsafe {
            *out_kind = kind;
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_copy(
    engine: *mut CalmEngine,
    out: *mut *mut u8,
    out_len: *mut usize,
    out_kind: *mut u32,
) -> CalmStatus {
    write_copy(engine, None, false, out, out_len, out_kind)
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_cut(
    engine: *mut CalmEngine,
    out: *mut *mut u8,
    out_len: *mut usize,
    out_kind: *mut u32,
) -> CalmStatus {
    write_copy(engine, None, true, out, out_len, out_kind)
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_copy_layer(
    engine: *mut CalmEngine,
    layer_index: u32,
    out: *mut *mut u8,
    out_len: *mut usize,
    out_kind: *mut u32,
) -> CalmStatus {
    write_copy(
        engine,
        Some(layer_index as usize),
        false,
        out,
        out_len,
        out_kind,
    )
}

#[no_mangle]
pub extern "C" fn calm_tool_is_shape(tool: u32) -> u8 {
    Tool::from_u32(tool).is_some_and(Tool::is_shape) as u8
}

#[no_mangle]
pub extern "C" fn calm_tool_is_selection(tool: u32) -> u8 {
    Tool::from_u32(tool).is_some_and(Tool::is_selection) as u8
}

#[no_mangle]
pub extern "C" fn calm_tool_takes_fill(tool: u32) -> u8 {
    Tool::from_u32(tool).is_some_and(Tool::takes_fill) as u8
}

#[no_mangle]
pub extern "C" fn calm_tool_takes_brush_size(tool: u32) -> u8 {
    Tool::from_u32(tool).is_some_and(Tool::takes_brush_size) as u8
}

#[no_mangle]
pub extern "C" fn calm_tool_takes_ink_opacity(tool: u32) -> u8 {
    Tool::from_u32(tool).is_some_and(Tool::takes_ink_opacity) as u8
}

#[no_mangle]
pub extern "C" fn calm_tool_shows_vector_mode(tool: u32) -> u8 {
    Tool::from_u32(tool).is_some_and(Tool::shows_vector_mode) as u8
}

#[no_mangle]
pub extern "C" fn calm_tool_takes_blur_strength(tool: u32) -> u8 {
    Tool::from_u32(tool).is_some_and(Tool::takes_blur_strength) as u8
}

#[no_mangle]
pub extern "C" fn calm_tool_takes_tolerance(tool: u32) -> u8 {
    Tool::from_u32(tool).is_some_and(Tool::takes_tolerance) as u8
}

#[no_mangle]
pub extern "C" fn calm_tool_takes_eyedropper_radius(tool: u32) -> u8 {
    Tool::from_u32(tool).is_some_and(Tool::takes_eyedropper_radius) as u8
}

#[no_mangle]
pub extern "C" fn calm_tool_takes_brush(tool: u32) -> u8 {
    Tool::from_u32(tool).is_some_and(Tool::takes_brush) as u8
}

#[no_mangle]
pub extern "C" fn calm_tool_takes_eraser_hardness(tool: u32) -> u8 {
    Tool::from_u32(tool).is_some_and(Tool::takes_eraser_hardness) as u8
}

#[no_mangle]
pub unsafe extern "C" fn calm_parse_hex_rgb(s: *const c_char, out_rgb: *mut u32) -> CalmStatus {
    if s.is_null() || out_rgb.is_null() {
        return CalmStatus::Null;
    }
    let Ok(text) = unsafe { CStr::from_ptr(s) }.to_str() else {
        return CalmStatus::Error;
    };
    let Some(rgb) = parse_hex_rgb(text) else {
        return CalmStatus::Error;
    };
    unsafe {
        *out_rgb = calumma_core::pack_rgb(rgb);
    }
    CalmStatus::Ok
}

#[no_mangle]
pub unsafe extern "C" fn calm_format_hex_rgb(rgb: u32) -> *mut c_char {
    let hex = format_hex_rgb(unpack_rgb(rgb));
    CString::new(hex)
        .map(CString::into_raw)
        .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn calm_lossy_export_quality() -> f32 {
    calumma_core::LOSSY_EXPORT_QUALITY
}

#[no_mangle]
pub extern "C" fn calm_ink_opacity_min() -> f32 {
    calumma_core::limits::INK_OPACITY_MIN
}

#[no_mangle]
pub extern "C" fn calm_ink_opacity_max() -> f32 {
    calumma_core::limits::INK_OPACITY_MAX
}

#[no_mangle]
pub extern "C" fn calm_ink_opacity_default() -> f32 {
    calumma_core::limits::INK_OPACITY_DEFAULT
}

#[no_mangle]
pub extern "C" fn calm_blur_strength_min() -> f32 {
    calumma_core::limits::BLUR_STRENGTH_MIN
}

#[no_mangle]
pub extern "C" fn calm_blur_strength_max() -> f32 {
    calumma_core::limits::BLUR_STRENGTH_MAX
}

#[no_mangle]
pub extern "C" fn calm_blur_strength_default() -> f32 {
    calumma_core::limits::BLUR_STRENGTH_DEFAULT
}

#[no_mangle]
pub extern "C" fn calm_eraser_hardness_min() -> f32 {
    calumma_core::limits::ERASER_HARDNESS_MIN
}

#[no_mangle]
pub extern "C" fn calm_eraser_hardness_max() -> f32 {
    calumma_core::limits::ERASER_HARDNESS_MAX
}

#[no_mangle]
pub extern "C" fn calm_eraser_hardness_default() -> f32 {
    calumma_core::limits::ERASER_HARDNESS_DEFAULT
}

#[no_mangle]
pub extern "C" fn calm_tolerance_min() -> u8 {
    calumma_core::limits::TOLERANCE_MIN
}

#[no_mangle]
pub extern "C" fn calm_tolerance_max() -> u8 {
    calumma_core::limits::TOLERANCE_MAX
}

#[no_mangle]
pub extern "C" fn calm_tolerance_default() -> u8 {
    calumma_core::limits::TOLERANCE_DEFAULT
}

#[no_mangle]
pub extern "C" fn calm_eyedropper_radius_min() -> u32 {
    calumma_core::limits::EYEDROPPER_RADIUS_MIN
}

#[no_mangle]
pub extern "C" fn calm_eyedropper_radius_max() -> u32 {
    calumma_core::limits::EYEDROPPER_RADIUS_MAX
}

#[no_mangle]
pub extern "C" fn calm_eyedropper_radius_default() -> u32 {
    calumma_core::limits::EYEDROPPER_RADIUS_DEFAULT
}

#[no_mangle]
pub extern "C" fn calm_engine_nudge_move_target(
    engine: *mut CalmEngine,
    steps_x: f32,
    steps_y: f32,
) -> c_int {
    if engine.is_null() {
        return 0;
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let mut inner = mutex.lock();
        let nudged = {
            let doc = inner.doc.as_mut()?;
            doc.nudge_move_target(steps_x, steps_y)
        };
        if nudged {
            inner.edited();
        }
        Some(c_int::from(nudged))
    })) {
        Ok(Some(v)) => v,
        _ => 0,
    }
}
