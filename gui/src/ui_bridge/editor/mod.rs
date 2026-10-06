mod color;
mod layers;
mod text;
mod tools;

pub use color::sync_color_picker;
pub use layers::{
    apply_filter_readout, apply_opacity_readout, sync_layer_rows, sync_layer_settings, sync_layers,
};
pub use tools::{sync_smart_tools, sync_tool_gate};

use super::landing::accent_color;
use super::{AppWindow, LayerListChrome, ProjectChrome, ProjectTabRow, ToolChrome, ZoomChrome};
use crate::shell::{format_bytes, tool_family_key, AppController};
use calumma_core::Tool;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

pub fn set_editor_open(ui: &AppWindow, controller: &mut AppController, open: bool) {
    controller.editor_open = open;
    ui.set_editor_open(open);
    if open {
        sync_editor(ui, controller);
    }
}

pub(super) fn put(value: String) -> SharedString {
    SharedString::from(value)
}

pub(super) fn put_rows<T: Clone + 'static>(current: ModelRc<T>, rows: Vec<T>) -> ModelRc<T> {
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
    ui.global::<ProjectChrome>()
        .set_tabs(ModelRc::new(VecModel::from(rows)));
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
    sync_tool_gate(ui, controller);
    let label = controller.l10n.get(tool_family_key(tool));
    ui.global::<ToolChrome>()
        .set_active_tool_label(SharedString::from(label.as_str()));
    let zoom_chrome = ui.global::<ZoomChrome>();
    zoom_chrome.set_zoom_unit(zoom_unit);
    zoom_chrome.set_zoom_text(SharedString::from(controller.l10n.format(
        "zoomPercent",
        &[&format!("{}", (zoom * 100.0).round() as i32)],
    )));
    zoom_chrome.set_is_fit(is_fit);
    ui.global::<super::SettingsChrome>()
        .set_memory_value(SharedString::from(memory));
    if let Some((width, height)) = controller.engine.borrow().document_size() {
        let list = ui.global::<LayerListChrome>();
        list.set_doc_width_text(SharedString::from(format!("{width}")));
        list.set_doc_height_text(SharedString::from(format!("{height}")));
    }
    sync_project_tabs(ui, controller);
    super::sync_rulers(ui, controller);
    super::sync_guides(ui, controller);
    super::sync_guide_readout(ui, controller);
    sync_color_picker(ui, controller);
    sync_layers(ui, controller);
}
