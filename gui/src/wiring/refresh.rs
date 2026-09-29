use crate::board::BoardHost;
use crate::board::ModifierState;
use crate::shell::SharedController;
use crate::ui_bridge::{sync_editor, sync_shell, SharedUi};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

pub struct InputState {
    pub mods: ModifierState,
    pub window: Rc<RefCell<ModifierState>>,
}

impl InputState {
    pub fn effective(&self) -> ModifierState {
        let mut mods = self.mods;
        let held = *self.window.borrow();
        mods.meta_held |= held.meta_held;
        mods.alt_held |= held.alt_held;
        mods.shift_held |= held.shift_held;
        mods
    }

    pub fn merge_shell(&mut self, control: bool, meta: bool, shift: bool, alt: bool) {
        self.mods.meta_held = meta || control;
        self.mods.shift_held = shift;
        self.mods.alt_held = alt;
    }
}

pub fn cursor_context(
    controller: &SharedController,
    input: &Rc<RefCell<InputState>>,
) -> (ModifierState, bool) {
    let ctrl = controller.borrow();
    (input.borrow().effective(), ctrl.any_modal_open())
}

pub fn refresh_board_cursor(
    host: &Rc<RefCell<BoardHost>>,
    controller: &SharedController,
    input: &Rc<RefCell<InputState>>,
) {
    let (mods, modal) = cursor_context(controller, input);
    host.borrow_mut().refresh_cursor(modal, mods);
}

pub fn defer_sync_editor(
    ui_weak: &SharedUi,
    controller: &SharedController,
    host: &Rc<RefCell<BoardHost>>,
    input: &Rc<RefCell<InputState>>,
) {
    let ui_weak = ui_weak.clone();
    let controller = controller.clone();
    let host = host.clone();
    let input = input.clone();
    slint::Timer::single_shot(Duration::ZERO, move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut ctrl = controller.borrow_mut();
            sync_editor(&ui, &mut ctrl);
            drop(ctrl);
            refresh_board_cursor(&host, &controller, &input);
            wake(&ui_weak);
        }
    });
}

pub fn defer_sync_editor_only(ui_weak: &SharedUi, controller: &SharedController) {
    let ui_weak = ui_weak.clone();
    let controller = controller.clone();
    slint::Timer::single_shot(Duration::ZERO, move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut ctrl = controller.borrow_mut();
            sync_editor(&ui, &mut ctrl);
            wake(&ui_weak);
        }
    });
}

pub fn schedule_toast_hide(ui_weak: &SharedUi, controller: SharedController) {
    slint::Timer::single_shot(Duration::from_secs(3), {
        let ui_weak = ui_weak.clone();
        let controller = controller.clone();
        move || {
            controller.borrow_mut().dismiss_toast();
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &controller.borrow());
            }
        }
    });
}

pub fn wake(ui_weak: &SharedUi) {
    if let Some(ui) = ui_weak.upgrade() {
        ui.window().request_redraw();
    }
}
