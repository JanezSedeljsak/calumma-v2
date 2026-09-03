use crate::engine::{cstring, with_inner, CalmEngine, CalmStatus};
use anyhow::{bail, Context};
use calumma_core::{
    font_family_at, font_family_count, Step, TextAlign, TEXT_LINE_HEIGHT_DEFAULT,
    TEXT_LINE_HEIGHT_MAX, TEXT_LINE_HEIGHT_MIN, TEXT_SIZE_DEFAULT, TEXT_SIZE_MAX, TEXT_SIZE_MIN,
};
use std::ffi::{c_char, CStr};
use std::os::raw::c_int;
use std::ptr;

/// The font list is read by index rather than as one array: the shell wants a row at a time
/// for a lazily rendered picker, and the engine already holds the enumeration for the whole
/// session, so neither side ever copies the list whole.
#[no_mangle]
pub extern "C" fn calm_font_family_count() -> u32 {
    font_family_count() as u32
}

#[no_mangle]
pub extern "C" fn calm_font_family_name(index: u32) -> *mut c_char {
    match font_family_at(index as usize) {
        Some(family) => cstring(&family.name),
        None => ptr::null_mut(),
    }
}

/// Which cuts of a family the system actually ships, as a bitmask: 1 = bold, 2 = italic.
/// A picker greys out B and I for a family that has neither rather than letting cosmic-text
/// answer with a synthesised face that is not the font anyone chose.
pub const FONT_STYLE_BOLD: u32 = 1;
pub const FONT_STYLE_ITALIC: u32 = 2;

#[no_mangle]
pub extern "C" fn calm_font_family_styles(index: u32) -> u32 {
    match font_family_at(index as usize) {
        Some(family) => {
            u32::from(family.bold) * FONT_STYLE_BOLD + u32::from(family.italic) * FONT_STYLE_ITALIC
        }
        None => 0,
    }
}

#[no_mangle]
pub extern "C" fn calm_text_size_min() -> f32 {
    TEXT_SIZE_MIN
}

#[no_mangle]
pub extern "C" fn calm_text_size_max() -> f32 {
    TEXT_SIZE_MAX
}

#[no_mangle]
pub extern "C" fn calm_text_size_default() -> f32 {
    TEXT_SIZE_DEFAULT
}

/// Text size rides the same slider curve as brush size — see `calm_brush_size_unit`.
#[no_mangle]
pub extern "C" fn calm_text_size_unit(size: f32) -> f32 {
    calumma_core::text_size_unit(size)
}

#[no_mangle]
pub extern "C" fn calm_text_size_from_unit(unit: f32) -> f32 {
    calumma_core::text_size_from_unit(unit)
}

/// The narrowest box that wraps as a box. Below it every word lands on its own line, so the
/// engine answers a narrower ask with no wrap at all rather than with a column of letters.
#[no_mangle]
pub extern "C" fn calm_text_wrap_min() -> f32 {
    calumma_core::TEXT_WRAP_MIN_WIDTH
}

/// The widest wrap worth offering: the paper. A box wider than the document can hold has
/// nothing to wrap against.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_text_wrap_max(engine: *mut CalmEngine) -> f32 {
    crate::engine::read_doc(engine, 0.0, |doc| doc.width as f32)
}

#[no_mangle]
pub extern "C" fn calm_text_line_height_min() -> f32 {
    TEXT_LINE_HEIGHT_MIN
}

#[no_mangle]
pub extern "C" fn calm_text_line_height_max() -> f32 {
    TEXT_LINE_HEIGHT_MAX
}

#[no_mangle]
pub extern "C" fn calm_text_line_height_default() -> f32 {
    TEXT_LINE_HEIGHT_DEFAULT
}

fn read_utf8(text: *const c_char) -> anyhow::Result<String> {
    if text.is_null() {
        return Ok(String::new());
    }
    Ok(unsafe { CStr::from_ptr(text) }
        .to_str()
        .context("text is not valid UTF-8")?
        .to_string())
}

