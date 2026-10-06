use super::{put, put_rows};
use crate::shell::{
    brush_icon_index, brush_label_key, grid_slot_selected, grid_slot_tip_key, grid_slot_tool,
    tool_icon_index, tool_label_key, AppController, BRUSHES, SELECT_TOOLS, SHAPE_TOOLS, TOOL_GRID,
};
use crate::ui_bridge::{AppWindow, BrushEntry, ToolChrome, ToolEntry};
use calumma_app::shortcuts::key_for_tool;
use calumma_app::ToolBlock;
use calumma_core::Tool;
use slint::{ComponentHandle, SharedString};

fn block_reason_key(block: ToolBlock) -> Option<&'static str> {
    match block {
        ToolBlock::None => None,
        ToolBlock::LayerLocked => Some("toolBlockedLocked"),
        ToolBlock::TextLayer => Some("toolBlockedText"),
        ToolBlock::VectorLayer => Some("toolBlockedVector"),
        ToolBlock::NoContent => Some("toolBlockedEmpty"),
    }
}

fn entry_for(controller: &AppController, tool: Tool, active: Tool) -> ToolEntry {
    let engine = controller.engine.borrow();
    let block = engine.tool_block(tool);
    let blocked = block_reason_key(block);
    let tip = match blocked {
        Some(key) => controller.l10n.get(key),
        None => controller.l10n.get(tool_label_key(tool)),
    };
    let shortcut = match (blocked, key_for_tool(tool)) {
        (None, Some(key)) => key.to_uppercase().to_string(),
        _ => String::new(),
    };
    let icon = if tool == Tool::Move && engine.transform_active() {
        tool_icon_index(Tool::Transform)
    } else {
        tool_icon_index(tool)
    };
    ToolEntry {
        id: tool as i32,
        icon,
        tip: SharedString::from(tip),
        shortcut: SharedString::from(shortcut),
        enabled: blocked.is_none(),
        selected: tool == active,
    }
}

fn tool_entries(controller: &AppController, active: Tool) -> Vec<ToolEntry> {
    let engine = controller.engine.borrow();
    let last_shape = engine.last_shape_tool();
    let last_select = engine.last_select_tool();
    drop(engine);
    TOOL_GRID
        .iter()
        .map(|slot| {
            let resolved = grid_slot_tool(*slot, last_shape, last_select);
            let family = *slot == Tool::Rect || *slot == Tool::SelectRect;
            let mut entry = entry_for(controller, resolved, active);
            entry.selected = grid_slot_selected(*slot, active);
            if family && entry.enabled {
                entry.tip = SharedString::from(controller.l10n.get(grid_slot_tip_key(*slot)));
            }
            entry
        })
        .collect()
}

fn shape_entries(controller: &AppController, active: Tool) -> Vec<ToolEntry> {
    SHAPE_TOOLS
        .iter()
        .map(|tool| entry_for(controller, *tool, active))
        .collect()
}

fn select_entries(controller: &AppController, active: Tool) -> Vec<ToolEntry> {
    SELECT_TOOLS
        .iter()
        .map(|tool| entry_for(controller, *tool, active))
        .collect()
}

fn brush_entries(controller: &AppController) -> Vec<BrushEntry> {
    let current = controller.engine.borrow().brush();
    BRUSHES
        .iter()
        .map(|brush| BrushEntry {
            id: *brush as i32,
            icon: brush_icon_index(*brush),
            tip: SharedString::from(controller.l10n.get(brush_label_key(*brush))),
            selected: *brush == current,
        })
        .collect()
}

pub fn sync_tool_gate(ui: &AppWindow, controller: &AppController) {
    let tool = controller
        .engine
        .borrow()
        .active_tool()
        .unwrap_or(Tool::Pen);
    let chrome = ui.global::<ToolChrome>();
    chrome.set_tools(put_rows(chrome.get_tools(), tool_entries(controller, tool)));
    sync_tool_chrome(ui, controller, tool);
    sync_smart_tools(ui, controller);
}

pub fn sync_smart_tools(ui: &AppWindow, controller: &AppController) {
    let (available, running, enabled) = {
        let engine = controller.engine.borrow();
        let enabled = engine
            .active_layer_index()
            .is_some_and(|index| engine.can_remove_background(index));
        (
            engine.background_removal_available(),
            engine.background_removal_running(),
            enabled,
        )
    };
    let chrome = ui.global::<ToolChrome>();
    chrome.set_show_smart_tools(available);
    chrome.set_remove_background_enabled(enabled);
    let key = if running {
        "removingBackground"
    } else {
        "removeBackground"
    };
    chrome.set_remove_background_label(put(controller.l10n.get(key)));
}

