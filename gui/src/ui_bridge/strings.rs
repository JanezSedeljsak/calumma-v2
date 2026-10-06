use super::{
    AppWindow, ColorChrome, GuideChrome, LayerChrome, LayerListChrome, MenuChrome, ProjectChrome,
    SettingsChrome, ToolChrome, ZoomChrome,
};
use crate::shell::Catalog;
use slint::{ComponentHandle, SharedString};

pub const DEFAULT_WIDTH: u32 = 1280;
pub const DEFAULT_HEIGHT: u32 = 720;

pub fn init_form_defaults(ui: &AppWindow, l10n: &Catalog) {
    let project = ui.global::<ProjectChrome>();
    project.set_name(SharedString::from(l10n.get("newProject")));
    project.set_width_text(SharedString::from(DEFAULT_WIDTH.to_string()));
    project.set_height_text(SharedString::from(DEFAULT_HEIGHT.to_string()));
    project.set_accent_index(super::random_accent_index());
    ui.global::<GuideChrome>()
        .set_add_offset(SharedString::from("0"));
    ui.global::<GuideChrome>().set_add_horizontal(true);
}

pub fn sync_strings(ui: &AppWindow, l10n: &Catalog) {
    let put = |value: String| SharedString::from(value);

    let project = ui.global::<ProjectChrome>();
    project.set_brand_text(put(l10n.get("brand")));
    project.set_tagline_text(put(l10n.get("tagline")));
    project.set_name_label(put(l10n.get("projectName")));
    project.set_resolution_label(put(l10n.get("resolution")));
    project.set_create_label(put(l10n.get("create")));
    project.set_presets_label(put(l10n.get("presets")));
    project.set_recents_label(put(l10n.get("recents")));
    project.set_clear_all_label(put(l10n.get("clearAllRecents")));
    project.set_no_recents_label(put(l10n.get("noRecents")));
    project.set_delete_label(put(l10n.get("deleteProject")));
    project.set_paste_artwork_title(put(l10n.get("pasteArtwork")));
    project.set_paste_artwork_hint(put(l10n.get("pasteArtworkHint")));
    project.set_artwork_formats(put(l10n.get("artworkFormats")));
    project.set_close_tab_tip(put(l10n.get("closeProjectTab")));

    let settings = ui.global::<SettingsChrome>();
    settings.set_title_text(put(l10n.get("settings")));
    settings.set_theme_label(put(l10n.get("theme")));
    settings.set_theme_light_label(put(l10n.get("themeLight")));
    settings.set_theme_dark_label(put(l10n.get("themeDark")));
    settings.set_language_label(put(l10n.get("language")));
    settings.set_language_name(put(l10n.get("languageEnglish")));
    settings.set_memory_label(put(l10n.get("memoryUsed")));
    settings.set_version_label(put(l10n.get("version")));

    let color = ui.global::<ColorChrome>();
    color.set_label(put(l10n.get("color")));
    color.set_primary_tip(put(l10n.get("primaryColor")));
    color.set_secondary_tip(put(l10n.get("secondaryColor")));
    color.set_tertiary_tip(put(l10n.get("tertiaryColor")));
    color.set_quaternary_tip(put(l10n.get("quaternaryColor")));

    let menu = ui.global::<MenuChrome>();
    menu.set_file_title(put(l10n.get("fileMenu")));
    menu.set_edit_title(put(l10n.get("editMenu")));
    menu.set_board_title(put(l10n.get("boardMenu")));
    menu.set_new_project_title(put(l10n.get("newProjectMenu")));
    menu.set_undo_title(put(l10n.get("undo")));
    menu.set_redo_title(put(l10n.get("redo")));
    menu.set_settings_title(put(l10n.get("settings")));
    menu.set_fit_view_title(put(l10n.get("fitToView")));
    menu.set_toggle_layers_title(put(l10n.get("toggleLayers")));
    menu.set_fullscreen_title(put(l10n.get("enterFullScreen")));
    menu.set_export_title(put(l10n.get("exportMenu")));
    menu.set_export_png_title(put(l10n.format("exportAs", &["PNG"])));
    menu.set_export_jpeg_title(put(l10n.format("exportAs", &["JPEG"])));
    menu.set_export_webp_title(put(l10n.format("exportAs", &["WebP"])));
    menu.set_export_avif_title(put(l10n.format("exportAs", &["AVIF"])));
    menu.set_export_heic_title(put(l10n.format("exportAs", &["HEIC"])));
    menu.set_export_psd_title(put(l10n.format("exportAs", &["PSD"])));
    menu.set_export_svg_title(put(l10n.format("exportAs", &["SVG"])));
    menu.set_export_pdf_title(put(l10n.format("exportAs", &["PDF"])));

    let zoom = ui.global::<ZoomChrome>();
    zoom.set_fit_tip(put(l10n.get("fitToView")));
    zoom.set_zoom_in_tip(put(l10n.get("zoomIn")));
    zoom.set_zoom_out_tip(put(l10n.get("zoomOut")));

    let guides = ui.global::<GuideChrome>();
    guides.set_open_tip(put(l10n.get("guides")));
    guides.set_title_text(put(l10n.get("guides")));
    guides.set_hint_text(put(l10n.get("guidesHint")));
    guides.set_empty_text(put(l10n.get("noGuides")));
    guides.set_add_label(put(l10n.get("addGuide")));
    guides.set_full_text(put(l10n.get("guidesFull")));
    guides.set_top_label(put(l10n.get("guideTop")));
    guides.set_left_label(put(l10n.get("guideLeft")));
    guides.set_delete_tip(put(l10n.get("deleteGuide")));
    guides.set_clear_label(put(l10n.get("clearGuides")));

    let list = ui.global::<LayerListChrome>();
    list.set_title_text(put(l10n.get("layers")));
    list.set_add_tip(put(l10n.get("addLayer")));
    list.set_bounds_label(put(l10n.get("layerBounds")));
    list.set_bounds_x_label(put(l10n.get("layerBoundsX")));
    list.set_bounds_y_label(put(l10n.get("layerBoundsY")));
    list.set_width_label(put(l10n.get("canvasWidth")));
    list.set_height_label(put(l10n.get("canvasHeight")));
    list.set_settings_tip(put(l10n.get("layerSettings")));
    list.set_delete_tip(put(l10n.get("deleteLayer")));
    list.set_visibility_tip(put(l10n.get("layerVisibility")));

    let chrome = ui.global::<ToolChrome>();
    chrome.set_brush_size_label(put(l10n.get("brushSize")));
    chrome.set_ink_opacity_label(put(l10n.get("inkOpacity")));
    chrome.set_blur_label(put(l10n.get("blurStrength")));
    chrome.set_hardness_label(put(l10n.get("eraserHardness")));
    chrome.set_tolerance_label(put(l10n.get("tolerance")));
    chrome.set_sample_label(put(l10n.get("sampleSize")));
    chrome.set_fill_label(put(l10n.get("fill")));
    chrome.set_stroke_label(put(l10n.get("stroke")));
    chrome.set_vector_label(put(l10n.get("vectorMode")));
    chrome.set_aligned_label(put(l10n.get("cloneAligned")));
    chrome.set_transform_label(put(l10n.get("toolTransform")));
    chrome.set_crop_aspect_label(put(l10n.get("cropAspectRatio")));
    chrome.set_crop_overlay_label(put(l10n.get("cropOverlay")));
    chrome.set_crop_free_label(put(l10n.get("cropAspectFree")));
    chrome.set_crop_square_label(put(l10n.get("cropAspectSquare")));
    chrome.set_crop_four_three_label(put(l10n.get("cropAspectFourThree")));
    chrome.set_crop_three_two_label(put(l10n.get("cropAspectThreeTwo")));
    chrome.set_crop_sixteen_nine_label(put(l10n.get("cropAspectSixteenNine")));
    chrome.set_crop_five_four_label(put(l10n.get("cropAspectFiveFour")));
    chrome.set_crop_overlay_off_label(put(l10n.get("cropOverlayOff")));
    chrome.set_crop_overlay_thirds_label(put(l10n.get("cropOverlayThirds")));
    chrome.set_crop_overlay_grid_label(put(l10n.get("cropOverlayGrid")));
    chrome.set_crop_overlay_diagonal_label(put(l10n.get("cropOverlayDiagonal")));
    chrome.set_crop_overlay_golden_label(put(l10n.get("cropOverlayGoldenRatio")));
    chrome.set_crop_cancel_label(put(l10n.get("cropCancel")));
    chrome.set_crop_commit_label(put(l10n.get("cropCommit")));
    chrome.set_text_font_label(put(l10n.get("textFont")));
    chrome.set_text_size_label(put(l10n.get("textSize")));
    chrome.set_text_line_height_label(put(l10n.get("textLineHeight")));
    chrome.set_text_wrap_label(put(l10n.get("textWrapWidth")));
    chrome.set_text_bold_label(put(l10n.get("textBold")));
    chrome.set_text_italic_label(put(l10n.get("textItalic")));
    chrome.set_text_align_left_label(put(l10n.get("textAlignLeft")));
    chrome.set_text_align_center_label(put(l10n.get("textAlignCenter")));
    chrome.set_text_align_right_label(put(l10n.get("textAlignRight")));
    chrome.set_text_no_fonts_label(put(l10n.get("textNoFonts")));
    chrome.set_smart_tools_label(put(l10n.get("smartTools")));

    let layers = ui.global::<LayerChrome>();
    layers.set_visibility_label(put(l10n.get("layerVisibility")));
    layers.set_align_left_label(put(l10n.get("alignLeft")));
    layers.set_align_center_h_label(put(l10n.get("alignCenterH")));
    layers.set_align_right_label(put(l10n.get("alignRight")));
    layers.set_align_top_label(put(l10n.get("alignTop")));
    layers.set_align_center_v_label(put(l10n.get("alignCenterV")));
    layers.set_align_bottom_label(put(l10n.get("alignBottom")));
    layers.set_distribute_h_label(put(l10n.get("distributeH")));
    layers.set_distribute_v_label(put(l10n.get("distributeV")));
    layers.set_opacity_label(put(l10n.get("opacity")));
    layers.set_blend_label(put(l10n.get("blendMode")));
    layers.set_blend_options(blend_options(l10n));
    layers.set_filters_label(put(l10n.get("filters")));
    layers.set_brightness_label(put(l10n.get("brightness")));
    layers.set_contrast_label(put(l10n.get("contrast")));
    layers.set_vibrance_label(put(l10n.get("vibrance")));
    layers.set_saturation_label(put(l10n.get("saturation")));
    layers.set_gamma_label(put(l10n.get("levelsGamma")));
    layers.set_hue_label(put(l10n.get("hue")));
    layers.set_reset_filters_label(put(l10n.get("resetFilters")));
    layers.set_rename_label(put(l10n.get("renameLayer")));
    layers.set_flatten_label(put(l10n.get("flattenClippingMask")));
    layers.set_mask_label(put(l10n.get("addLayerMask")));
    layers.set_merge_label(put(l10n.get("mergeLayerDown")));
    layers.set_reset_transform_label(put(l10n.get("resetTransform")));
    layers.set_move_up_label(put(l10n.get("moveLayerUp")));
    layers.set_move_down_label(put(l10n.get("moveLayerDown")));
    layers.set_rasterize_label(put(l10n.get("rasterizeLayer")));
    layers.set_export_label(put(l10n.get("exportLayer")));
    layers.set_duplicate_label(put(l10n.get("duplicateLayer")));
    layers.set_delete_label(put(l10n.get("deleteLayer")));
}

/// The blend menu in the engine's order and groups, a divider opening every group after the
/// first — rebuilt with the rest of the strings, so switching language relabels it.
fn blend_options(l10n: &Catalog) -> slint::ModelRc<super::SelectOption> {
    let options: Vec<super::SelectOption> = calumma_core::BlendMode::MENU
        .iter()
        .enumerate()
        .flat_map(|(group, modes)| {
            modes
                .iter()
                .enumerate()
                .map(move |(i, &mode)| super::SelectOption {
                    label: SharedString::from(l10n.get(crate::shell::blend_label_key(mode))),
                    value: mode.as_u32() as i32,
                    group_start: group > 0 && i == 0,
                })
        })
        .collect();
    slint::ModelRc::new(slint::VecModel::from(options))
}
