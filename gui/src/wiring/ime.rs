use super::wake;
use crate::input::{ImeEvent, ImeQueue};
use crate::shell::SharedController;
use crate::ui_bridge::{sync_editor, sync_layers, AppWindow, SharedUi};
use i_slint_backend_winit::WinitWindowAccessor;
use slint::ComponentHandle;
use std::cell::Cell;

#[derive(Default)]
pub struct ImeSync {
    allowed: Cell<Option<bool>>,
    area: Cell<Option<[i32; 4]>>,
}

pub fn tick(
    ui: &AppWindow,
    ui_weak: &SharedUi,
    controller: &SharedController,
    ime: &ImeQueue,
    sync: &ImeSync,
) {
    let (editing, caret) = {
        let ctrl = controller.borrow();
        let engine = ctrl.engine.borrow();
        (
            ctrl.editor_open && !ctrl.any_modal_open() && engine.text_editing(),
            engine.text_caret_screen_rect(),
        )
    };
    let board_focused = ui.get_shell_keys_focused();
    ime.set_board_owns(editing && board_focused);
    if !board_focused {
        sync.allowed.set(None);
        sync.area.set(None);
    } else if sync.allowed.get() != Some(editing) {
        ui.window()
            .with_winit_window(|window| window.set_ime_allowed(editing));
        sync.allowed.set(Some(editing));
        sync.area.set(None);
    }
    if let (true, Some([x, y, width, height])) = (ime.board_owns(), caret) {
        let (x, y) = (ui.get_board_x() + x, ui.get_board_y() + y);
        let key = [x, y, width, height].map(|v| v.round() as i32);
        if sync.area.get() != Some(key) {
            ui.window().with_winit_window(|window| {
                window.set_ime_cursor_area(
                    winit::dpi::LogicalPosition::new(x, y),
                    winit::dpi::LogicalSize::new(width, height),
                );
            });
            sync.area.set(Some(key));
        }
    }
    deliver(ui, ui_weak, controller, ime.take());
}

fn deliver(
    ui: &AppWindow,
    ui_weak: &SharedUi,
    controller: &SharedController,
    events: Vec<ImeEvent>,
) {
    if events.is_empty() {
        return;
    }
    let engine = controller.borrow().engine.clone();
    for event in events {
        let mut engine = engine.borrow_mut();
        match event {
            ImeEvent::Preedit(text, cursor) => engine.text_set_composition(&text, cursor),
            ImeEvent::Commit(text) => engine.text_insert(&text),
        }
    }
    let mut ctrl = controller.borrow_mut();
    sync_editor(ui, &mut ctrl);
    sync_layers(ui, &mut ctrl);
    drop(ctrl);
    wake(ui_weak);
}
