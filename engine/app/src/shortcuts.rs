use calumma_core::Tool;

pub struct ToolKey {
    pub key: char,
    pub tool: Tool,
}

pub const TOOL_KEYS: &[ToolKey] = &[
    ToolKey {
        key: 'p',
        tool: Tool::Pen,
    },
    ToolKey {
        key: 'l',
        tool: Tool::Line,
    },
    ToolKey {
        key: 'r',
        tool: Tool::Rect,
    },
    ToolKey {
        key: 'o',
        tool: Tool::Ellipse,
    },
    ToolKey {
        key: 'a',
        tool: Tool::Arrow,
    },
    ToolKey {
        key: '3',
        tool: Tool::Triangle,
    },
    ToolKey {
        key: '5',
        tool: Tool::Pentagon,
    },
    ToolKey {
        key: 't',
        tool: Tool::Text,
    },
    ToolKey {
        key: 'e',
        tool: Tool::Eraser,
    },
    ToolKey {
        key: 'u',
        tool: Tool::Blur,
    },
    ToolKey {
        key: 'c',
        tool: Tool::Clone,
    },
    ToolKey {
        key: 'h',
        tool: Tool::Heal,
    },
    ToolKey {
        key: 'g',
        tool: Tool::Fill,
    },
    ToolKey {
        key: 'i',
        tool: Tool::Eyedropper,
    },
    ToolKey {
        key: 'm',
        tool: Tool::SelectRect,
    },
    ToolKey {
        key: 'w',
        tool: Tool::MagicWand,
    },
    ToolKey {
        key: 'v',
        tool: Tool::Move,
    },
    ToolKey {
        key: 'k',
        tool: Tool::Crop,
    },
];

pub fn tool_for_key(key: char) -> Option<Tool> {
    TOOL_KEYS
        .iter()
        .find(|entry| entry.key == key)
        .map(|entry| entry.tool)
}

pub fn is_marquee_family(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::SelectRect | Tool::SelectEllipse | Tool::SelectLasso
    )
}

pub fn key_for_tool(tool: Tool) -> Option<char> {
    if matches!(tool, Tool::Transform | Tool::SelectColor) {
        return None;
    }
    let family = if is_marquee_family(tool) {
        Tool::SelectRect
    } else {
        tool
    };
    TOOL_KEYS
        .iter()
        .find(|entry| entry.tool == family)
        .map(|entry| entry.key)
}

pub fn is_toggle_layers_shortcut(text: &str, meta: bool, alt: bool) -> bool {
    meta && alt && text.eq_ignore_ascii_case("l")
}

pub fn is_transform_shortcut(text: &str, meta: bool, ctrl: bool) -> bool {
    (meta || ctrl) && text.eq_ignore_ascii_case("t")
}

pub fn is_undo_shortcut(text: &str, meta: bool, ctrl: bool, shift: bool) -> bool {
    (meta || ctrl) && !shift && text.eq_ignore_ascii_case("z")
}

pub fn is_redo_shortcut(text: &str, meta: bool, ctrl: bool, shift: bool) -> bool {
    (meta || ctrl) && shift && text.eq_ignore_ascii_case("z")
}
