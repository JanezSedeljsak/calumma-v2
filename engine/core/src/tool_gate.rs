//! One answer to "may this tool run on the active layer, and if not, why".
//!
//! The rules existed before this module did — scattered across seven early returns that said
//! nothing on the way out, plus `enter_transform` spelling a third variant of the same idea.
//! They live here now so the greyed-out button in the panel and the engine's refusal cannot
//! drift apart: both read `Document::tool_block`.

use crate::document::Document;
use crate::layer::Layer;
use crate::shape::Tool;

use num_enum::{IntoPrimitive, TryFromPrimitive};

/// Why a tool cannot run, or `None` when it can. Crosses the FFI as its discriminant, so the
/// shell shows the reason without knowing any of the rules behind it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u32)]
pub enum ToolBlock {
    #[default]
    None = 0,
    LayerLocked = 1,
    TextLayer = 2,
    VectorLayer = 3,
    NoContent = 4,
}

impl ToolBlock {
    pub fn from_u32(value: u32) -> Option<Self> {
        Self::try_from(value).ok()
    }
}

/// Whether pixels can be written into this layer at all. A text layer's tiles are a cache of
/// its run, so a stroke there disappears on the next keystroke; a vector layer has no tiles to
/// write into; a locked one refuses on purpose.
pub(crate) fn accepts_pixels(layer: &Layer) -> bool {
    layer.tiles().is_some() && !layer.is_text() && !layer.locked
}

impl Document {
    /// A vector layer pins vector mode on: its one item is the whole layer, so the only thing a
    /// pen or a shape can mean there is another vector — never pixels the layer cannot hold.
    pub fn vector_mode_locked(&self) -> bool {
        self.layers
            .get(self.active_layer)
            .is_some_and(|layer| layer.content.is_vector())
    }

    /// The vector mode that actually governs a commit: the shell's knob, or the active layer
    /// forcing it. Everything that used to read `vector_mode` directly reads this instead.
    pub fn effective_vector_mode(&self) -> bool {
        self.vector_mode || self.vector_mode_locked()
    }

    /// Whether this tool's drag lands a brand-new vector layer. When it does, the active
    /// layer is a bystander — nothing about it can block the drag.
    fn draws_new_vector(&self, tool: Tool) -> bool {
        (tool == Tool::Pen || tool.is_shape()) && self.effective_vector_mode()
    }

    /// The single source of truth for tool availability.
    ///
    /// Three tools are never blocked, each for its own reason. The eyedropper reads the
    /// composite and writes only the colour swatch. Move picks its own target out of the stack
    /// rather than acting on the active layer. A pen or shape in vector mode commits into a
    /// layer that does not exist yet.
    pub fn tool_block(&self, tool: Tool) -> ToolBlock {
        let Some(layer) = self.layers.get(self.active_layer) else {
            return ToolBlock::NoContent;
        };
        if matches!(tool, Tool::Eyedropper | Tool::Move) || self.draws_new_vector(tool) {
            return ToolBlock::None;
        }
        if layer.locked {
            return ToolBlock::LayerLocked;
        }
        if tool == Tool::Transform {
            return match layer.content_bounds() {
                Some(_) => ToolBlock::None,
                None => ToolBlock::NoContent,
            };
        }
        // Text starts its own layer when the click misses every run, so what the active layer
        // happens to be does not decide whether the tool can be used.
        if tool == Tool::Text {
            return ToolBlock::None;
        }
        if layer.is_text() {
            return ToolBlock::TextLayer;
        }
        if layer.content.is_vector() {
            return ToolBlock::VectorLayer;
        }
        ToolBlock::None
    }

    pub fn tool_blocked(&self, tool: Tool) -> bool {
        self.tool_block(tool) != ToolBlock::None
    }

    /// Whether a board press stops here, recording why it did so the shell can say it out
    /// loud. The reason is kept once per (layer, tool) pair: repeating it on every press of a
    /// tool the user has already been told about would be nagging, not feedback.
    pub(crate) fn press_blocked(&mut self, tool: Tool) -> bool {
        let block = self.tool_block(tool);
        if block == ToolBlock::None {
            self.blocked_notice_key = None;
            return false;
        }
        let key = (self.active_layer, self.tool);
        if self.blocked_notice_key != Some(key) {
            self.blocked_notice_key = Some(key);
            self.blocked_notice = Some(block);
        }
        true
    }

    pub fn take_tool_block_notice(&mut self) -> Option<ToolBlock> {
        self.blocked_notice.take()
    }
}
