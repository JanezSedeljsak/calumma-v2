slint::include_modules!();

use crate::shell::{format_bytes, hue_color, relative_time, slint_color, tool_label_key, AppController, Catalog, Theme};
use calumma_app::{Engine, ProjectSummary};
use calumma_core::Tool;
use slint::{Color, ModelRc, SharedString, VecModel};

pub fn brush(color: Color) -> slint::Brush {
    slint::Brush::SolidColor(color)
}

pub fn sync_strings(ui: &AppWindow, l10n: &Catalog) {
    ui.set_brand_text(SharedString::from(l10n.get("brand")));
    ui.set_tagline_text(SharedString::from(l10n.get("tagline")));
    ui.set_project_name_label(SharedString::from(l10n.get("projectName")));
    ui.set_resolution_label(SharedString::from(l10n.get("resolution")));
    ui.set_create_label(SharedString::from(l10n.get("create")));
    ui.set_presets_label(SharedString::from(l10n.get("presets")));
    ui.set_recents_label(SharedString::from(l10n.get("recents")));
    ui.set_clear_all_label(SharedString::from(l10n.get("clearAllRecents")));
    ui.set_paste_artwork_title(SharedString::from(l10n.get("pasteArtwork")));
    ui.set_paste_artwork_hint(SharedString::from(l10n.get("pasteArtworkHint")));
    ui.set_artwork_formats(SharedString::from(l10n.get("artworkFormats")));
    ui.set_back_label(SharedString::from(l10n.get("backToProjects")));
    ui.set_brush_size_label(SharedString::from(l10n.get("brushSize")));
    ui.set_ink_opacity_label(SharedString::from(l10n.get("inkOpacity")));
    ui.set_settings_title(SharedString::from(l10n.get("settings")));
    ui.set_theme_label(SharedString::from(l10n.get("theme")));
    ui.set_theme_light_label(SharedString::from(l10n.get("themeLight")));
    ui.set_theme_dark_label(SharedString::from(l10n.get("themeDark")));
    ui.set_language_label(SharedString::from(l10n.get("language")));
    ui.set_memory_label(SharedString::from(l10n.get("memoryUsed")));
    ui.set_version_label(SharedString::from(l10n.get("version")));
    ui.set_color_label(SharedString::from(l10n.get("color")));
    ui.set_project_name(SharedString::from(l10n.get("newProject")));
    ui.set_file_menu_title(SharedString::from(l10n.get("fileMenu")));
    ui.set_edit_menu_title(SharedString::from(l10n.get("editMenu")));
    ui.set_board_menu_title(SharedString::from(l10n.get("boardMenu")));
    ui.set_new_project_menu_title(SharedString::from(l10n.get("newProjectMenu")));
    ui.set_undo_menu_title(SharedString::from(l10n.get("undo")));
    ui.set_redo_menu_title(SharedString::from(l10n.get("redo")));
    ui.set_fit_view_menu_title(SharedString::from(l10n.get("fitToView")));
    ui.set_toggle_layers_menu_title(SharedString::from(l10n.get("toggleLayers")));
    ui.set_fullscreen_menu_title(SharedString::from(l10n.get("enterFullScreen")));
    ui.set_layers_title(SharedString::from(l10n.get("layers")));
    ui.set_add_layer_label(SharedString::from(l10n.get("addLayer")));
    ui.set_export_menu_title(SharedString::from(l10n.get("exportMenu")));
    ui.set_export_png_title(SharedString::from(l10n.format("exportAs", &["PNG"])));
    ui.set_export_jpeg_title(SharedString::from(l10n.format("exportAs", &["JPEG"])));
    ui.set_export_webp_title(SharedString::from(l10n.format("exportAs", &["WebP"])));
    ui.set_export_avif_title(SharedString::from(l10n.format("exportAs", &["AVIF"])));
    ui.set_export_heic_title(SharedString::from(l10n.format("exportAs", &["HEIC"])));
    ui.set_export_psd_title(SharedString::from(l10n.format("exportAs", &["PSD"])));
    ui.set_export_svg_title(SharedString::from(l10n.format("exportAs", &["SVG"])));
    ui.set_export_pdf_title(SharedString::from(l10n.format("exportAs", &["PDF"])));
    ui.set_tools_menu_title(SharedString::from(l10n.get("smartTools")));
    ui.set_upscale_label(SharedString::from(l10n.get("upscale")));
    ui.set_seam_carve_label(SharedString::from(l10n.get("seamCarve")));
    ui.set_layer_settings_title(SharedString::from(l10n.get("layerSettings")));
    ui.set_layer_export_label(SharedString::from(l10n.get("exportLayer")));
    ui.set_layer_duplicate_label(SharedString::from(l10n.get("duplicateLayer")));
    ui.set_layer_delete_label(SharedString::from(l10n.get("deleteLayer")));
}

