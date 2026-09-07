use crate::engine::{read_doc, with_inner, CalmEngine, CalmStatus};
use anyhow::{bail, Context};
use std::os::raw::c_int;

#[no_mangle]
pub extern "C" fn calm_engine_create_clipping_mask(
    engine: *mut CalmEngine,
    index: u32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        if !doc.create_clipping_mask(index as usize) {
            bail!("layer {index} cannot create a clipping mask");
        }
        inner.edited();
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn calm_engine_release_clipping_mask(
    engine: *mut CalmEngine,
    index: u32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        if !doc.release_clipping_mask(index as usize) {
            bail!("layer {index} is not clipped");
        }
        inner.edited();
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn calm_engine_layer_is_clipped(engine: *mut CalmEngine, index: u32) -> c_int {
    read_doc(engine, 0, |doc| c_int::from(doc.is_layer_clipped(index as usize)))
}

#[no_mangle]
pub extern "C" fn calm_engine_layer_is_clip_base(engine: *mut CalmEngine, index: u32) -> c_int {
    read_doc(engine, 0, |doc| c_int::from(doc.is_layer_clip_base(index as usize)))
}

#[no_mangle]
pub extern "C" fn calm_engine_layer_can_create_clipping_mask(
    engine: *mut CalmEngine,
    index: u32,
) -> c_int {
    read_doc(engine, 0, |doc| {
        c_int::from(doc.can_create_clipping_mask(index as usize))
    })
}
