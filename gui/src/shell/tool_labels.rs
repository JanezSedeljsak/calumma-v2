use calumma_core::Tool;

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
