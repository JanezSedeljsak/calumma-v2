use super::{sync_layers, AppWindow};
use crate::shell::SharedController;
use std::cell::RefCell;
use std::rc::Rc;

struct Pending {
    filter: Option<(usize, i32, f32)>,
    opacity: Option<(usize, f32)>,
}

/// Stages layer opacity/filter edits while a slider is being dragged and
/// pushes them to the engine in one shot on release (or on any external
/// event that needs the latest value, such as switching layers).
pub struct FilterDebounce {
    pending: RefCell<Pending>,
}

impl FilterDebounce {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            pending: RefCell::new(Pending {
                filter: None,
                opacity: None,
            }),
        })
    }

    pub fn stage_filter(&self, index: usize, kind: i32, value: f32) {
        self.pending.borrow_mut().filter = Some((index, kind, value));
    }

    pub fn stage_opacity(&self, index: usize, value: f32) {
        self.pending.borrow_mut().opacity = Some((index, value));
    }

    pub fn commit(&self, controller: &SharedController, ui: &AppWindow) {
        let pending = self.pending.replace(Pending {
            filter: None,
            opacity: None,
        });
        let mut ctrl = controller.borrow_mut();
        ctrl.layer_settings_dragging = false;
        let mut changed = false;
        if let Some((index, kind, value)) = pending.filter {
            ctrl.set_layer_filter(index, kind, value);
            changed = true;
        }
        if let Some((index, value)) = pending.opacity {
            ctrl.set_layer_opacity(index, value);
            changed = true;
        }
        if changed {
            sync_layers(ui, &mut ctrl);
        }
    }

    pub fn cancel_filter(&self) {
        self.pending.borrow_mut().filter = None;
    }
}
