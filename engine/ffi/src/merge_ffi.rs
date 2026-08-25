use crate::engine::{read_doc, with_inner, CalmEngine, CalmStatus};
use anyhow::{bail, Context};
use std::os::raw::c_int;

#[no_mangle]
pub extern "C" fn calm_engine_merge_layer_down(engine: *mut CalmEngine, index: u32) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        if !doc.merge_layer_down(index as usize) {
            bail!("layer {index} cannot be merged down");
        }
        inner.edited();
        Ok(())
    })
}

/// Merge Down through the layer below's alpha. Destructive by design — there is no clipped
/// state to persist afterwards, so this is the whole feature.
#[no_mangle]
pub extern "C" fn calm_engine_clip_layer_down(engine: *mut CalmEngine, index: u32) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        if !doc.clip_layer_down(index as usize) {
            bail!("layer {index} cannot be clipped to the layer below");
        }
        inner.edited();
        Ok(())
    })
}

/// Whether the action is offered at all. The extra rules over Merge Down — a raster base with
/// no transform — are the engine's, so the shell greys the button out on the same answer the
/// engine would refuse the call with.
#[no_mangle]
pub extern "C" fn calm_engine_layer_can_clip_down(engine: *mut CalmEngine, index: u32) -> c_int {
    read_doc(engine, 0, |doc| {
        c_int::from(doc.can_clip_layer_down(index as usize))
    })
}
