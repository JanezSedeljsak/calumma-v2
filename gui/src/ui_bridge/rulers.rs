use super::{AppWindow, RulerTickRow};
use crate::shell::AppController;
use slint::{ModelRc, SharedString, VecModel};

fn rows(ticks: &[calumma_core::RulerTick], zoom: f32, pan: f32) -> Vec<RulerTickRow> {
    ticks
        .iter()
        .map(|tick| RulerTickRow {
            offset: tick.doc * zoom + pan,
            label: SharedString::from(format!("{}", tick.doc.round() as i32)),
            major: tick.major,
        })
        .collect()
}

pub fn sync_rulers(ui: &AppWindow, controller: &AppController) {
    let engine = controller.engine.borrow();
    let zoom = engine.zoom_factor();
    let (pan_x, pan_y) = engine.camera_pan();
    let x = rows(&engine.ruler_ticks_x(), zoom, pan_x);
    let y = rows(&engine.ruler_ticks_y(), zoom, pan_y);
    drop(engine);
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
    ui.set_zoom_text(SharedString::from(format!("{}%", (zoom * 100.0).round() as i32)));
    ui.set_is_fit(is_fit);
}

pub fn camera_signature(controller: &AppController) -> (f32, f32, f32) {
    let engine = controller.engine.borrow();
    let (pan_x, pan_y) = engine.camera_pan();
    (engine.zoom_factor(), pan_x, pan_y)
}
