use crate::board::{board_layout, hole_in_board, BoardHost, BoardLayout, BoardRect};
use crate::ui_bridge::{AppWindow, SharedUi};
use i_slint_backend_winit::WinitWindowAccessor;
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;

pub fn setup_board(ui_weak: &SharedUi, host: &Rc<RefCell<BoardHost>>) {
    if let Some(ui) = ui_weak.upgrade() {
        let snapshot = board_geo(&ui);
        sync_board_geometry(&ui, host, &snapshot);
        if !snapshot.overlay {
            host.borrow_mut().render();
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct BoardGeo {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    content_height: i32,
    scale: u32,
    overlay: bool,
}

pub struct BoardGeoSnapshot {
    pub layout: BoardLayout,
    pub content_height: f32,
    pub scale: f32,
    pub overlay: bool,
    pub key: BoardGeo,
}

pub fn board_geo(ui: &AppWindow) -> BoardGeoSnapshot {
    let scale = ui.window().scale_factor();
    let content_height = ui.window().size().to_logical(scale).height;
    let overlay = ui.get_overlay_chrome_open();
    let layout = board_layout(
        ui.get_board_x(),
        ui.get_board_y(),
        ui.get_board_width(),
        ui.get_board_height(),
    );
    BoardGeoSnapshot {
        key: BoardGeo {
            x: (layout.x * 100.0).round() as i32,
            y: (layout.y * 100.0).round() as i32,
            width: layout.width,
            height: layout.height,
            content_height: (content_height * 100.0).round() as i32,
            scale: (scale * 1000.0).round() as u32,
            overlay,
        },
        layout,
        content_height,
        scale,
        overlay,
    }
}

fn collect_board_holes(ui: &AppWindow, layout: &BoardLayout) -> Vec<BoardRect> {
    let mut holes = Vec::new();
    let mut punch = |chrome: BoardRect| {
        if let Some(hole) = hole_in_board(layout, chrome) {
            holes.push(hole);
        }
    };
    if !ui.get_layer_settings_open() {
        punch(BoardRect {
            x: ui.get_zoom_chrome_x(),
            y: ui.get_zoom_chrome_y(),
            width: ui.get_zoom_chrome_width(),
            height: ui.get_zoom_chrome_height(),
            radius: ui.get_zoom_chrome_radius(),
        });
    }
    if ui.get_hover_preview_visible() && !ui.get_layer_settings_open() {
        punch(BoardRect {
            x: ui.get_hover_chrome_x(),
            y: ui.get_hover_chrome_y(),
            width: ui.get_hover_chrome_width(),
            height: ui.get_hover_chrome_height(),
            radius: ui.get_hover_chrome_radius(),
        });
    }
    if ui.get_guides_open() {
        punch(BoardRect {
            x: ui.get_guides_chrome_x(),
            y: ui.get_guides_chrome_y(),
            width: ui.get_guides_chrome_width(),
            height: ui.get_guides_chrome_height(),
            radius: ui.get_guides_chrome_radius(),
        });
    }
    if ui.get_new_project_open() {
        punch(BoardRect {
            x: ui.get_new_project_chrome_x(),
            y: ui.get_new_project_chrome_y(),
            width: ui.get_new_project_chrome_width(),
            height: ui.get_new_project_chrome_height(),
            radius: ui.get_new_project_chrome_radius(),
        });
    }
    if ui.get_layer_settings_open() {
        punch(BoardRect {
            x: ui.get_layer_settings_chrome_x(),
            y: ui.get_layer_settings_chrome_y(),
            width: ui.get_layer_settings_chrome_width(),
            height: ui.get_layer_settings_chrome_height(),
            radius: ui.get_layer_settings_chrome_radius(),
        });
    }
    if ui.get_project_settings_open() {
        punch(BoardRect {
            x: ui.get_project_settings_chrome_x(),
            y: ui.get_project_settings_chrome_y(),
            width: ui.get_project_settings_chrome_width(),
            height: ui.get_project_settings_chrome_height(),
            radius: ui.get_project_settings_chrome_radius(),
        });
    }
    if ui.get_tip_visible() {
        punch(BoardRect {
            x: ui.get_tip_chrome_x(),
            y: ui.get_tip_chrome_y(),
            width: ui.get_tip_chrome_width(),
            height: ui.get_tip_chrome_height(),
            radius: ui.get_tip_chrome_radius(),
        });
    }
    holes
}

pub fn sync_board_geometry(
    ui: &AppWindow,
    host: &Rc<RefCell<BoardHost>>,
    snapshot: &BoardGeoSnapshot,
) {
    let holes = collect_board_holes(ui, &snapshot.layout);
    ui.window().with_winit_window(|winit_window| {
        host.borrow_mut().sync_geometry(
            winit_window,
            &snapshot.layout,
            snapshot.content_height,
            snapshot.scale,
            snapshot.overlay,
            &holes,
        );
    });
}
