use super::{sync_layers, AppWindow};
use crate::shell::SharedController;
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

const SETTLE: Duration = Duration::from_millis(400);

struct Pending {
    filter: Option<(usize, i32, f32)>,
    opacity: Option<(usize, f32)>,
}

pub struct FilterDebounce {
    timer: slint::Timer,
    pending: RefCell<Pending>,
}

impl FilterDebounce {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            timer: slint::Timer::default(),
            pending: RefCell::new(Pending {
                filter: None,
                opacity: None,
            }),
        })
    }

    pub fn schedule_filter(
        self: &Rc<Self>,
        index: usize,
        kind: i32,
        value: f32,
        controller: &SharedController,
        ui_weak: &slint::Weak<AppWindow>,
    ) {
        self.pending.borrow_mut().filter = Some((index, kind, value));
        self.arm(controller, ui_weak);
    }

    pub fn schedule_opacity(
        self: &Rc<Self>,
        index: usize,
        value: f32,
        controller: &SharedController,
        ui_weak: &slint::Weak<AppWindow>,
    ) {
        self.pending.borrow_mut().opacity = Some((index, value));
        self.arm(controller, ui_weak);
    }

    pub fn commit(&self, controller: &SharedController, ui: &AppWindow) {
        self.timer.stop();
        let pending = self.pending.replace(Pending {
            filter: None,
            opacity: None,
        });
        let mut ctrl = controller.borrow_mut();
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
        if self.pending.borrow().opacity.is_none() {
            self.timer.stop();
        }
    }

    fn arm(self: &Rc<Self>, controller: &SharedController, ui_weak: &slint::Weak<AppWindow>) {
        let this = Rc::clone(self);
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        self.timer
            .start(slint::TimerMode::SingleShot, SETTLE, move || {
                if let Some(ui) = ui_weak.upgrade() {
                    this.commit(&controller, &ui);
                    ui.window().request_redraw();
                }
            });
    }
}
