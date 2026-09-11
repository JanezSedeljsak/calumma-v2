use i_slint_backend_winit::{CustomApplicationHandler, EventResult};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

#[derive(Clone, Default)]
pub struct DropQueue(Rc<RefCell<Vec<PathBuf>>>);

impl DropQueue {
    pub fn take(&self) -> Vec<PathBuf> {
        std::mem::take(&mut self.0.borrow_mut())
    }

    fn push(&self, path: PathBuf) {
        self.0.borrow_mut().push(path);
    }
}

pub struct DropHandler(pub DropQueue);

impl CustomApplicationHandler for DropHandler {
    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _winit_window: Option<&Window>,
        _slint_window: Option<&slint::Window>,
        event: &WindowEvent,
    ) -> EventResult {
        if let WindowEvent::DroppedFile(path) = event {
            self.0.push(path.clone());
        }
        EventResult::Propagate
    }
}