fn sync_tool_chrome(ui: &AppWindow, controller: &AppController, tool: Tool) {
    let (
        vector_locked,
        vector_on,
        show_brushes,
        brush_size_unit,
        brush_size,
        ink_opacity,
        blur,
        hardness,
        tolerance,
        radius,
        fill_on,
        stroke_on,
        aligned,
        transform_on,
    ) = {
        let engine = controller.engine.borrow();
        (
            engine.vector_mode_locked(),
            engine.vector_mode(),
            tool.takes_brush() && !engine.vector_mode(),
            engine.brush_size_unit(),
            engine.brush_size(),
            engine.ink_opacity(),
            engine.blur_strength(),
            engine.eraser_hardness(),
            engine.tolerance(),
            engine.eyedropper_radius(),
            engine.shape_fill(),
            engine.shape_stroke(),
            engine.clone_aligned(),
            engine.transform_active(),
        )
    };
    let chrome = ui.global::<ToolChrome>();
    chrome.set_show_shapes(tool.is_shape());
    chrome.set_show_selection(tool.is_selection());
    chrome.set_show_brushes(show_brushes);
    chrome.set_show_brush_size(tool.takes_brush_size());
    chrome.set_show_ink_opacity(tool.takes_ink_opacity());
    chrome.set_show_blur(tool.takes_blur_strength());
    chrome.set_show_hardness(tool.takes_eraser_hardness());
    chrome.set_show_tolerance(tool.takes_tolerance());
    chrome.set_show_eyedropper(tool.takes_eyedropper_radius());
    chrome.set_show_fill(tool.takes_fill());
    chrome.set_show_vector(tool.shows_vector_mode());
    chrome.set_show_aligned(tool.takes_clone_aligned());
    chrome.set_show_transform(tool == Tool::Move);
    chrome.set_show_crop(tool == Tool::Crop);
    chrome.set_show_text(tool == Tool::Text);
    chrome.set_shape_tools(put_rows(
        chrome.get_shape_tools(),
        shape_entries(controller, tool),
    ));
    chrome.set_select_tools(put_rows(
        chrome.get_select_tools(),
        select_entries(controller, tool),
    ));
    chrome.set_brush_tools(put_rows(
        chrome.get_brush_tools(),
        brush_entries(controller),
    ));
    chrome.set_brush_size_unit(brush_size_unit);
    chrome.set_brush_size_text(put(format!("{}", brush_size.round() as i32)));
    chrome.set_ink_opacity(ink_opacity);
    chrome.set_ink_opacity_text(put(format!("{}", (ink_opacity * 100.0).round() as i32)));
    chrome.set_blur_strength(blur);
    chrome.set_blur_text(put(format!("{}", (blur * 100.0).round() as i32)));
    chrome.set_eraser_hardness(hardness);
    chrome.set_hardness_text(put(format!("{}", (hardness * 100.0).round() as i32)));
    chrome.set_tolerance_unit(f32::from(tolerance));
    chrome.set_tolerance_text(put(format!("{tolerance}")));
    chrome.set_eyedropper_radius(radius as f32);
    let side = radius * 2 + 1;
    chrome.set_eyedropper_text(put(format!("{side}×{side}")));
    chrome.set_fill_on(fill_on);
    chrome.set_stroke_on(stroke_on);
    chrome.set_vector_on(vector_on);
    chrome.set_vector_locked(vector_locked);
    chrome.set_clone_aligned(aligned);
    chrome.set_transform_on(transform_on);
    if tool == Tool::Crop {
        let engine = controller.engine.borrow();
        chrome.set_crop_overlay(engine.crop_overlay_style() as i32);
        chrome.set_crop_aspect(crop_aspect_index(engine.crop_aspect_lock()));
    }
    super::text::sync_text_chrome(chrome, controller, tool);
    super::color::sync_color_tips(ui, controller, tool);
}

fn crop_aspect_index(ratio: Option<f32>) -> i32 {
    match ratio {
        Some(value) if (value - 1.0).abs() < 0.02 => 1,
        Some(value) if (value - 4.0 / 3.0).abs() < 0.02 => 2,
        Some(value) if (value - 3.0 / 2.0).abs() < 0.02 => 3,
        Some(value) if (value - 16.0 / 9.0).abs() < 0.02 => 4,
        Some(value) if (value - 5.0 / 4.0).abs() < 0.02 => 5,
        _ => 0,
    }
}
