use super::Engine;
use calumma_core::{Tool, ToolBlock};

impl Engine {
    pub fn set_tool(&mut self, tool: Tool) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_tool(tool);
            let last_shape = doc.last_shape_tool;
            let last_select = doc.last_select_tool;
            inner.last_shape_tool = last_shape;
            inner.last_select_tool = last_select;
        } else if tool.is_shape() {
            inner.last_shape_tool = tool;
        } else if tool.is_selection() {
            inner.last_select_tool = tool;
        }
        inner.invalidate_renderer();
    }

    pub fn active_tool(&self) -> Option<Tool> {
        self.inner.lock().doc.as_ref().map(|doc| doc.tool)
    }

    pub fn tool_block(&self, tool: Tool) -> ToolBlock {
        self.inner
            .lock()
            .doc
            .as_ref()
            .map(|doc| doc.tool_block(tool))
            .unwrap_or(ToolBlock::None)
    }

    pub fn take_tool_block_notice(&mut self) -> Option<ToolBlock> {
        self.inner
            .lock()
            .doc
            .as_mut()
            .and_then(|doc| doc.take_tool_block_notice())
    }

    pub fn last_select_tool(&self) -> Tool {
        let inner = self.inner.lock();
        inner
            .doc
            .as_ref()
            .map(|doc| doc.last_select_tool)
            .unwrap_or(inner.last_select_tool)
    }

    pub fn last_shape_tool(&self) -> Tool {
        let inner = self.inner.lock();
        inner
            .doc
            .as_ref()
            .map(|doc| doc.last_shape_tool)
            .unwrap_or(inner.last_shape_tool)
    }
}