pub fn apply_theme(ui: &AppWindow, theme: &Theme) {
    ui.set_theme_bg(brush(theme.bg));
    ui.set_theme_surface(brush(theme.surface));
    ui.set_theme_surface_hover(brush(theme.surface_hover));
    ui.set_theme_text(brush(theme.text));
    ui.set_theme_text_muted(brush(theme.text_muted));
    ui.set_theme_accent_teal(brush(theme.accent_teal));
    ui.set_theme_accent_orange(brush(theme.accent_orange));
    ui.set_theme_island_border(brush(theme.island_border));
    ui.set_theme_control_border(brush(theme.control_border));
    ui.set_theme_control_focus_border(brush(theme.control_focus_border));
    ui.set_theme_tool_selected(brush(theme.tool_selected));
    ui.set_theme_danger(brush(theme.danger));

    let presets: Vec<PresetRow> = theme
        .presets
        .iter()
        .map(|p| PresetRow {
            label: SharedString::from(p.label.as_str()),
            width: p.width as i32,
            height: p.height as i32,
        })
        .collect();
    ui.set_presets(ModelRc::new(VecModel::from(presets)));
}

pub fn sync_recents(ui: &AppWindow, controller: &AppController) {
    let engine = controller.engine.borrow();
    let rows: Vec<RecentRow> = controller
        .refresh_recents()
        .iter()
        .map(|item| RecentRow {
            id: SharedString::from(item.id.as_str()),
            name: SharedString::from(item.name.as_str()),
            size_text: SharedString::from(format!("{} x {}", item.width, item.height)),
            time_text: SharedString::from(relative_time(item.opened_at)),
            accent: accent_color(item),
            thumb: recent_thumb(&engine, &item.id),
        })
        .collect();
    ui.set_recents(ModelRc::new(VecModel::from(rows)));
}

fn recent_thumb(engine: &Engine, id: &str) -> slint::Image {
    if let Some((w, h, rgba)) = engine.project_thumbnail_rgba(id) {
        let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&rgba, w, h);
        return slint::Image::from_rgba8(buffer);
    }
    placeholder_thumb()
}

fn placeholder_thumb() -> slint::Image {
    let side = 44u32;
    let mut rgba = vec![0u8; (side * side * 4) as usize];
    for y in 0..side {
        for x in 0..side {
            let i = ((y * side + x) * 4) as usize;
            let v = if (x + y) % 8 < 4 { 48 } else { 40 };
            rgba[i] = v;
            rgba[i + 1] = v;
            rgba[i + 2] = v;
            rgba[i + 3] = 255;
        }
    }
    let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&rgba, side, side);
    slint::Image::from_rgba8(buffer)
}

fn accent_color(item: &ProjectSummary) -> Color {
    let hex = Engine::accent_hex(item);
    let trimmed = hex.trim_start_matches('#');
    let value = u32::from_str_radix(trimmed, 16).unwrap_or(0x3aa6a6);
    Color::from_rgb_u8(
        ((value >> 16) & 0xFF) as u8,
        ((value >> 8) & 0xFF) as u8,
        (value & 0xFF) as u8,
    )
}

pub fn parse_dimension(text: &str, fallback: u32) -> u32 {
    text.trim().parse::<u32>().unwrap_or(fallback).max(1)
}

