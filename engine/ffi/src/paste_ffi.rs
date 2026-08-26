use crate::engine::{with_inner, CalmEngine, CalmStatus};
use anyhow::{bail, Context};
use calumma_core::paste::PasteOutcome;
use calumma_core::unpremultiply_rgba;

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
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        let mut rgba = unsafe { std::slice::from_raw_parts(premultiplied_rgba, len) }.to_vec();
        unpremultiply_rgba(&mut rgba);
        let n = doc.layers.len() + 1;
        let name = calumma_core::names::numbered_pasted_layer(n);
        let outcome = doc.paste_image_as_layer(name, &rgba, width, height);
        if !out_outcome.is_null() {
            unsafe { out_outcome.write(outcome.into()) };
        }
        if outcome == PasteOutcome::Failed {
            bail!("pasting a {width}x{height} image as a new layer failed");
        }
        inner.edited();
        Ok(())
    })
}
