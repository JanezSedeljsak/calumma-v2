use calumma_core::{Tool, ToolBlock};
use calumma_ffi::*;
use std::ffi::CString;
use std::ptr;

const SIDE: u32 = 256;

struct GateEngine {
    ptr: *mut CalmEngine,
    _dir: tempfile::TempDir,
}

impl GateEngine {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = CString::new(dir.path().join("g.sqlite").to_str().unwrap()).unwrap();
        let ptr = unsafe { calm_engine_new(path.as_ptr()) };
        assert!(!ptr.is_null());
        let name = CString::new("Gate").unwrap();
        let id = unsafe { calm_project_create(ptr, name.as_ptr(), SIDE, SIDE) };
        assert!(!id.is_null());
        unsafe { calm_string_free(id) };
        assert_eq!(
            unsafe { calm_engine_resize(ptr, SIDE, SIDE, 1.0) },
            CalmStatus::Ok
        );
        assert_eq!(unsafe { calm_engine_fit(ptr) }, CalmStatus::Ok);
        Self { ptr, _dir: dir }
    }

    fn tool(&self, tool: Tool) {
        assert_eq!(
            unsafe { calm_engine_set_tool(self.ptr, tool as u32) },
            CalmStatus::Ok
        );
    }

    fn drag(&self, from: (f32, f32), to: (f32, f32)) {
        unsafe {
            calm_engine_pointer_down(self.ptr, from.0, from.1);
            calm_engine_pointer_move(self.ptr, to.0, to.1);
            calm_engine_pointer_up(self.ptr, to.0, to.1);
        }
    }

    fn block(&self, tool: Tool) -> u32 {
        calm_engine_tool_block(self.ptr, tool as u32)
    }

    fn notice(&self) -> u32 {
        let mut out = u32::MAX;
        assert_eq!(
            unsafe { calm_engine_take_tool_block_notice(self.ptr, &mut out) },
            CalmStatus::Ok
        );
        out
    }

    fn vector_layer(&self) -> u32 {
        (0..16)
            .find(|i| calm_engine_layer_is_vector(self.ptr, *i) == 1)
            .expect("a vector layer should exist")
    }
}

impl Drop for GateEngine {
    fn drop(&mut self) {
        unsafe { calm_engine_free(self.ptr) };
    }
}

/// The shell reads the whole table in one call and indexes it by tool discriminant, so the
/// slot a tool lands in is part of the contract.
#[test]
fn the_table_comes_back_indexed_by_tool() {
    let e = GateEngine::new();
    e.tool(Tool::Rect);
    assert_eq!(calm_engine_set_vector_mode(e.ptr, 1), CalmStatus::Ok);
    e.drag((60.0, 60.0), (160.0, 160.0));
    let layer = e.vector_layer();
    assert_eq!(
        unsafe { calm_engine_set_active_layer(e.ptr, layer) },
        CalmStatus::Ok
    );
    assert_eq!(calm_engine_set_vector_mode(e.ptr, 0), CalmStatus::Ok);

    let mut blocks = [u32::MAX; 24];
    let written =
        unsafe { calm_engine_tool_blocks(e.ptr, blocks.as_mut_ptr(), blocks.len() as u32) };
    assert_eq!(written, blocks.len() as u32);

    assert_eq!(blocks[Tool::Eraser as usize], ToolBlock::VectorLayer as u32);
    assert_eq!(blocks[Tool::Pen as usize], ToolBlock::None as u32);
    assert_eq!(blocks[Tool::Move as usize], ToolBlock::None as u32);
    assert_eq!(
        blocks[Tool::SelectRect as usize],
        ToolBlock::VectorLayer as u32
    );
    assert_eq!(
        blocks[Tool::Eraser as usize],
        e.block(Tool::Eraser),
        "the batch call and the single call are the same rule"
    );
    assert_eq!(
        calm_engine_vector_mode_locked(e.ptr),
        1,
        "a vector layer pins the knob on"
    );
    assert_eq!(calm_engine_layer_is_rasterizable(e.ptr, layer), 1);
}

