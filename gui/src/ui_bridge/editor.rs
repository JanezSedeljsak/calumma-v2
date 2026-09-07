use super::landing::accent_color;
use super::{brush, AppWindow, LayerRow, ToolEntry};
use crate::shell::{
    format_bytes, grid_slot_selected, grid_slot_tip_key, hue_color, slint_color, tool_family_key,
    AppController, TOOL_GRID,
};
use calumma_app::shortcuts::key_for_tool;
use calumma_app::ToolBlock;
use calumma_core::Tool;
use slint::{ModelRc, SharedString, VecModel};

pub fn set_editor_open(ui: &AppWindow, controller: &mut AppController, open: bool) {
    controller.editor_open = open;
    ui.set_editor_open(open);
    if open {
        sync_editor(ui, controller);
    }
}

fn block_reason_key(block: ToolBlock) -> Option<&'static str> {
    match block {
        ToolBlock::None => None,
        ToolBlock::LayerLocked => Some("toolBlockedLocked"),
        ToolBlock::TextLayer => Some("toolBlockedText"),
        ToolBlock::VectorLayer => Some("toolBlockedVector"),
        ToolBlock::NoContent => Some("toolBlockedEmpty"),
    }
}

fn tool_entries(controller: &AppController, active: Tool) -> Vec<ToolEntry> {
    let engine = controller.engine.borrow();
    TOOL_GRID
        .iter()
        .map(|slot| {
            let block = engine.tool_block(*slot);
            let blocked = block_reason_key(block);
            let tip = match blocked {
                Some(key) => controller.l10n.get(key),
                None => controller.l10n.get(grid_slot_tip_key(*slot)),
            };
            let shortcut = match (blocked, key_for_tool(*slot)) {
                (None, Some(key)) => key.to_uppercase().to_string(),
                _ => String::new(),
            };
            ToolEntry {
                id: *slot as i32,
                tip: SharedString::from(tip),
                shortcut: SharedString::from(shortcut),
                enabled: blocked.is_none(),
                selected: grid_slot_selected(*slot, active),
            }
        })
        .collect()
}

pub fn sync_layer_settings(ui: &AppWindow, controller: &AppController) {
    if let Some(layer) = controller.layer_settings_summary() {
        ui.set_layer_settings_name(SharedString::from(layer.name.as_str()));
        ui.set_layer_settings_visible(layer.visible);
        ui.set_layer_settings_can_delete(!layer.is_paper);
    }
    ui.set_layer_visibility_label(SharedString::from(controller.l10n.get("layerVisibility")));
    if let Some(index) = controller.layer_hover_index {
        let engine = controller.engine.borrow();
        let name = engine
            .list_layers()
            .into_iter()
            .find(|layer| layer.index == index)
            .map(|layer| layer.name)
            .unwrap_or_default();
        ui.set_hover_preview_visible(true);
        ui.set_hover_preview_name(SharedString::from(name.as_str()));
        ui.set_hover_preview_image(controller.thumb_cache.preview_image(index));
    } else {
        ui.set_hover_preview_visible(false);
    }
}

pub fn sync_color_picker(ui: &AppWindow, controller: &AppController) {
    let colors = &controller.quick_colors;
    ui.set_color_swatch0(brush(slint_color(colors.slots[0])));
    ui.set_color_swatch1(brush(slint_color(colors.slots[1])));
    ui.set_color_swatch2(brush(slint_color(colors.slots[2])));
    ui.set_active_color_swatch(colors.active as i32);
    ui.set_color_hue_brush(hue_color(colors.hsb.hue));
    ui.set_color_sb_x(colors.hsb.saturation);
    ui.set_color_sb_y(1.0 - colors.hsb.brightness);
    ui.set_color_hue_x(colors.hsb.hue);
    ui.set_color_hex_text(SharedString::from(colors.hex_text().as_str()));
}

pub fn sync_layers(ui: &AppWindow, controller: &mut AppController) {
    controller.thumb_cache.sync(&controller.engine.borrow());
    let engine = controller.engine.borrow();
    let rows: Vec<LayerRow> = engine
        .list_layers()
        .iter()
        .map(|layer| LayerRow {
            index: layer.index as i32,
            name: SharedString::from(layer.name.as_str()),
            visible: layer.visible,
            locked: layer.locked,
            active: layer.active,
            thumb: controller.thumb_cache.row_image(layer.index),
        })
        .collect();
    drop(engine);
    ui.set_layers(ModelRc::new(VecModel::from(rows)));
    sync_layer_settings(ui, controller);
}

fn sync_active_project(ui: &AppWindow, controller: &AppController) {
    let name = controller
        .engine
        .borrow()
        .project_name()
        .unwrap_or_else(|| controller.l10n.get("untitled"));
    ui.set_active_project_name(SharedString::from(name.as_str()));

    let id = controller.prefs.last_active_project_id.clone();
    let accent = id
        .and_then(|id| {
            controller
                .refresh_recents()
                .into_iter()
                .find(|item| item.id == id)
        })
        .map(|item| accent_color(&item))
        .unwrap_or(controller.theme.accent_teal);
    ui.set_active_project_accent(accent);
}

pub fn sync_editor(ui: &AppWindow, controller: &mut AppController) {
    let (tool, brush_size_unit, brush_size, ink_opacity, zoom_unit, zoom, is_fit, memory) = {
        let engine = controller.engine.borrow();
        let tool = engine.active_tool().unwrap_or(Tool::Pen);
        (
            tool,
            engine.brush_size_unit(),
            engine.brush_size(),
            engine.ink_opacity(),
            engine.zoom_unit(),
            engine.zoom_factor(),
            engine.is_fit(),
            format_bytes(engine.resident_memory_bytes()),
        )
    };
    ui.set_active_tool(tool as i32);
    ui.set_tools(ModelRc::new(VecModel::from(tool_entries(controller, tool))));
    let label = controller.l10n.get(tool_family_key(tool));
    ui.set_active_tool_label(SharedString::from(label.as_str()));
    ui.set_show_brush_size(tool.takes_brush());
    ui.set_show_ink_opacity(tool.takes_ink_opacity());
    ui.set_brush_size_unit(brush_size_unit);
    ui.set_brush_size_text(SharedString::from(format!("{}", brush_size.round() as i32)));
    ui.set_ink_opacity(ink_opacity);
    ui.set_ink_opacity_text(SharedString::from(format!(
        "{}",
        (ink_opacity * 100.0).round() as i32
    )));
    ui.set_zoom_unit(zoom_unit);
    ui.set_zoom_text(SharedString::from(format!(
        "{}%",
        (zoom * 100.0).round() as i32
    )));
    ui.set_is_fit(is_fit);
    ui.set_memory_value(SharedString::from(memory));
    if let Some((width, height)) = controller.engine.borrow().document_size() {
        ui.set_doc_width_text(SharedString::from(format!("{width}")));
        ui.set_doc_height_text(SharedString::from(format!("{height}")));
    }
    sync_active_project(ui, controller);
    ui.set_tools_menu_open(controller.tools_menu_open);
    ui.set_tools_busy(controller.tools_busy);
    ui.set_smart_matte_label(SharedString::from(controller.smart_matte_label().as_str()));
    let can_tools = controller.can_run_smart_tools();
    ui.set_can_upscale(can_tools);
    ui.set_can_smart_matte(can_tools);
    ui.set_can_seam_carve(can_tools);
    sync_color_picker(ui, controller);
    sync_layers(ui, controller);
}
