use calumma_core::Tool;

pub const TOOL_GRID: [Tool; 12] = [
    Tool::Move,
    Tool::SelectRect,
    Tool::Pen,
    Tool::Eraser,
    Tool::Blur,
    Tool::Clone,
    Tool::Heal,
    Tool::Rect,
    Tool::Fill,
    Tool::Eyedropper,
    Tool::Text,
    Tool::Crop,
];

pub fn tool_label_key(tool: Tool) -> &'static str {
    match tool {
        Tool::Pen => "toolPen",
        Tool::Line => "toolLine",
        Tool::Rect => "toolRect",
        Tool::Ellipse => "toolEllipse",
        Tool::Arrow => "toolArrow",
        Tool::Triangle => "toolTriangle",
        Tool::Pentagon => "toolPentagon",
        Tool::SelectRect => "toolSelectRect",
        Tool::SelectEllipse => "toolSelectEllipse",
        Tool::SelectLasso => "toolSelectLasso",
        Tool::Fill => "toolBucket",
        Tool::Eyedropper => "toolEyedropper",
        Tool::Text => "toolText",
        Tool::Eraser => "toolEraser",
        Tool::Blur => "toolBlur",
        Tool::Clone => "toolClone",
        Tool::Heal => "toolHeal",
        Tool::MagicWand => "toolMagicWand",
        Tool::Move => "toolMove",
        Tool::Crop => "toolCrop",
        Tool::Transform => "toolTransform",
        Tool::SelectColor => "toolSelectColor",
    }
}

pub fn tool_family_key(tool: Tool) -> &'static str {
    if tool.is_shape() {
        return "shapes";
    }
    if tool.is_selection() {
        return "selectionTools";
    }
    tool_label_key(tool)
}

pub fn grid_slot_selected(slot: Tool, active: Tool) -> bool {
    if slot == Tool::Rect {
        return active.is_shape();
    }
    if slot == Tool::SelectRect {
        return active.is_selection();
    }
    slot == active
}

pub fn grid_slot_tip_key(slot: Tool) -> &'static str {
    if slot == Tool::Rect {
        return "shapes";
    }
    if slot == Tool::SelectRect {
        return "selectionTools";
    }
    tool_label_key(slot)
}
