use crate::board::BoardHost;
use crate::board_geometry::setup_board;
use crate::shell::SharedController;
use crate::shell::TabCloseResult;
use crate::ui_bridge::{
    refresh_landing, set_editor_open, sync_project_tabs, sync_ruler_preview, sync_shell, AppWindow,
    ProjectChrome, SharedUi,
};
use calumma_core::limits::SKELETON_MIN_HOLD_MS;
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

pub fn wire(
    ui: &AppWindow,
    controller: SharedController,
    host: Rc<RefCell<BoardHost>>,
    ui_weak: SharedUi,
) {
    ui.global::<ProjectChrome>().on_switch_tab({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let host = host.clone();
        move |id| {
            let id = id.to_string();
            let summary = {
                let mut ctrl = controller.borrow_mut();
                ctrl.bump_load_generation();
                ctrl.prepare_switch_to(&id)
            };
            if let Some(summary) = summary {
                deferred_load_project(summary, controller.clone(), ui_weak.clone(), host.clone());
            }
        }
    });

    ui.global::<ProjectChrome>().on_close_tab({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let host = host.clone();
        move |id| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let id = id.to_string();
            let result = controller.borrow_mut().close_project_tab(&id);
            handle_tab_close_result(
                result,
                controller.clone(),
                ui_weak.clone(),
                host.clone(),
                &ui,
            );
        }
    });

    ui.global::<ProjectChrome>().on_edit_tab({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |id, anchor_x, anchor_y| {
            let id = id.to_string();
            let mut ctrl = controller.borrow_mut();
            ctrl.open_project_settings(&id, anchor_x, anchor_y);
            let name = ctrl
                .engine
                .borrow()
                .project_summary(&id)
                .map(|summary| summary.name)
                .unwrap_or_default();
            if let Some(ui) = ui_weak.upgrade() {
                ui.global::<ProjectChrome>()
                    .set_settings_name(slint::SharedString::from(name));
                sync_shell(&ui, &ctrl);
            }
        }
    });
}

pub fn deferred_load_project(
    summary: calumma_app::ProjectSummary,
    controller: SharedController,
    ui_weak: SharedUi,
    host: Rc<RefCell<BoardHost>>,
) {
    let id = summary.id.clone();
    let Some(ui) = ui_weak.upgrade() else { return };
    ui.set_editor_open(true);
    ui.set_loading(true);
    let preview = calumma_app::Engine::fit_preview(
        ui.get_board_width(),
        ui.get_board_height(),
        summary.width,
        summary.height,
    );
    let [paper_x, paper_y, paper_width, paper_height] =
        preview.paper_rect(summary.width as f32, summary.height as f32);
    sync_ruler_preview(&ui, &preview);
    let shown_at = Instant::now();
    ui.set_loading_paper_x(paper_x);
    ui.set_loading_paper_y(paper_y);
    ui.set_loading_paper_width(paper_width);
    ui.set_loading_paper_height(paper_height);
    host.borrow_mut().set_active(true);
    setup_board(&ui_weak, &host);
    sync_project_tabs(&ui, &controller.borrow());

    let gen = controller.borrow_mut().bump_load_generation();
    let controller = controller.clone();
    let ui_weak = ui_weak.clone();
    let host = host.clone();
    slint::Timer::single_shot(Duration::ZERO, move || {
        let Some(ui) = ui_weak.upgrade() else {
            return;
        };
        let mut ctrl = controller.borrow_mut();
        if ctrl.load_generation != gen {
            return;
        }
        if ctrl.load_project(&id).is_ok() {
            set_editor_open(&ui, &mut ctrl, true);
        } else {
            ui.set_editor_open(false);
            host.borrow_mut().set_active(false);
        }
        sync_shell(&ui, &ctrl);
        drop(ctrl);

        let hold = Duration::from_millis(SKELETON_MIN_HOLD_MS).saturating_sub(shown_at.elapsed());
        let ui_weak = ui_weak.clone();
        slint::Timer::single_shot(hold, move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_loading(false);
            }
        });
    });
}

pub fn handle_tab_close_result(
    result: TabCloseResult,
    controller: SharedController,
    ui_weak: SharedUi,
    host: Rc<RefCell<BoardHost>>,
    ui: &AppWindow,
) {
    match result {
        TabCloseResult::Unchanged => {
            let ctrl = controller.borrow();
            sync_project_tabs(ui, &ctrl);
            sync_shell(ui, &ctrl);
        }
        TabCloseResult::SwitchTo(next) => {
            let summary = controller.borrow_mut().prepare_switch_to(&next);
            if let Some(summary) = summary {
                deferred_load_project(summary, controller, ui_weak, host);
            }
        }
        TabCloseResult::ShowLanding => {
            host.borrow_mut().set_active(false);
            ui.set_editor_open(false);
            ui.set_loading(false);
            refresh_landing(ui, &controller.borrow());
        }
    }
}
