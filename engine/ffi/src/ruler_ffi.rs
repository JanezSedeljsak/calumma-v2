use crate::engine::{read_doc, CalmEngine, CalmRulerTick};
use calumma_core::{ruler_ticks, RulerTick};

fn write_ticks(out: *mut CalmRulerTick, items: &[RulerTick], cap: usize) -> usize {
    let n = items.len().min(cap);
    for (i, tick) in items.iter().take(n).enumerate() {
        unsafe {
            *out.add(i) = CalmRulerTick {
                doc: tick.doc,
                major: u8::from(tick.major),
            };
        }
    }
    n
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_ruler_ticks_x(
    engine: *mut CalmEngine,
    out: *mut CalmRulerTick,
    cap: usize,
) -> usize {
    if out.is_null() || cap == 0 {
        return 0;
    }
    read_doc(engine, 0, |doc| {
        write_ticks(out, &doc.camera.ruler_ticks_x(), cap)
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_ruler_ticks_x(
    zoom: f32,
    pan: f32,
    viewport_extent: f32,
    out: *mut CalmRulerTick,
    cap: usize,
) -> usize {
    if out.is_null() || cap == 0 {
        return 0;
    }
    write_ticks(out, &ruler_ticks(zoom, pan, viewport_extent), cap)
}

#[no_mangle]
pub unsafe extern "C" fn calm_ruler_ticks_y(
    zoom: f32,
    pan: f32,
    viewport_extent: f32,
    out: *mut CalmRulerTick,
    cap: usize,
) -> usize {
    if out.is_null() || cap == 0 {
        return 0;
    }
    write_ticks(out, &ruler_ticks(zoom, pan, viewport_extent), cap)
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_ruler_ticks_y(
    engine: *mut CalmEngine,
    out: *mut CalmRulerTick,
    cap: usize,
) -> usize {
    if out.is_null() || cap == 0 {
        return 0;
    }
    read_doc(engine, 0, |doc| {
        write_ticks(out, &doc.camera.ruler_ticks_y(), cap)
    })
}
