slint::include_modules!();

mod editor;
mod landing;
mod strings;
mod theme;

pub use editor::{set_editor_open, sync_editor, sync_layer_settings, sync_layers};
pub use landing::{parse_dimension, refresh_landing, sync_recents};
pub use strings::{init_form_defaults, sync_strings, DEFAULT_HEIGHT, DEFAULT_WIDTH};
pub use theme::{apply_theme, load_app_icon};

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
    ui.set_toast_visible(controller.toast_visible);
    ui.set_toast_text(SharedString::from(controller.toast_text.as_str()));
    ui.set_toast_is_error(controller.toast_is_error);
    ui.set_layers_open(controller.prefs.layers_panel_open);
    ui.set_can_undo(controller.can_undo());
    ui.set_can_redo(controller.can_redo());
    sync_layer_settings(ui, controller);
}
