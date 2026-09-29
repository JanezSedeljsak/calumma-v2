use super::wake;
use crate::shell::SharedController;
use crate::ui_bridge::{sync_editor, AppWindow, SharedUi, ToolChrome};
use slint::ComponentHandle;

pub fn wire(ui: &AppWindow, controller: SharedController, ui_weak: SharedUi) {
    ui.global::<ToolChrome>().on_text_family_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |family| {
            controller
                .borrow_mut()
                .engine
                .borrow_mut()
                .set_text_family(family.as_str());
            if let Some(ui) = ui_weak.upgrade() {
                ui.global::<ToolChrome>()
                    .set_text_font_query(slint::SharedString::from(""));
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.global::<ToolChrome>().on_text_size_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |unit| {
            controller
                .borrow_mut()
                .engine
                .borrow_mut()
                .set_text_size_unit(unit);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.global::<ToolChrome>().on_text_size_committed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |text| {
            if let Ok(size) = text.trim().parse::<f32>() {
                controller
                    .borrow_mut()
                    .engine
                    .borrow_mut()
                    .set_text_size(size);
            }
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<ToolChrome>().on_text_line_height_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |value| {
            controller
                .borrow_mut()
                .engine
                .borrow_mut()
                .set_text_line_height(value);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.global::<ToolChrome>().on_text_wrap_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |width| {
            controller
                .borrow_mut()
                .engine
                .borrow_mut()
                .set_text_wrap_width(width);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.global::<ToolChrome>().on_text_bold_toggled({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let next = !controller.borrow().engine.borrow().text_bold();
            controller
                .borrow_mut()
                .engine
                .borrow_mut()
                .set_text_bold(next);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.global::<ToolChrome>().on_text_italic_toggled({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let next = !controller.borrow().engine.borrow().text_italic();
            controller
                .borrow_mut()
                .engine
                .borrow_mut()
                .set_text_italic(next);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.global::<ToolChrome>().on_text_align_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |align| {
            if let Some(align) = calumma_core::TextAlign::from_u32(align as u32) {
                controller
                    .borrow_mut()
                    .engine
                    .borrow_mut()
                    .set_text_align(align);
            }
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.global::<ToolChrome>().on_text_font_search({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |_| {
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
}
