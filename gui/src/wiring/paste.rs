use super::{schedule_toast_hide, wake};
use crate::board::BoardHost;
use crate::board_geometry::setup_board;
use crate::shell::SharedController;
use crate::shell::{self, ClipboardContent, NamedImage};
use crate::ui_bridge::{set_editor_open, sync_editor, sync_layers, sync_shell, SharedUi};
use std::cell::RefCell;
use std::rc::Rc;

pub fn deliver_images(
    images: Vec<NamedImage>,
    controller: &SharedController,
    ui_weak: &SharedUi,
    host: &Rc<RefCell<BoardHost>>,
) {
    let Some(ui) = ui_weak.upgrade() else {
        return;
    };
    let mut ctrl = controller.borrow_mut();
    if images.is_empty() {
        ctrl.show_toast_key("artworkImportFailed", true);
    } else if ctrl.editor_open {
        ctrl.paste_images(&images);
        sync_editor(&ui, &mut ctrl);
        sync_layers(&ui, &mut ctrl);
    } else if ctrl.import_artworks(&images).is_ok() {
        set_editor_open(&ui, &mut ctrl, true);
        host.borrow_mut().set_active(true);
        setup_board(ui_weak, host);
    } else {
        ctrl.show_toast_key("artworkImportFailed", true);
    }
    sync_shell(&ui, &ctrl);
    if ctrl.toast_visible {
        schedule_toast_hide(ui_weak, controller.clone());
    }
    drop(ctrl);
    wake(ui_weak);
}

pub fn paste_from_clipboard(
    controller: &SharedController,
    ui_weak: &SharedUi,
    host: &Rc<RefCell<BoardHost>>,
) {
    match shell::read_clipboard() {
        ClipboardContent::Images(images) => deliver_images(images, controller, ui_weak, host),
        ClipboardContent::Text(text) if controller.borrow().engine.borrow().text_editing() => {
            let engine = controller.borrow().engine.clone();
            engine.borrow_mut().text_insert(&text);
            wake(ui_weak);
        }
        ClipboardContent::Text(_) | ClipboardContent::Empty => {
            let mut ctrl = controller.borrow_mut();
            ctrl.show_toast_key("clipboardNoImage", true);
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
            }
            schedule_toast_hide(ui_weak, controller.clone());
        }
    }
}
