slint::include_modules!();

mod editor;
mod guides;
mod landing;
mod rulers;
mod strings;
mod theme;

pub use editor::{
    set_editor_open, sync_editor, sync_layer_rows, sync_layer_settings, sync_layers,
    sync_project_tabs,
};
pub use guides::{sync_guide_readout, sync_guides};
pub use landing::{
    form_accent, parse_dimension, random_accent_index, refresh_landing, sync_recents,
};
pub use rulers::{camera_signature, sync_rulers, sync_zoom_chrome};
pub use strings::{init_form_defaults, sync_strings, DEFAULT_HEIGHT, DEFAULT_WIDTH};
pub use theme::apply_theme;

use crate::shell::AppController;
use slint::SharedString;

pub type SharedUi = slint::Weak<AppWindow>;

pub fn brush(color: slint::Color) -> slint::Brush {
    slint::Brush::SolidColor(color)
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
    ui.set_layer_settings_expanded(controller.layer_settings_expanded);
    ui.set_layer_settings_anchor_x(controller.layer_settings_anchor_x);
    ui.set_layer_settings_anchor_y(controller.layer_settings_anchor_y);
    ui.set_guides_open(controller.guides_open);
    ui.set_project_settings_open(controller.project_settings_open);
    ui.set_project_settings_anchor_x(controller.project_settings_anchor_x);
    ui.set_project_settings_anchor_y(controller.project_settings_anchor_y);
    if controller.project_settings_open {
        if let Some(summary) = controller
            .engine
            .borrow()
            .project_summary(&controller.project_settings_id)
        {
            ui.set_project_settings_size_text(SharedString::from(format!(
                "{} × {}",
                summary.width, summary.height
            )));
            ui.set_project_settings_accent_index(calumma_core::project_color_index(
                calumma_core::unpack_rgb(summary.accent_rgb),
            ) as i32);
        }
    }
    ui.set_toast_visible(controller.toast_visible);
    ui.set_toast_text(SharedString::from(controller.toast_text.as_str()));
    ui.set_toast_is_error(controller.toast_is_error);
    ui.set_layers_open(controller.prefs.layers_panel_open);
    if controller.editor_open {
        editor::sync_project_tabs(ui, controller);
    }
    ui.set_can_undo(controller.can_undo());
    ui.set_can_redo(controller.can_redo());
    sync_layer_settings(ui, controller);
    guides::sync_guide_palette(ui);
    guides::sync_guides(ui, controller);
    guides::sync_guide_readout(ui, controller);
}
