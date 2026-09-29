use super::{deferred_load_project, deliver_images, handle_tab_close_result};
use crate::board::BoardHost;
use crate::board_geometry::setup_board;
use crate::shell::SharedController;
use crate::shell::{self, pick_artwork_files};
use crate::ui_bridge::{
    form_accent, parse_dimension, random_accent_index, refresh_landing, set_editor_open,
    sync_project_tabs, sync_shell, AppWindow, SharedUi, DEFAULT_HEIGHT, DEFAULT_WIDTH,
};
use std::cell::RefCell;
use std::rc::Rc;

pub fn wire(
    ui: &AppWindow,
    controller: SharedController,
    host: Rc<RefCell<BoardHost>>,
    ui_weak: SharedUi,
) {
    ui.on_create_project({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let host = host.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let name = ui.get_project_name().to_string();
            let width = parse_dimension(ui.get_width_text().as_ref(), DEFAULT_WIDTH);
            let height = parse_dimension(ui.get_height_text().as_ref(), DEFAULT_HEIGHT);
            let accent = form_accent(&ui);
            let mut ctrl = controller.borrow_mut();
            if ctrl
                .create_project(&name, width, height, Some(accent))
                .is_ok()
            {
                ui.set_project_accent_index(random_accent_index());
                set_editor_open(&ui, &mut ctrl, true);
                host.borrow_mut().set_active(true);
                setup_board(&ui_weak, &host);
            }
            sync_shell(&ui, &ctrl);
        }
    });

    ui.on_preset_size({
        let ui_weak = ui_weak.clone();
        move |width, height| {
            let Some(ui) = ui_weak.upgrade() else { return };
            ui.set_width_text(width.to_string().into());
            ui.set_height_text(height.to_string().into());
        }
    });

    ui.on_open_recent({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let host = host.clone();
        move |id| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let id = id.to_string();
            let summary = {
                let mut ctrl = controller.borrow_mut();
                ctrl.prepare_switch_to(&id)
            };
            if let Some(summary) = summary {
                deferred_load_project(summary, controller.clone(), ui_weak.clone(), host.clone());
            } else {
                let ctrl = controller.borrow();
                sync_project_tabs(&ui, &ctrl);
            }
        }
    });

    ui.on_delete_recent({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let host = host.clone();
        move |id| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let id = id.to_string();
            let mut ctrl = controller.borrow_mut();
            let untitled = ctrl.l10n.get("untitled");
            let name = ctrl
                .refresh_recents()
                .into_iter()
                .find(|project| project.id == id)
                .map(|project| project.name)
                .filter(|name| !name.is_empty())
                .unwrap_or(untitled);
            let title = ctrl.l10n.get("deleteProject");
            let message = ctrl.l10n.format("deleteProjectMessage", &[&name]);
            let ok = ctrl.l10n.get("deleteProject");
            let cancel = ctrl.l10n.get("cancel");
            let Some(result) = ctrl.delete_project_confirmed(&id, &title, &message, &ok, &cancel)
            else {
                sync_shell(&ui, &ctrl);
                return;
            };
            drop(ctrl);
            handle_tab_close_result(
                result,
                controller.clone(),
                ui_weak.clone(),
                host.clone(),
                &ui,
            );
        }
    });

    ui.on_clear_recents({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let host = host.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let mut ctrl = controller.borrow_mut();
            let title = ctrl.l10n.get("clearAllRecentsTitle");
            let message = ctrl.l10n.get("clearAllRecentsMessage");
            let ok = ctrl.l10n.get("clearAllRecents");
            let cancel = ctrl.l10n.get("cancel");
            let cleared = ctrl
                .clear_recents_confirmed(&title, &message, &ok, &cancel)
                .unwrap_or(false);
            if cleared {
                host.borrow_mut().set_active(false);
                ui.set_editor_open(false);
                ui.set_loading(false);
                refresh_landing(&ui, &ctrl);
            } else {
                sync_shell(&ui, &ctrl);
            }
        }
    });

    ui.on_app_icon_clicked(shell::play_meow);

    ui.on_paste_artwork_clicked({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let host = host.clone();
        move || {
            let filter = controller.borrow().l10n.get("imagesFilter");
            let images = pick_artwork_files(&filter);
            if images.is_empty() {
                return;
            }
            deliver_images(images, &controller, &ui_weak, &host);
        }
    });
}