pub fn sync_shell(ui: &AppWindow, controller: &AppController) {
    sync_strings(ui, &controller.l10n);
    apply_theme(ui, &controller.theme);
    sync_recents(ui, controller);
    ui.set_theme_is_dark(controller.prefs.is_dark());
    ui.set_language_is_en(controller.prefs.language == "en");
    ui.set_memory_value(SharedString::from(controller.memory_label()));
    ui.set_version_value(SharedString::from(env!("CARGO_PKG_VERSION")));
    ui.set_settings_open(controller.settings_open);
    ui.set_new_project_open(controller.new_project_open);
    ui.set_layer_settings_open(controller.layer_settings_open);
    ui.set_toast_visible(controller.toast_visible);
    ui.set_toast_text(SharedString::from(controller.toast_text.as_str()));
    ui.set_toast_is_error(controller.toast_is_error);
    ui.set_layers_open(controller.prefs.layers_panel_open);
    ui.set_can_undo(controller.can_undo());
    ui.set_can_redo(controller.can_redo());
    sync_layer_settings(ui, controller);
}

fn layer_visibility_label(controller: &AppController, visible: bool) -> String {
    if visible {
        controller.l10n.get("hideLayer")
    } else {
        controller.l10n.get("showLayer")
    }
}

pub fn sync_layer_settings(ui: &AppWindow, controller: &AppController) {
    if let Some(layer) = controller.layer_settings_summary() {
        ui.set_layer_settings_name(SharedString::from(layer.name.as_str()));
        ui.set_layer_settings_index(layer.index as i32);
        ui.set_layer_settings_visible(layer.visible);
        ui.set_layer_settings_can_delete(!layer.is_paper);
        ui.set_layer_visibility_label(SharedString::from(
            layer_visibility_label(controller, layer.visible).as_str(),
        ));
    }
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
    ui.set_color_hue_brush(brush(hue_color(colors.hsb.hue)));
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
    ui.set_layers(ModelRc::new(VecModel::from(rows)));
    sync_layer_settings(ui, controller);
}

pub fn refresh_landing(ui: &AppWindow, controller: &AppController) {
    sync_shell(ui, controller);
}

pub type SharedUi = slint::Weak<AppWindow>;

pub fn set_editor_open(ui: &AppWindow, controller: &mut AppController, open: bool) {
    controller.editor_open = open;
    ui.set_editor_open(open);
    if open {
        sync_editor(ui, controller);
    }
}

pub fn sync_editor(ui: &AppWindow, controller: &mut AppController) {
    let (tool, brush_size_unit, brush_size, ink_opacity, zoom_unit, memory) = {
        let engine = controller.engine.borrow();
        let tool = engine.active_tool().unwrap_or(Tool::Pen);
        (
            tool,
            engine.brush_size_unit(),
            engine.brush_size(),
            engine.ink_opacity(),
            engine.zoom_unit(),
            format_bytes(engine.resident_memory_bytes()),
        )
    };
    ui.set_active_tool(tool as i32);
    let label = controller.l10n.get(tool_label_key(tool)).to_uppercase();
    ui.set_active_tool_label(SharedString::from(label.as_str()));
    ui.set_show_brush_size(tool.takes_brush());
    ui.set_show_ink_opacity(tool.takes_ink_opacity());
    ui.set_brush_size_unit(brush_size_unit);
    ui.set_brush_size_text(SharedString::from(format!("{}", brush_size.round() as i32)));
    ui.set_ink_opacity(ink_opacity);
    ui.set_ink_opacity_text(SharedString::from(format!(
        "{}%",
        (ink_opacity * 100.0).round() as i32
    )));
    ui.set_zoom_unit(zoom_unit);
    ui.set_memory_value(SharedString::from(memory));
    if let Some((width, height)) = controller.engine.borrow().document_size() {
        ui.set_doc_width_text(SharedString::from(format!("{}", width)));
        ui.set_doc_height_text(SharedString::from(format!("{}", height)));
    }
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
