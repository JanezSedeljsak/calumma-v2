use super::ruler_labels::ruler_label;
use super::{AppWindow, GuideChrome, RulerTickRow, Theme, Tokens, ZoomChrome};
use crate::shell::AppController;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

const LABEL_GAP_PX: f32 = 3.0;
const LABEL_INSET_PX: f32 = 1.0;

/// Where one ruler strip sits, in logical points, and how labels are drawn on it. Everything
/// a tick or label lands on is rounded to a whole *device* pixel against the window, not the
/// strip — a strip that a resize left on a fractional pixel would otherwise blur every label.
struct Strip {
    along_origin: f32,
    cross_origin: f32,
    scale: f32,
    turned: bool,
    label_px: f32,
    color: [u8; 4],
}

impl Strip {
    fn device(&self, value: f32) -> f32 {
        (value * self.scale).round()
    }

    fn row(&self, tick: &calumma_core::RulerTick, zoom: f32, pan: f32) -> RulerTickRow {
        let along = self.device(self.along_origin + tick.doc * zoom + pan);
        let mut row = RulerTickRow {
            offset: along / self.scale - self.along_origin,
            major: tick.major,
            ..Default::default()
        };
        if !tick.major {
            return row;
        }
        let text = format!("{}", tick.doc.round() as i32);
        let Some(label) = ruler_label(&text, self.label_px, self.color, self.turned) else {
            return row;
        };
        let gap = self.device(LABEL_GAP_PX);
        let cross = self.device(self.cross_origin) + self.device(LABEL_INSET_PX);
        let (left, top) = if self.turned {
            (cross, along - gap)
        } else {
            (along + gap, cross)
        };
        let left = (left + label.ink_x as f32) / self.scale;
        let top = (top + label.ink_y as f32) / self.scale;
        let (x0, y0) = if self.turned {
            (self.cross_origin, self.along_origin)
        } else {
            (self.along_origin, self.cross_origin)
        };
        row.label = label.image.clone();
        row.label_x = left - x0;
        row.label_y = top - y0;
        row.label_width = label.width as f32 / self.scale;
        row.label_height = label.height as f32 / self.scale;
        row
    }

    fn rows(&self, ticks: &[calumma_core::RulerTick], zoom: f32, pan: f32) -> Vec<RulerTickRow> {
        ticks.iter().map(|tick| self.row(tick, zoom, pan)).collect()
    }
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
    let thickness = ui.global::<Tokens>().get_ruler_thickness();
    let label_px = ui.global::<Tokens>().get_label_size() * scale;
    let muted = ui.global::<Theme>().get_text_muted();
    let color = [muted.red(), muted.green(), muted.blue(), muted.alpha()];
    let (board_x, board_y) = (ui.get_board_x(), ui.get_board_y());
    let top = Strip {
        along_origin: board_x,
        cross_origin: board_y - thickness,
        scale,
        turned: false,
        label_px,
        color,
    };
    let left = Strip {
        along_origin: board_y,
        cross_origin: board_x - thickness,
        turned: true,
        ..top
    };
    let guides = ui.global::<GuideChrome>();
    guides.set_ruler_ticks_x(ModelRc::new(VecModel::from(top.rows(ticks_x, zoom, pan_x))));
    guides.set_ruler_ticks_y(ModelRc::new(VecModel::from(
        left.rows(ticks_y, zoom, pan_y),
    )));
}

pub fn sync_zoom_chrome(ui: &AppWindow, controller: &AppController) {
    let engine = controller.engine.borrow();
    let unit = engine.zoom_unit();
    let zoom = engine.zoom_factor();
    let is_fit = engine.is_fit();
    drop(engine);
    let chrome = ui.global::<ZoomChrome>();
    chrome.set_zoom_unit(unit);
    chrome.set_zoom_text(SharedString::from(controller.l10n.format(
        "zoomPercent",
        &[&format!("{}", (zoom * 100.0).round() as i32)],
    )));
    chrome.set_is_fit(is_fit);
}

pub fn camera_signature(controller: &AppController) -> (f32, f32, f32) {
    let engine = controller.engine.borrow();
    let (pan_x, pan_y) = engine.camera_pan();
    (engine.zoom_factor(), pan_x, pan_y)
}
