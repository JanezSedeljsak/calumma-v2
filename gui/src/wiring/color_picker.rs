use super::wake;
use crate::shell::SharedController;
use crate::ui_bridge::{sync_editor, AppWindow, ColorChrome, SharedUi};
use slint::ComponentHandle;

pub fn wire(ui: &AppWindow, controller: SharedController, ui_weak: SharedUi) {
    ui.global::<ColorChrome>().on_select_swatch({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |index| {
            let mut ctrl = controller.borrow_mut();
            ctrl.select_quick_color(index as usize);
            if let Some(ui) = ui_weak.upgrade() {
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<ColorChrome>().on_sb_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |x, y| {
            let mut ctrl = controller.borrow_mut();
            ctrl.set_color_sb(x, 1.0 - y);
            if let Some(ui) = ui_weak.upgrade() {
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<ColorChrome>().on_hue_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |hue| {
            let mut ctrl = controller.borrow_mut();
            ctrl.set_color_hue(hue);
            if let Some(ui) = ui_weak.upgrade() {
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<ColorChrome>().on_hex_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |text| {
            let mut ctrl = controller.borrow_mut();
            let hex = ctrl.commit_color_hex(text.as_ref());
            if let Some(ui) = ui_weak.upgrade() {
                ui.global::<ColorChrome>().set_hex_text(hex.into());
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
}