/// Every text mutation redraws the board and marks the project dirty. Typing is the one
/// interaction where the pixel result has to land on the very next frame, so none of these
/// defer their invalidate.
fn edited(inner: &mut crate::engine::Inner) {
    inner.mark_dirty_save();
    inner.invalidate_renderer();
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_text_insert(
    engine: *mut CalmEngine,
    text: *const c_char,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let text = read_utf8(text)?;
        let doc = inner.doc.as_mut().context("no project is open")?;
        doc.text_insert(&text);
        edited(inner);
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_text_set_marked(
    engine: *mut CalmEngine,
    text: *const c_char,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let text = read_utf8(text)?;
        let doc = inner.doc.as_mut().context("no project is open")?;
        doc.text_set_marked(&text);
        edited(inner);
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_text_backspace(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        doc.text_backspace();
        edited(inner);
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_text_delete_forward(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        doc.text_delete_forward();
        edited(inner);
        Ok(())
    })
}

/// `extend` is shift: the anchor stays put and the selection grows. Without it the anchor
/// follows the caret, which is how a plain arrow key cancels a selection.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_text_move_caret(
    engine: *mut CalmEngine,
    step: u32,
    extend: c_int,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let step = Step::from_u32(step).with_context(|| format!("unknown caret step {step}"))?;
        let doc = inner.doc.as_mut().context("no project is open")?;
        doc.text_step_caret(step, extend != 0);
        inner.invalidate_renderer();
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_text_select_all(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        if !doc.text_select_all() {
            bail!("no text is being edited");
        }
        inner.invalidate_renderer();
        Ok(())
    })
}

