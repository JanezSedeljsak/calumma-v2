use calumma_core::Tool;
use calumma_ffi::Engine;

use crate::shortcuts::{is_marquee_family, tool_for_key};

pub fn pick_tool(engine: &mut Engine, tool: Tool) {
    let resolved = if is_marquee_family(tool) {
        engine.last_select_tool()
    } else {
        tool
    };
    engine.set_tool(resolved);
}

pub fn pick_tool_key(engine: &mut Engine, key: char) -> bool {
    let Some(tool) = tool_for_key(key) else {
        return false;
    };
    pick_tool(engine, tool);
    true
}
