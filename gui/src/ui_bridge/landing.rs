use super::{sync_shell, AppWindow, RecentRow};
use crate::shell::{relative_time, AppController};
use calumma_app::{Engine, ProjectSummary};
use slint::{Color, ModelRc, SharedString, VecModel};

const RECENTS_SHOWN: usize = 5;

pub fn sync_recents(ui: &AppWindow, controller: &AppController) {
    let engine = controller.engine.borrow();
    let rows: Vec<RecentRow> = controller
        .refresh_recents()
        .iter()
        .take(RECENTS_SHOWN)
        .map(|item| RecentRow {
            id: SharedString::from(item.id.as_str()),
            name: SharedString::from(item.name.as_str()),
            size_text: SharedString::from(format!("{} × {}", item.width, item.height)),
            time_text: SharedString::from(relative_time(item.opened_at, &controller.l10n)),
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
    slint::Image::default()
}

pub fn accent_color(item: &ProjectSummary) -> Color {
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

pub fn form_accent(ui: &AppWindow) -> [u8; 3] {
    calumma_core::project_color(ui.get_project_accent_index().max(0) as usize)
}

pub fn random_accent_index() -> i32 {
    calumma_core::project_color_index(calumma_core::random_project_color()) as i32
}

pub fn refresh_landing(ui: &AppWindow, controller: &AppController) {
    sync_shell(ui, controller);
}
