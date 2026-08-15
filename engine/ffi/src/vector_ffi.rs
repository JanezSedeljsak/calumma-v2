use crate::engine::{read_doc, with_inner, CalmEngine, CalmStatus};
use anyhow::Context;
use std::os::raw::c_int;

/// None of these entry points touch a raw pointer themselves — `with_inner` and `read_doc`
/// own the null check, the lock and the `catch_unwind` — so none of them is an `unsafe fn`.
/// Keeping the qualifier off is not cosmetic: it means a future edit that *does* reach for a
/// pointer has to say so.
#[no_mangle]
pub extern "C" fn calm_engine_set_vector_mode(engine: *mut CalmEngine, on: u8) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.set_vector_mode(on != 0);
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn calm_engine_vector_mode(engine: *mut CalmEngine) -> c_int {
    read_doc(engine, 0, |doc| c_int::from(doc.vector_mode))
}

#[no_mangle]
pub extern "C" fn calm_engine_layer_is_vector(engine: *mut CalmEngine, index: u32) -> c_int {
    read_doc(engine, -1, |doc| match doc.layers.get(index as usize) {
        Some(layer) => c_int::from(layer.content.is_vector()),
        None => -1,
    })
}

/// How many items a vector layer holds, so the layers panel can say what a layer *is* rather
/// than only what it is called.
#[no_mangle]
pub extern "C" fn calm_engine_layer_item_count(engine: *mut CalmEngine, index: u32) -> u32 {
    read_doc(engine, 0, |doc| {
        doc.vector_item_count(index as usize) as u32
    })
}

/// The selected item's index, or -1 when nothing is selected. Which layer it belongs to is
/// not a second question: selecting an item makes its layer active, and activating another
/// layer drops the selection, so `CalmState.active_layer` is always the answer.
#[no_mangle]
pub extern "C" fn calm_engine_selected_vector_item(engine: *mut CalmEngine) -> c_int {
    read_doc(engine, -1, |doc| {
        doc.selected_vector_item()
            .map_or(-1, |pick| pick.item as c_int)
    })
}

#[no_mangle]
pub extern "C" fn calm_engine_clear_vector_selection(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.clear_vector_selection();
            inner.invalidate_renderer();
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn calm_engine_delete_selected_vector_item(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        if doc.delete_selected_vector_item() {
            inner.edited();
        }
        Ok(())
    })
}

/// Arrow-key move of the selected item. The shell passes a direction in steps; the step size
/// is core's, like every other product constant.
#[no_mangle]
pub extern "C" fn calm_engine_nudge_selected_vector_item(
    engine: *mut CalmEngine,
    steps_x: f32,
    steps_y: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        if doc.nudge_selected_vector_item(steps_x, steps_y) {
            inner.edited();
        }
        Ok(())
    })
}
