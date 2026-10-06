use super::wake;
use crate::shell::SharedController;
use crate::ui_bridge::{
    sync_editor, sync_layers, sync_shell, AppWindow, FilterDebounce, LayerListChrome, MenuChrome,
    SharedUi,
};
use crate::window_chrome;
use i_slint_backend_winit::WinitWindowAccessor;
use slint::ComponentHandle;
use std::rc::Rc;

pub fn wire(
    ui: &AppWindow,
    controller: SharedController,
    ui_weak: SharedUi,
    filter_debounce: Rc<FilterDebounce>,
) {
    let toggle_layers = {
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let _ = ctrl.toggle_layers_panel();
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_layers_open(ctrl.prefs.layers_panel_open);
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    };

    ui.global::<MenuChrome>()
        .on_toggle_layers(toggle_layers.clone());
    ui.global::<LayerListChrome>().on_add_layer({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().add_layer();
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerListChrome>().on_pick_layer({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let filter_debounce = filter_debounce.clone();
        move |index| {
            if let Some(ui) = ui_weak.upgrade() {
                filter_debounce.commit(&controller, &ui);
            }
            controller.borrow_mut().pick_layer(index as usize);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<MenuChrome>().on_fit_view({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().fit_to_view();
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<MenuChrome>().on_new_project({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().new_project_open = true;
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &controller.borrow());
            }
        }
    });
    ui.global::<MenuChrome>().on_settings({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            ctrl.open_settings();
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<MenuChrome>().on_undo({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().undo();
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_shell(&ui, &ctrl);
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<MenuChrome>().on_redo({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().redo();
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_shell(&ui, &ctrl);
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<MenuChrome>().on_fullscreen({
        let ui_weak = ui_weak.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.window().with_winit_window(|window| {
                    if window.fullscreen().is_some() {
                        window.set_fullscreen(None);
                    } else {
                        window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
                    }
                });
            }
        }
    });
    ui.on_titlebar_drag({
        let ui_weak = ui_weak.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                window_chrome::drag(&ui);
            }
        }
    });
    ui.on_titlebar_zoom({
        let ui_weak = ui_weak.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                window_chrome::zoom(&ui);
            }
        }
    });
}
