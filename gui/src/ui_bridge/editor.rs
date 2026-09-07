use super::landing::accent_color;
use super::{brush, AppWindow, BrushEntry, LayerRow, ToolChrome, ToolEntry};
use crate::shell::{
    brush_icon_index, brush_label_key, format_bytes, grid_slot_selected, grid_slot_tip_key,
    grid_slot_tool, hue_color, slint_color, tool_family_key, tool_icon_index, tool_label_key,
    AppController, BRUSHES, SELECT_TOOLS, SHAPE_TOOLS, TOOL_GRID,
};
use calumma_app::shortcuts::key_for_tool;
use calumma_app::ToolBlock;
use calumma_core::Tool;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

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

fn put(value: String) -> SharedString {
    SharedString::from(value)
}

fn put_rows<T: Clone + 'static>(current: ModelRc<T>, rows: Vec<T>) -> ModelRc<T> {
    if let Some(model) = current.as_any().downcast_ref::<VecModel<T>>() {
        if model.row_count() == rows.len() {
            for (index, row) in rows.into_iter().enumerate() {
                model.set_row_data(index, row);
            }
            return current;
        }
        model.set_vec(rows);
        return current;
    }
    ModelRc::new(VecModel::from(rows))
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
}

pub fn sync_layer_settings(ui: &AppWindow, controller: &AppController) {
    if let Some(layer) = controller.layer_settings_summary() {
        let opacity = controller.engine.borrow().layer_opacity(layer.index);
        ui.set_layer_settings_name(SharedString::from(layer.name.as_str()));
        ui.set_layer_settings_visible(layer.visible);
        ui.set_layer_settings_locked(layer.locked);
        ui.set_layer_settings_can_delete(!layer.is_paper);
        ui.set_layer_settings_opacity(opacity);
        ui.set_layer_settings_opacity_text(put(format!("{}", (opacity * 100.0).round() as i32)));
        ui.set_layer_settings_preview(controller.thumb_cache.preview_image(layer.index));
        ui.set_layer_lock_label(put(controller.l10n.get(if layer.locked {
            "layerUnlock"
        } else {
            "layerLock"
        })));
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
        ui.set_hover_preview_y(controller.layer_hover_y);
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

fn sync_layer_bounds(ui: &AppWindow, controller: &AppController) {
    let engine = controller.engine.borrow();
    let index = engine.active_layer_index();
    let bounds = index.and_then(|index| engine.layer_bounds(index));
    if let Some((x, y, x1, y1)) = bounds {
        ui.set_layer_x_text(put(format!("{}", x.round() as i32)));
        ui.set_layer_y_text(put(format!("{}", y.round() as i32)));
        ui.set_layer_w_text(put(format!("{}", (x1 - x).round() as i32)));
        ui.set_layer_h_text(put(format!("{}", (y1 - y).round() as i32)));
    } else {
        ui.set_layer_x_text(put("0".into()));
        ui.set_layer_y_text(put("0".into()));
        ui.set_layer_w_text(put("0".into()));
        ui.set_layer_h_text(put("0".into()));
    }
}

pub fn sync_layer_rows(ui: &AppWindow, controller: &mut AppController) {
    let thumbs_changed = controller.thumb_cache.sync(&controller.engine.borrow());
    if thumbs_changed {
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
    }
    sync_layer_settings(ui, controller);
}

pub fn sync_layers(ui: &AppWindow, controller: &mut AppController) {
    sync_layer_rows(ui, controller);
    sync_layer_bounds(ui, controller);
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
    let (tool, zoom_unit, zoom, is_fit, memory) = {
        let engine = controller.engine.borrow();
        let tool = engine.active_tool().unwrap_or(Tool::Pen);
        (
            tool,
            engine.zoom_unit(),
            engine.zoom_factor(),
            engine.is_fit(),
            format_bytes(engine.resident_memory_bytes()),
        )
    };
    ui.set_active_tool(tool as i32);
    ui.set_tools(put_rows(ui.get_tools(), tool_entries(controller, tool)));
    let label = controller.l10n.get(tool_family_key(tool));
    ui.set_active_tool_label(SharedString::from(label.as_str()));
    sync_tool_chrome(ui, controller, tool);
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
    ui.set_tools_busy(controller.tools_busy);
    ui.set_smart_matte_label(SharedString::from(controller.smart_matte_label().as_str()));
    let can_tools = controller.can_run_smart_tools();
    ui.set_can_upscale(can_tools);
    ui.set_can_smart_matte(can_tools);
    ui.set_can_seam_carve(can_tools);
    super::sync_rulers(ui, controller);
    sync_color_picker(ui, controller);
    sync_layers(ui, controller);
}
