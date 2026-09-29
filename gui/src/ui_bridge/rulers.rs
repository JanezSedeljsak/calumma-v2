use super::{AppWindow, RulerTickRow};
use crate::shell::AppController;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

fn snap_offset(offset: f32, scale: f32) -> f32 {
    (offset * scale).round() / scale
}

fn rows(ticks: &[calumma_core::RulerTick], zoom: f32, pan: f32, scale: f32) -> Vec<RulerTickRow> {
    ticks
        .iter()
        .map(|tick| RulerTickRow {
            offset: snap_offset(tick.doc * zoom + pan, scale),
            label: SharedString::from(format!("{}", tick.doc.round() as i32)),
            major: tick.major,
        })
        .collect()
}

pub fn sync_rulers(ui: &AppWindow, controller: &AppController) {
    let engine = controller.engine.borrow();
    let zoom = engine.zoom_factor();
    let (pan_x, pan_y) = engine.camera_pan();
    let ticks_x = engine.ruler_ticks_x();
    let ticks_y = engine.ruler_ticks_y();
    drop(engine);
    set_rulers(ui, &ticks_x, &ticks_y, zoom, (pan_x, pan_y));
}

pub fn sync_ruler_preview(ui: &AppWindow, camera: &calumma_core::Camera) {
    set_rulers(
        ui,
        &camera.ruler_ticks_x(),
        &camera.ruler_ticks_y(),
        camera.zoom,
        (camera.pan_x, camera.pan_y),
    );
}

fn set_rulers(
    ui: &AppWindow,
    ticks_x: &[calumma_core::RulerTick],
    ticks_y: &[calumma_core::RulerTick],
    zoom: f32,
    (pan_x, pan_y): (f32, f32),
) {
    let scale = ui.window().scale_factor().max(0.01);
    let x = rows(ticks_x, zoom, pan_x, scale);
    let y = rows(ticks_y, zoom, pan_y, scale);
    ui.set_ruler_ticks_x(ModelRc::new(VecModel::from(x)));
    ui.set_ruler_ticks_y(ModelRc::new(VecModel::from(y)));
}

pub fn sync_zoom_chrome(ui: &AppWindow, controller: &AppController) {
    let engine = controller.engine.borrow();
    let unit = engine.zoom_unit();
    let zoom = engine.zoom_factor();
    let is_fit = engine.is_fit();
    drop(engine);
    ui.set_zoom_unit(unit);
    ui.set_zoom_text(SharedString::from(controller.l10n.format(
        "zoomPercent",
        &[&format!("{}", (zoom * 100.0).round() as i32)],
    )));
    ui.set_is_fit(is_fit);
}

pub fn camera_signature(controller: &AppController) -> (f32, f32, f32) {
    let engine = controller.engine.borrow();
    let (pan_x, pan_y) = engine.camera_pan();
    (engine.zoom_factor(), pan_x, pan_y)
}