/// Double- and triple-click, in board coordinates. Which bytes a word or a paragraph covers is
/// a question about the shaped layout, so the shell sends the click count and the point and
/// nothing else.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_text_select_word_at(
    engine: *mut CalmEngine,
    x: f32,
    y: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        let (dx, dy) = doc.camera.to_doc(x, y);
        doc.text_select_word_at(dx, dy);
        inner.invalidate_renderer();
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_text_select_paragraph_at(
    engine: *mut CalmEngine,
    x: f32,
    y: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        let (dx, dy) = doc.camera.to_doc(x, y);
        doc.text_select_paragraph_at(dx, dy);
        inner.invalidate_renderer();
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_text_has_selection(engine: *mut CalmEngine) -> c_int {
    crate::engine::read_doc(engine, 0, |doc| i32::from(doc.text_selection().is_some()))
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_text_commit(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        doc.commit_text();
        edited(inner);
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_text_edit_layer(
    engine: *mut CalmEngine,
    index: u32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        if !doc.edit_text_layer(index as usize) {
            bail!("layer {index} is not a text layer");
        }
        inner.invalidate_renderer();
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_text_editing(engine: *mut CalmEngine) -> c_int {
    crate::engine::read_doc(engine, -1, |doc| if doc.text_editing() { 1 } else { 0 })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_layer_is_text(engine: *mut CalmEngine, index: u32) -> c_int {
    crate::engine::read_doc(engine, -1, |doc| match doc.layers.get(index as usize) {
        Some(layer) if layer.is_text() => 1,
        Some(_) => 0,
        None => -1,
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_text_family(
    engine: *mut CalmEngine,
    family: *const c_char,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let family = read_utf8(family)?;
        let doc = inner.doc.as_mut().context("no project is open")?;
        if !doc.set_text_family(&family) {
            bail!("no font family named {family}");
        }
        edited(inner);
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_text_bold(
    engine: *mut CalmEngine,
    bold: c_int,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        doc.set_text_bold(bold != 0);
        edited(inner);
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_text_italic(
    engine: *mut CalmEngine,
    italic: c_int,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        doc.set_text_italic(italic != 0);
        edited(inner);
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_text_line_height(
    engine: *mut CalmEngine,
    line_height: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        doc.set_text_line_height(line_height);
        edited(inner);
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_text_size(
    engine: *mut CalmEngine,
    size: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        doc.set_text_size(size);
        edited(inner);
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_text_align(
    engine: *mut CalmEngine,
    align: u32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let align =
            TextAlign::from_u32(align).with_context(|| format!("unknown text align {align}"))?;
        let doc = inner.doc.as_mut().context("no project is open")?;
        doc.set_text_align(align);
        edited(inner);
        Ok(())
    })
}

/// How wide the block wraps. `0` means it does not — the run grows with its longest line —
/// and any width below `TEXT_WRAP_MIN_WIDTH` is refused the same way, because a box narrower
/// than that puts every word on a line of its own.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_text_wrap_width(
    engine: *mut CalmEngine,
    width: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        doc.set_text_wrap_width((width > 0.0).then_some(width));
        edited(inner);
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_text_wrap_width(engine: *mut CalmEngine) -> f32 {
    crate::engine::read_doc(engine, 0.0, |doc| doc.text_wrap_width().unwrap_or(0.0))
}

/// The family, size and alignment the shell shows: the run being edited when there is one,
/// otherwise the document's own defaults for the next text layer. Family, size and the style
/// bits are resolved *at the selection* — with one word selected the panel has to show that
/// word's font, not the block's.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_text_family(engine: *mut CalmEngine) -> *mut c_char {
    crate::engine::read_doc(engine, ptr::null_mut(), |doc| {
        cstring(&doc.active_text_style().family)
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_text_size(engine: *mut CalmEngine) -> f32 {
    crate::engine::read_doc(engine, TEXT_SIZE_DEFAULT, |doc| {
        doc.active_text_style().size
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_text_align(engine: *mut CalmEngine) -> u32 {
    crate::engine::read_doc(engine, 0, |doc| {
        doc.active_text_run()
            .map(|run| run.align)
            .unwrap_or(doc.text_style.align)
            .as_u32()
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_text_line_height(engine: *mut CalmEngine) -> f32 {
    crate::engine::read_doc(engine, TEXT_LINE_HEIGHT_DEFAULT, |doc| {
        doc.active_text_run()
            .map(|run| run.line_height)
            .unwrap_or(doc.text_style.line_height)
    })
}

/// Bold and italic of the run the shell is showing, as the same bitmask the family list
/// reports its available cuts in.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_text_styles(engine: *mut CalmEngine) -> u32 {
    crate::engine::read_doc(engine, 0, |doc| {
        let style = doc.active_text_style();
        u32::from(style.bold) * FONT_STYLE_BOLD + u32::from(style.italic) * FONT_STYLE_ITALIC
    })
}

/// The caret in *screen* coordinates, which is what an IME candidate window has to be
/// anchored to. The conversion is the camera's, so the shell never multiplies by zoom.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_text_caret_rect(
    engine: *mut CalmEngine,
    out_x: *mut f32,
    out_y: *mut f32,
    out_height: *mut f32,
) -> CalmStatus {
    if out_x.is_null() || out_y.is_null() || out_height.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let doc = inner.doc.as_ref().context("no project is open")?;
        let Some((top, bottom)) = doc.text_caret_segment() else {
            bail!("no text is being edited");
        };
        let (sx, sy) = doc.camera.to_screen(top.0, top.1);
        let (_, sy_bottom) = doc.camera.to_screen(bottom.0, bottom.1);
        unsafe {
            *out_x = sx;
            *out_y = sy;
            *out_height = (sy_bottom - sy).abs().max(1.0);
        }
        Ok(())
    })
}

/// The text of a layer, so the layers panel can label a text layer with what it says.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_layer_text(
    engine: *mut CalmEngine,
    index: u32,
) -> *mut c_char {
    crate::engine::read_doc(engine, ptr::null_mut(), |doc| {
        match doc.layers.get(index as usize).and_then(|l| l.run()) {
            Some(run) => cstring(&run.text),
            None => ptr::null_mut(),
        }
    })
}
