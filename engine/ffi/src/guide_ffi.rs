use crate::engine::{read_doc, with_inner, CalmEngine, CalmStatus};
use anyhow::Context;
use calumma_core::{pack_rgb, unpack_rgb, GuideAxis};
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
            inner.invalidate_overlay();
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
                // Overlay, not content. A guide that moved has changed no tile, no layer and no
                // camera, so the full `invalidate` this used to call — which drops the layer
                // cache, the pan cache and the overview along with it — was rebuilding the
                // entire board on every pointer move of the drag. That is what made dragging one
                // off a ruler feel like it was catching up rather than following.
                inner.invalidate_overlay();
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

/// The guide being dragged right now: its axis, its document position, and where that position
/// falls on the board in screen points. Returns 0 when nothing is in flight, so the shell can
/// ask on every pointer move and get the answer and the "no readout" case from one call.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_dragged_guide(
    engine: *mut CalmEngine,
    out_axis: *mut u8,
    out_position: *mut f32,
    out_screen: *mut f32,
) -> c_int {
    if out_axis.is_null() || out_position.is_null() || out_screen.is_null() {
        return 0;
    }
    read_doc(engine, 0, |doc| match doc.dragged_guide_readout() {
        Some((axis, position, screen)) => {
            unsafe {
                *out_axis = u8::from(axis);
                *out_position = position;
                *out_screen = screen;
            }
            1
        }
        None => 0,
    })
}

/// One guide as the guides card lists it. Position is document pixels along the axis the rule
/// crosses — `y` for a horizontal one, `x` for a vertical one. `color` is packed `0xRRGGBB`,
/// the same shape `calm_palette_color` hands back, so the card can compare a guide against a
/// palette swatch without unpacking either.
#[repr(C)]
pub struct CalmGuide {
    pub axis: u8,
    pub position: f32,
    pub color: u32,
}

/// Reads the whole list into `out`, returning how many were written. The card is the only
/// caller and redraws from scratch, so there is no cursor and no partial read to reconcile.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_guide_list(
    engine: *mut CalmEngine,
    out: *mut CalmGuide,
    cap: usize,
) -> usize {
    if out.is_null() || cap == 0 {
        return 0;
    }
    read_doc(engine, 0, |doc| {
        let guides = doc.guides();
        let n = guides.len().min(cap);
        for (i, guide) in guides.iter().take(n).enumerate() {
            unsafe {
                *out.add(i) = CalmGuide {
                    axis: u8::from(guide.axis),
                    position: guide.position,
                    color: pack_rgb(guide.color),
                };
            }
        }
        n
    })
}

/// Drops a guide at an exact document position — the card's Add, as opposed to a drag.
#[no_mangle]
pub extern "C" fn calm_engine_add_guide(
    engine: *mut CalmEngine,
    axis: u8,
    position: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let axis = GuideAxis::from_u8(axis).context("unknown guide axis")?;
        let doc = inner.doc.as_mut().context("no project is open")?;
        if doc.add_guide(axis, position).is_some() {
            inner.edited();
        }
        Ok(())
    })
}

/// Moves one guide to an exact position, clamped onto the paper rather than discarded — see
/// `Document::set_guide_position` for why typing differs from dragging here.
#[no_mangle]
pub extern "C" fn calm_engine_set_guide_position(
    engine: *mut CalmEngine,
    index: usize,
    position: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            if doc.set_guide_position(index, position) {
                inner.edited();
            }
        }
        Ok(())
    })
}

/// Moves a guide to the other edge, keeping its number — the card's Top/Left toggle on a row
/// that already exists.
#[no_mangle]
pub extern "C" fn calm_engine_set_guide_axis(
    engine: *mut CalmEngine,
    index: usize,
    axis: u8,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let axis = GuideAxis::from_u8(axis).context("unknown guide axis")?;
        let doc = inner.doc.as_mut().context("no project is open")?;
        if doc.set_guide_axis(index, axis) {
            inner.edited();
        }
        Ok(())
    })
}

/// Recolors one guide. `rgb` is packed `0xRRGGBB` — a guide has no alpha to set, because how
/// solid a rule is drawn is what says whether it is the one being dragged
/// (`compose::GUIDE_ALPHA`), not a choice.
#[no_mangle]
pub extern "C" fn calm_engine_set_guide_color(
    engine: *mut CalmEngine,
    index: usize,
    rgb: u32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            if doc.set_guide_color(index, unpack_rgb(rgb)) {
                inner.edited();
            }
        }
        Ok(())
    })
}

/// The color a guide is created with, so the card can offer it as the first swatch — asked
/// rather than written down in Swift, like every other product constant here.
#[no_mangle]
pub extern "C" fn calm_default_guide_color() -> u32 {
    pack_rgb(calumma_core::default_guide_color())
}

#[no_mangle]
pub extern "C" fn calm_engine_remove_guide(engine: *mut CalmEngine, index: usize) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            if doc.remove_guide(index) {
                inner.edited();
            }
        }
        Ok(())
    })
}

/// How many guides one document may hold. The card greys its Add out on this rather than letting
/// the button quietly do nothing once the list is full.
#[no_mangle]
pub extern "C" fn calm_guides_limit() -> usize {
    calumma_core::limits::GUIDES_LIMIT
}
