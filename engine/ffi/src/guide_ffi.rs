use crate::engine::{read_doc, with_inner, CalmEngine, CalmStatus};
use anyhow::Context;
use calumma_core::GuideAxis;
use std::os::raw::c_int;

/// Guides pulled off the board itself go through `calm_engine_pointer_*` like every other Move
/// gesture. These entry points exist for the *rulers*, which are shell views: once a drag starts
/// on a ruler strip the pointer events belong to it, not to the board, so the ruler has to drive
/// the same drag by hand. Coordinates are board screen points — a drag still inside the ruler is
/// simply a negative one, which is what makes releasing it there discard the guide.
#[no_mangle]
pub extern "C" fn calm_engine_guide_drag_from_ruler(
    engine: *mut CalmEngine,
    axis: u8,
    x: f32,
    y: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let axis = GuideAxis::from_u8(axis).context("unknown guide axis")?;
        let doc = inner.doc.as_mut().context("no project is open")?;
        if doc.begin_guide_drag_from_ruler(axis, x, y) {
            inner.invalidate_renderer();
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn calm_engine_guide_drag_update(
    engine: *mut CalmEngine,
    x: f32,
    y: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            if doc.update_guide_drag(x, y) {
                inner.invalidate_renderer();
            }
        }
        Ok(())
    })
}

/// Takes the release position so the ruler never has to send a last update of its own; the
/// engine decides from where the guide landed whether it survives.
#[no_mangle]
pub extern "C" fn calm_engine_guide_drag_end(
    engine: *mut CalmEngine,
    x: f32,
    y: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.update_guide_drag(x, y);
            if doc.end_guide_drag() {
                inner.edited();
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn calm_engine_clear_guides(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            if doc.clear_guides() {
                inner.edited();
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn calm_engine_guide_count(engine: *mut CalmEngine) -> usize {
    read_doc(engine, 0, |doc| doc.guides().len())
}

/// The axis of the guide under a board point, or -1 for none — the one thing the cursor needs
/// to know to offer a grab.
#[no_mangle]
pub extern "C" fn calm_engine_guide_axis_at(engine: *mut CalmEngine, x: f32, y: f32) -> c_int {
    read_doc(engine, -1, |doc| {
        doc.guide_at(x, y)
            .and_then(|index| doc.guides().get(index))
            .map_or(-1, |guide| u8::from(guide.axis) as c_int)
    })
}
