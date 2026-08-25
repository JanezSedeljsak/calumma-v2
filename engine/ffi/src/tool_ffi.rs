use crate::engine::{read_doc, with_inner, CalmEngine, CalmStatus};
use anyhow::{bail, Context};
use calumma_core::{Tool, ToolBlock};
use std::os::raw::c_int;

/// The whole rule table in one call. The shell asks once per (active layer, layer flags,
/// vector mode) change and indexes the answer by tool discriminant — never per frame, and
/// never one lock per button.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_tool_blocks(
    engine: *mut CalmEngine,
    out: *mut u32,
    len: u32,
) -> u32 {
    if out.is_null() || len == 0 {
        return 0;
    }
    read_doc(engine, 0, |doc| {
        let mut written = 0;
        for value in 0..len {
            let block = match Tool::from_u32(value) {
                Some(tool) => doc.tool_block(tool),
                None => ToolBlock::None,
            };
            unsafe { out.add(value as usize).write(block.into()) };
            written = value + 1;
        }
        written
    })
}

#[no_mangle]
pub extern "C" fn calm_engine_tool_block(engine: *mut CalmEngine, tool: u32) -> u32 {
    read_doc(engine, 0, |doc| match Tool::from_u32(tool) {
        Some(tool) => doc.tool_block(tool).into(),
        None => 0,
    })
}

/// Takes the reason the last board press did nothing, clearing it. The shell turns it into one
/// toast; leaving it behind would mean saying the same thing again on the next sync.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_take_tool_block_notice(
    engine: *mut CalmEngine,
    out: *mut u32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if out.is_null() {
            bail!("no output slot for the tool block notice");
        }
        let block = inner
            .doc
            .as_mut()
            .and_then(|doc| doc.take_tool_block_notice())
            .unwrap_or_default();
        unsafe { out.write(block.into()) };
        Ok(())
    })
}

/// Whether the active layer pins vector mode on, so the toggle can show itself as locked
/// rather than as a knob the user is free to turn and then finds ignored.
#[no_mangle]
pub extern "C" fn calm_engine_vector_mode_locked(engine: *mut CalmEngine) -> c_int {
    read_doc(engine, 0, |doc| c_int::from(doc.vector_mode_locked()))
}

#[no_mangle]
pub extern "C" fn calm_engine_layer_is_rasterizable(engine: *mut CalmEngine, index: u32) -> c_int {
    read_doc(engine, 0, |doc| {
        c_int::from(doc.layer_is_rasterizable(index as usize))
    })
}

/// The way out of every block a live layer imposes: turn it into ordinary pixels. Works on a
/// text layer and on a vector one, so the panel offers one command rather than two.
#[no_mangle]
pub extern "C" fn calm_engine_rasterize_layer(engine: *mut CalmEngine, index: u32) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        if !doc.rasterize_layer(index as usize) {
            bail!("layer {index} is already pixels");
        }
        inner.edited();
        Ok(())
    })
}