#[test]
fn a_refused_press_leaves_a_reason_to_read_once() {
    let e = GateEngine::new();
    e.tool(Tool::Rect);
    assert_eq!(calm_engine_set_vector_mode(e.ptr, 1), CalmStatus::Ok);
    e.drag((60.0, 60.0), (160.0, 160.0));
    let layer = e.vector_layer();
    assert_eq!(
        unsafe { calm_engine_set_active_layer(e.ptr, layer) },
        CalmStatus::Ok
    );
    assert_eq!(calm_engine_set_vector_mode(e.ptr, 0), CalmStatus::Ok);
    assert_eq!(e.notice(), ToolBlock::None as u32, "nothing said yet");

    e.tool(Tool::Eraser);
    e.drag((70.0, 70.0), (90.0, 90.0));
    assert_eq!(e.notice(), ToolBlock::VectorLayer as u32);
    assert_eq!(e.notice(), ToolBlock::None as u32, "taking it clears it");
}

#[test]
fn rasterizing_a_vector_layer_hands_the_paint_tools_back() {
    let e = GateEngine::new();
    e.tool(Tool::Rect);
    assert_eq!(calm_engine_set_vector_mode(e.ptr, 1), CalmStatus::Ok);
    e.drag((60.0, 60.0), (160.0, 160.0));
    let layer = e.vector_layer();
    assert_eq!(
        unsafe { calm_engine_set_active_layer(e.ptr, layer) },
        CalmStatus::Ok
    );

    assert_eq!(calm_engine_rasterize_layer(e.ptr, layer), CalmStatus::Ok);

    assert_eq!(calm_engine_layer_is_vector(e.ptr, layer), 0);
    assert_eq!(calm_engine_layer_is_rasterizable(e.ptr, layer), 0);
    assert_eq!(calm_engine_vector_mode_locked(e.ptr), 0);
    assert_eq!(e.block(Tool::Eraser), ToolBlock::None as u32);
    assert_eq!(
        calm_engine_rasterize_layer(e.ptr, layer),
        CalmStatus::Error,
        "there is nothing left to flatten"
    );
}

#[test]
fn the_gate_calls_are_null_safe() {
    let mut out = 7u32;
    assert_eq!(calm_engine_tool_block(ptr::null_mut(), 0), 0);
    assert_eq!(
        unsafe { calm_engine_tool_blocks(ptr::null_mut(), &mut out, 1) },
        0
    );
    assert_eq!(
        unsafe { calm_engine_tool_blocks(ptr::null_mut(), ptr::null_mut(), 1) },
        0
    );
    assert_eq!(calm_engine_vector_mode_locked(ptr::null_mut()), 0);
    assert_eq!(calm_engine_layer_is_rasterizable(ptr::null_mut(), 0), 0);
    assert_eq!(
        calm_engine_rasterize_layer(ptr::null_mut(), 0),
        CalmStatus::Null
    );
    assert_eq!(
        unsafe { calm_engine_take_tool_block_notice(ptr::null_mut(), &mut out) },
        CalmStatus::Null
    );
    let engine = GateEngine::new();
    assert_eq!(
        unsafe { calm_engine_take_tool_block_notice(engine.ptr, ptr::null_mut()) },
        CalmStatus::Error
    );
}

/// The shell hides its own cursor while the ring is up, so "is there a ring" has to be exactly
/// `Document::brush_ring` and not the shell's guess at it — a ring withheld over a locked layer
/// has to bring the pointer back.
#[test]
fn brush_ring_visibility_follows_the_tool_the_layer_and_the_pointer() {
    let e = GateEngine::new();
    unsafe {
        assert_eq!(
            calm_engine_brush_ring_visible(e.ptr),
            0,
            "no pointer over the board yet"
        );
        assert_eq!(
            calm_engine_set_tool(e.ptr, Tool::Pen as u32),
            CalmStatus::Ok
        );
        assert_eq!(
            calm_engine_set_pointer_hover(e.ptr, 20.0, 20.0),
            CalmStatus::Ok
        );
        assert_eq!(calm_engine_brush_ring_visible(e.ptr), 1);

        // A tool that lays no stamp has no ring to show.
        assert_eq!(
            calm_engine_set_tool(e.ptr, Tool::Fill as u32),
            CalmStatus::Ok
        );
        assert_eq!(calm_engine_brush_ring_visible(e.ptr), 0);

        assert_eq!(
            calm_engine_set_tool(e.ptr, Tool::Pen as u32),
            CalmStatus::Ok
        );
        assert_eq!(calm_engine_brush_ring_visible(e.ptr), 1);
        assert_eq!(calm_engine_clear_pointer_hover(e.ptr), CalmStatus::Ok);
        assert_eq!(
            calm_engine_brush_ring_visible(e.ptr),
            0,
            "pointer off the board takes the ring with it"
        );
    }
}
