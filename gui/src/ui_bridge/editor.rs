use super::landing::accent_color;
use super::{
    brush, AppWindow, BrushEntry, FontFamilyRow, LayerChrome, LayerRow, ProjectTabRow, ToolChrome,
    ToolEntry,
};
use crate::shell::{
    brush_icon_index, brush_label_key, format_bytes, grid_slot_selected, grid_slot_tip_key,
    grid_slot_tool, hue_color, slint_color, tool_family_key, tool_icon_index, tool_label_key,
    AppController, BRUSHES, SELECT_TOOLS, SHAPE_TOOLS, TOOL_GRID,
};
use calumma_app::shortcuts::key_for_tool;
use calumma_app::{Engine, ToolBlock};
use calumma_core::{Tool, TEXT_LINE_HEIGHT_MAX, TEXT_LINE_HEIGHT_MIN};
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

pub fn sync_tool_gate(ui: &AppWindow, controller: &AppController) {
    let tool = controller
        .engine
        .borrow()
        .active_tool()
        .unwrap_or(Tool::Pen);
    ui.set_tools(put_rows(ui.get_tools(), tool_entries(controller, tool)));
    sync_tool_chrome(ui, controller, tool);
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
    sync_text_chrome(chrome, controller, tool);
    sync_color_tips(ui, controller, tool);
}

fn font_rows(query: &str) -> Vec<FontFamilyRow> {
    let query = query.trim().to_lowercase();
    Engine::font_families()
        .into_iter()
        .filter(|family| query.is_empty() || family.name.to_lowercase().contains(&query))
        .map(|family| FontFamilyRow {
            name: SharedString::from(family.name),
            has_bold: family.has_bold,
            has_italic: family.has_italic,
        })
        .collect()
}

fn sync_text_chrome(chrome: ToolChrome, controller: &AppController, tool: Tool) {
    chrome.set_text_line_height_min(TEXT_LINE_HEIGHT_MIN);
    chrome.set_text_line_height_max(TEXT_LINE_HEIGHT_MAX);
    if tool != Tool::Text {
        return;
    }
    let engine = controller.engine.borrow();
    let size = engine.text_size();
    let line_height = engine.text_line_height();
    let wrap = engine.text_wrap_width();
    let wrap_max = engine.text_wrap_max();
    chrome.set_text_family(put(engine.text_family()));
    chrome.set_text_size_unit(engine.text_size_unit());
    chrome.set_text_size_text(put(format!("{}", size.round() as i32)));
    chrome.set_text_line_height(line_height);
    chrome.set_text_line_height_text(put(format!("{line_height:.1}")));
    chrome.set_text_wrap_width(wrap);
    chrome.set_text_wrap_max(wrap_max);
    chrome.set_text_wrap_text(put(format!("{}", wrap.round() as i32)));
    chrome.set_text_bold(engine.text_bold());
    chrome.set_text_italic(engine.text_italic());
    chrome.set_text_can_bold(engine.text_can_bold());
    chrome.set_text_can_italic(engine.text_can_italic());
    chrome.set_text_align(engine.text_align() as i32);
    drop(engine);
    let query = chrome.get_text_font_query();
    chrome.set_font_families(put_rows(
        chrome.get_font_families(),
        font_rows(query.as_str()),
    ));
}

