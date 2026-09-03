use crate::engine::{read_doc, renderer_set_memory_pressure, CalmEngine, CalmStatus};
use calumma_core::memory::document_memory;
use calumma_core::MemoryPressureLevel;

/// What the engine is holding right now, in bytes. Exactly one project is resident at a time
/// — opening or closing one drops the previous document and the GPU textures cached for it —
/// so this is the whole picture, not a per-project slice of a larger pool.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CalmMemory {
    pub tile_bytes: u64,
    pub history_bytes: u64,
    pub mask_bytes: u64,
    pub vector_bytes: u64,
    pub text_bytes: u64,
    pub preview_bytes: u64,
    pub gpu_bytes: u64,
    pub tile_count: u32,
    /// Tiles whose pixels another tile already paid for — Paper's shared fill, and whatever
    /// history still shares with the live document.
    pub shared_tile_count: u32,
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_memory(
    engine: *mut CalmEngine,
    out: *mut CalmMemory,
) -> CalmStatus {
    if out.is_null() {
        return CalmStatus::Null;
    }
    let gpu_bytes = crate::engine::renderer_gpu_bytes(engine);
    let report = read_doc(engine, None, |doc| Some(document_memory(doc)));
    let Some(report) = report else {
        unsafe { *out = CalmMemory::default() };
        return if engine.is_null() {
            CalmStatus::Null
        } else {
            CalmStatus::Ok
        };
    };
    unsafe {
        *out = CalmMemory {
            tile_bytes: report.tile_bytes as u64,
            history_bytes: report.history_bytes as u64,
            mask_bytes: report.mask_bytes as u64,
            vector_bytes: report.vector_bytes as u64,
            text_bytes: report.text_bytes as u64,
            preview_bytes: report.preview_bytes as u64,
            gpu_bytes: gpu_bytes as u64,
            tile_count: report.tile_count as u32,
            shared_tile_count: report.shared_tile_count as u32,
        };
    }
    CalmStatus::Ok
}

/// The one inbound knob docs/plans/22-adaptive-gpu-memory-pressure.md adds: the shell forwards
/// whatever level the OS just reported — mirroring `DISPATCH_SOURCE_TYPE_MEMORYPRESSURE` on
/// macOS, `0` = normal, `1` = warn, `2` = critical, the same raw-`u32` convention
/// `calm_engine_set_tool` uses rather than a declared C enum type — and the engine decides what
/// that costs the tile atlas (`calumma_core::MemoryPressureLevel`,
/// `calumma_render::Renderer::set_memory_pressure`). `Normal` is also what the renderer starts
/// in, so a shell with no pressure API to wire up yet never needs to call this at all.
/// `calm_engine_memory`'s `gpu_bytes` is how a caller verifies the response actually happened.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_memory_pressure(
    engine: *mut CalmEngine,
    level: u32,
) -> CalmStatus {
    if engine.is_null() {
        return CalmStatus::Null;
    }
    let Some(level) = MemoryPressureLevel::from_u32(level) else {
        return CalmStatus::Error;
    };
    renderer_set_memory_pressure(engine, level);
    CalmStatus::Ok
}
