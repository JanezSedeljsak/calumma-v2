use super::{AppWindow, GuideRow};
use crate::shell::{slint_color, AppController};
use calumma_core::guide::GuideAxis;
use calumma_core::PROJECT_COLORS;
use slint::{ModelRc, SharedString, VecModel};

fn format_offset(position: f32) -> String {
    if (position - position.round()).abs() < 0.05 {
        format!("{}", position.round() as i32)
    } else {
        format!("{:.1}", position)
    }
}

pub fn sync_guides(ui: &AppWindow, controller: &AppController) {
    let engine = controller.engine.borrow();
    let rows: Vec<GuideRow> = engine
        .guides()
        .iter()
        .enumerate()
        .map(|(index, guide)| GuideRow {
            index: index as i32,
            horizontal: guide.axis == GuideAxis::Horizontal,
            offset_text: SharedString::from(format_offset(guide.position)),
            color: slint_color([guide.color[0], guide.color[1], guide.color[2], 255]),
            palette_index: calumma_core::project_color_index(guide.color) as i32,
        })
        .collect();
    let count = rows.len();
    let limit = engine.guides_limit();
    drop(engine);
    ui.set_guides(ModelRc::new(VecModel::from(rows)));
    ui.set_can_add_guide(count < limit);
}

pub fn sync_guide_palette(ui: &AppWindow) {
    let colors: Vec<slint::Color> = PROJECT_COLORS
        .iter()
        .map(|rgb| slint_color([rgb[0], rgb[1], rgb[2], 255]))
        .collect();
    ui.set_guide_palette(ModelRc::new(VecModel::from(colors)));
}

pub fn sync_guide_readout(ui: &AppWindow, controller: &AppController) {
    match controller.engine.borrow().dragged_guide_readout() {
        Some((horizontal, position, screen)) => {
            ui.set_guide_readout_visible(true);
            ui.set_guide_readout_horizontal(horizontal);
            ui.set_guide_readout_text(SharedString::from(format_offset(position)));
            ui.set_guide_readout_screen(screen);
        }
        None => ui.set_guide_readout_visible(false),
    }
}