fn sync_color_tips(ui: &AppWindow, controller: &AppController, tool: Tool) {
    let put = |value: String| SharedString::from(value);
    if tool.takes_fill() {
        ui.set_primary_color_label(put(controller.l10n.get("strokeColor")));
        ui.set_secondary_color_label(put(controller.l10n.get("fillColor")));
    } else {
        ui.set_primary_color_label(put(controller.l10n.get("primaryColor")));
        ui.set_secondary_color_label(put(controller.l10n.get("secondaryColor")));
    }
    if tool == Tool::SelectColor {
        ui.set_tertiary_color_label(put(controller.l10n.get("matchColor")));
    } else {
        ui.set_tertiary_color_label(put(controller.l10n.get("tertiaryColor")));
    }
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

fn signed_percent(value: f32) -> String {
    format!("{:+}", (value * 100.0).round() as i32)
}

pub fn sync_layer_settings(ui: &AppWindow, controller: &AppController) {
    let chrome = ui.global::<LayerChrome>();
    if let Some(layer) = controller.layer_settings_summary() {
        let engine = controller.engine.borrow();
        let index = layer.index;
        let opacity = engine.layer_opacity(index);
        let adjustments = engine.layer_adjustments(index);
        let clipped = engine.is_layer_clipped(index);
        chrome.set_name(SharedString::from(layer.name.as_str()));
        chrome.set_visible(layer.visible);
        chrome.set_locked(layer.locked);
        chrome.set_can_delete(!layer.is_paper);
        chrome.set_can_rename(engine.can_rename_layer(index));
        chrome.set_opacity(opacity);
        chrome.set_opacity_text(put(format!("{}", (opacity * 100.0).round() as i32)));
        chrome.set_preview(controller.thumb_cache.preview_image(index));
        chrome.set_blend(engine.layer_blend_mode(index) as i32);
        chrome.set_brightness(adjustments.brightness);
        chrome.set_contrast(adjustments.contrast);
        chrome.set_vibrance(adjustments.vibrance);
        chrome.set_saturation(adjustments.saturation);
        chrome.set_gamma(adjustments.levels_gamma);
        chrome.set_brightness_text(put(signed_percent(adjustments.brightness)));
        chrome.set_contrast_text(put(signed_percent(adjustments.contrast)));
        chrome.set_vibrance_text(put(signed_percent(adjustments.vibrance)));
        chrome.set_saturation_text(put(signed_percent(adjustments.saturation)));
        chrome.set_gamma_text(put(format!("{:.2}", adjustments.levels_gamma)));
        chrome.set_clipped(clipped);
        chrome.set_can_clip(if clipped {
            engine.can_release_clipping_mask(index)
        } else {
            engine.can_create_clipping_mask(index)
        });
        chrome.set_can_flatten(engine.can_flatten_clip(index));
        chrome.set_can_merge(engine.can_merge_layer_down(index));
        chrome.set_can_reset_transform(engine.layer_has_transform(index) && !layer.locked);
        chrome.set_can_move_up(engine.can_move_layer_up(index));
        chrome.set_can_move_down(engine.can_move_layer_down(index));
        chrome.set_can_rasterize(engine.layer_is_rasterizable(index));
        chrome.set_lock_label(put(controller.l10n.get(if layer.locked {
            "layerUnlock"
        } else {
            "layerLock"
        })));
        chrome.set_clip_label(put(controller.l10n.get(if clipped {
            "releaseClippingMask"
        } else {
            "createClippingMask"
        })));
        drop(engine);
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
    ui.set_color_swatch3(brush(slint_color(colors.slots[3])));
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
    let thumbs_changed = controller
        .thumb_cache
        .sync(&controller.engine.borrow(), controller.prefs.is_dark());
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
                paper: layer.is_paper,
                clipped: layer.clipped,
                clip_base: layer.clip_base,
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
    sync_tool_gate(ui, controller);
}

pub fn sync_project_tabs(ui: &AppWindow, controller: &AppController) {
    let active = controller.active_project_id.as_deref();
    let rows: Vec<ProjectTabRow> = controller
        .open_tabs
        .iter()
        .filter_map(|id| {
            controller
                .engine
                .borrow()
                .project_summary(id)
                .map(|summary| ProjectTabRow {
                    id: SharedString::from(summary.id.as_str()),
                    name: SharedString::from(summary.name.as_str()),
                    accent: accent_color(&summary),
                    active: active == Some(id.as_str()),
                })
        })
        .collect();
    ui.set_project_tabs(ModelRc::new(VecModel::from(rows)));
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
            format_bytes(engine.resident_memory_bytes(), &controller.l10n),
        )
    };
    ui.set_active_tool(tool as i32);
    sync_tool_gate(ui, controller);
    let label = controller.l10n.get(tool_family_key(tool));
    ui.set_active_tool_label(SharedString::from(label.as_str()));
    ui.set_zoom_unit(zoom_unit);
    ui.set_zoom_text(SharedString::from(controller.l10n.format(
        "zoomPercent",
        &[&format!("{}", (zoom * 100.0).round() as i32)],
    )));
    ui.set_is_fit(is_fit);
    ui.set_memory_value(SharedString::from(memory));
    if let Some((width, height)) = controller.engine.borrow().document_size() {
        ui.set_doc_width_text(SharedString::from(format!("{width}")));
        ui.set_doc_height_text(SharedString::from(format!("{height}")));
    }
    sync_project_tabs(ui, controller);
    ui.set_tools_busy(controller.tools_busy);
    ui.set_smart_matte_label(SharedString::from(controller.smart_matte_label().as_str()));
    let can_tools = controller.can_run_smart_tools();
    ui.set_can_upscale(can_tools);
    ui.set_can_smart_matte(can_tools);
    ui.set_can_seam_carve(can_tools);
    super::sync_rulers(ui, controller);
    super::sync_guides(ui, controller);
    super::sync_guide_readout(ui, controller);
    sync_color_picker(ui, controller);
    sync_layers(ui, controller);
}
