use super::file_drop::DropHandler;
use crate::board::ModifierState;
use i_slint_backend_winit::{CustomApplicationHandler, EventResult};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

#[derive(Clone, Default)]
pub struct FrameSignal(Rc<Cell<bool>>);

impl FrameSignal {
    pub fn mark(&self) {
        self.0.set(true);
    }

    pub fn take(&self) -> bool {
        self.0.replace(false)
    }
}

pub struct ShellEvents {
    pub drops: DropHandler,
    pub frame_changed: FrameSignal,
    pub modifiers: Rc<RefCell<ModifierState>>,
}

impl CustomApplicationHandler for ShellEvents {
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        winit_window: Option<&Window>,
        slint_window: Option<&slint::Window>,
        event: &WindowEvent,
    ) -> EventResult {
        if matches!(event, WindowEvent::Resized(_) | WindowEvent::Moved(_)) {
            self.frame_changed.mark();
        }
        if let WindowEvent::ModifiersChanged(modifiers) = event {
            let state = modifiers.state();
            let mut mods = self.modifiers.borrow_mut();
            mods.meta_held = state.super_key() || state.control_key();
            mods.alt_held = state.alt_key();
            mods.shift_held = state.shift_key();
        }
        self.drops
            .window_event(event_loop, window_id, winit_window, slint_window, event)
    }
}
