use super::{schedule_toast_hide, wake};
use crate::board::BoardHost;
use crate::board_geometry::setup_board;
use crate::shell::SharedController;
use crate::ui_bridge::{
    form_accent, parse_dimension, random_accent_index, set_editor_open, sync_editor, sync_guides,
    sync_project_tabs, sync_shell, AppWindow, GuideChrome, ProjectChrome, SettingsChrome, SharedUi,
    DEFAULT_HEIGHT, DEFAULT_WIDTH,
};
use crate::window_chrome;
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;

pub fn wire(
    ui: &AppWindow,
    controller: SharedController,
    host: Rc<RefCell<BoardHost>>,
    ui_weak: SharedUi,
) {
    ui.global::<SettingsChrome>().on_dismissed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().settings_open = false;
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &controller.borrow());
            }
        }
    });

    ui.global::<SettingsChrome>().on_set_theme_light({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let _ = ctrl.set_theme_dark(false);
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
                if ctrl.editor_open {
                    sync_editor(&ui, &mut ctrl);
                }
                window_chrome::apply_appearance(&ui, false);
            }
            wake(&ui_weak);
        }
    });

    ui.global::<SettingsChrome>().on_set_theme_dark({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let _ = ctrl.set_theme_dark(true);
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
                if ctrl.editor_open {
                    sync_editor(&ui, &mut ctrl);
                }
                window_chrome::apply_appearance(&ui, true);
            }
            wake(&ui_weak);
        }
    });

    ui.global::<SettingsChrome>().on_set_language_en({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let _ = ctrl.set_language("en");
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
                if ctrl.editor_open {
                    sync_editor(&ui, &mut ctrl);
                }
            }
            wake(&ui_weak);
        }
    });

    ui.global::<ProjectChrome>().on_new_project_dismissed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().new_project_open = false;
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &controller.borrow());
            }
        }
    });

    ui.global::<GuideChrome>().on_open_card({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().open_guides();
            if let Some(ui) = ui_weak.upgrade() {
                let ctrl = controller.borrow();
                if ui.global::<GuideChrome>().get_add_offset().is_empty() {
                    ui.global::<GuideChrome>()
                        .set_add_offset(slint::SharedString::from("0"));
                }
                sync_shell(&ui, &ctrl);
                sync_guides(&ui, &ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<GuideChrome>().on_dismissed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().guides_open = false;
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &controller.borrow());
            }
        }
    });
    ui.global::<GuideChrome>().on_add_guide({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                let horizontal = ui.global::<GuideChrome>().get_add_horizontal();
                let offset = ui.global::<GuideChrome>().get_add_offset().to_string();
                controller
                    .borrow_mut()
                    .add_guide_from_card(horizontal, &offset);
                let ctrl = controller.borrow();
                sync_guides(&ui, &ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<GuideChrome>().on_remove_guide({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |index| {
            controller.borrow_mut().remove_guide(index as usize);
            if let Some(ui) = ui_weak.upgrade() {
                sync_guides(&ui, &controller.borrow());
            }
            wake(&ui_weak);
        }
    });
    ui.global::<GuideChrome>().on_clear_guides({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().clear_guides();
            if let Some(ui) = ui_weak.upgrade() {
                sync_guides(&ui, &controller.borrow());
            }
            wake(&ui_weak);
        }
    });
    ui.global::<GuideChrome>().on_set_axis({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |index, horizontal| {
            controller
                .borrow_mut()
                .set_guide_axis(index as usize, horizontal);
            if let Some(ui) = ui_weak.upgrade() {
                sync_guides(&ui, &controller.borrow());
            }
            wake(&ui_weak);
        }
    });
    ui.global::<GuideChrome>().on_set_offset({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |index, text| {
            controller
                .borrow_mut()
                .set_guide_offset(index as usize, text.as_str());
            if let Some(ui) = ui_weak.upgrade() {
                sync_guides(&ui, &controller.borrow());
            }
            wake(&ui_weak);
        }
    });
    ui.global::<GuideChrome>().on_set_color({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |index, palette_index| {
            controller
                .borrow_mut()
                .set_guide_color(index as usize, palette_index as usize);
            if let Some(ui) = ui_weak.upgrade() {
                sync_guides(&ui, &controller.borrow());
            }
            wake(&ui_weak);
        }
    });

    ui.global::<ProjectChrome>().on_create_from_modal({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let host = host.clone();
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let name = ui.global::<ProjectChrome>().get_name().to_string();
            let width = parse_dimension(
                ui.global::<ProjectChrome>().get_width_text().as_ref(),
                DEFAULT_WIDTH,
            );
            let height = parse_dimension(
                ui.global::<ProjectChrome>().get_height_text().as_ref(),
                DEFAULT_HEIGHT,
            );
            let accent = form_accent(&ui);
            let mut ctrl = controller.borrow_mut();
            ctrl.new_project_open = false;
            if ctrl
                .create_project(&name, width, height, Some(accent))
                .is_ok()
            {
                ui.global::<ProjectChrome>()
                    .set_accent_index(random_accent_index());
                set_editor_open(&ui, &mut ctrl, true);
                host.borrow_mut().set_active(true);
                setup_board(&ui_weak, &host);
                ctrl.show_toast_key("projectCreated", false);
            }
            sync_shell(&ui, &ctrl);
            schedule_toast_hide(&ui_weak, controller.clone());
        }
    });

    ui.on_toast_dismissed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().dismiss_toast();
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &controller.borrow());
            }
        }
    });

    ui.global::<ProjectChrome>().on_settings_dismissed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().project_settings_open = false;
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &controller.borrow());
            }
        }
    });

    ui.global::<ProjectChrome>().on_rename({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |name| {
            let mut ctrl = controller.borrow_mut();
            if ctrl.rename_open_project_settings(name.as_str()).is_err() {
                return;
            }
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
                sync_project_tabs(&ui, &ctrl);
            }
        }
    });

    ui.global::<ProjectChrome>().on_recolor({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |index| {
            let mut ctrl = controller.borrow_mut();
            if ctrl.recolor_open_project_settings(index as usize).is_err() {
                return;
            }
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
                sync_project_tabs(&ui, &ctrl);
            }
            wake(&ui_weak);
        }
    });
}
