use calumma_core::{BlendMode, Brush, Tool};

pub fn blend_label_key(mode: BlendMode) -> &'static str {
    match mode {
        BlendMode::Normal => "blendNormal",
        BlendMode::Multiply => "blendMultiply",
        BlendMode::Screen => "blendScreen",
        BlendMode::Darken => "blendDarken",
        BlendMode::ColorBurn => "blendColorBurn",
        BlendMode::LinearBurn => "blendLinearBurn",
        BlendMode::DarkerColor => "blendDarkerColor",
        BlendMode::Lighten => "blendLighten",
        BlendMode::ColorDodge => "blendColorDodge",
        BlendMode::LinearDodge => "blendLinearDodge",
        BlendMode::LighterColor => "blendLighterColor",
        BlendMode::Overlay => "blendOverlay",
        BlendMode::SoftLight => "blendSoftLight",
        BlendMode::HardLight => "blendHardLight",
        BlendMode::VividLight => "blendVividLight",
        BlendMode::LinearLight => "blendLinearLight",
        BlendMode::PinLight => "blendPinLight",
        BlendMode::HardMix => "blendHardMix",
        BlendMode::Difference => "blendDifference",
        BlendMode::Exclusion => "blendExclusion",
        BlendMode::Subtract => "blendSubtract",
        BlendMode::Divide => "blendDivide",
        BlendMode::Hue => "blendHue",
        BlendMode::Saturation => "blendSaturation",
        BlendMode::Color => "blendColor",
        BlendMode::Luminosity => "blendLuminosity",
    }
}

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

pub const SHAPE_TOOLS: [Tool; 6] = [
    Tool::Line,
    Tool::Rect,
    Tool::Ellipse,
    Tool::Arrow,
    Tool::Triangle,
    Tool::Pentagon,
];

pub const SELECT_TOOLS: [Tool; 5] = [
    Tool::SelectRect,
    Tool::SelectEllipse,
    Tool::SelectLasso,
    Tool::MagicWand,
    Tool::SelectColor,
];

pub const BRUSHES: [Brush; 4] = [Brush::Pen, Brush::Marker, Brush::Crayon, Brush::Airbrush];

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

pub fn brush_label_key(brush: Brush) -> &'static str {
    match brush {
        Brush::Pen => "brushPen",
        Brush::Marker => "brushMarker",
        Brush::Crayon => "brushCrayon",
        Brush::Airbrush => "brushAirbrush",
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

pub fn grid_slot_tool(slot: Tool, last_shape: Tool, last_select: Tool) -> Tool {
    if slot == Tool::Rect {
        return last_shape;
    }
    if slot == Tool::SelectRect {
        return last_select;
    }
    slot
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

pub fn tool_icon_index(tool: Tool) -> i32 {
    match tool {
        Tool::Move => 0,
        Tool::SelectRect => 1,
        Tool::Pen => 2,
        Tool::Eraser => 3,
        Tool::Blur => 4,
        Tool::Clone => 5,
        Tool::Heal => 6,
        Tool::Rect => 7,
        Tool::Fill => 8,
        Tool::Eyedropper => 9,
        Tool::Text => 10,
        Tool::Crop => 11,
        Tool::Line => 12,
        Tool::Ellipse => 13,
        Tool::Arrow => 14,
        Tool::Triangle => 15,
        Tool::Pentagon => 16,
        Tool::SelectEllipse => 17,
        Tool::SelectLasso => 18,
        Tool::MagicWand => 19,
        Tool::SelectColor => 20,
        Tool::Transform => 21,
    }
}

pub fn brush_icon_index(brush: Brush) -> i32 {
    match brush {
        Brush::Pen => 0,
        Brush::Marker => 1,
        Brush::Crayon => 2,
        Brush::Airbrush => 3,
    }
}
