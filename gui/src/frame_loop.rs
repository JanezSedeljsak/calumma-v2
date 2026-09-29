use crate::board::BoardHost;
use crate::board_geometry::{board_geo, sync_board_geometry, BoardGeo};
use crate::input::{DropQueue, ImeQueue};
use crate::shell::{self, SharedController};
use crate::ui_bridge::{
    camera_signature, sync_layer_rows, sync_rulers, sync_zoom_chrome, SharedUi,
};
use crate::wiring::deliver_images;
use crate::wiring::ime::{self, ImeSync};
use crate::{app_icon, window_chrome};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

pub fn start(
    ui_weak: SharedUi,
    host: Rc<RefCell<BoardHost>>,
    controller: SharedController,
    drops: DropQueue,
    ime_queue: ImeQueue,
) -> Vec<slint::Timer> {
    let ime_sync = ImeSync::default();
    let icon_done = Rc::new(Cell::new(false));
    let board = slint::Timer::default();
    let camera = Rc::new(RefCell::new((f32::NAN, f32::NAN, f32::NAN)));
    let last_geo = Rc::new(Cell::new(None::<BoardGeo>));
    let last_present = Rc::new(Cell::new(Instant::now() - Duration::from_secs(1)));
    board.start(slint::TimerMode::Repeated, Duration::from_millis(8), {
        let ui_weak = ui_weak.clone();
        let host = host.clone();
        let controller = controller.clone();
        let icon_done = icon_done.clone();
        let last_present = last_present.clone();
        let last_geo = last_geo.clone();
        move || {
            let dropped = drops.take();
            if !dropped.is_empty() {
                let images = shell::read_image_files(&dropped);
                deliver_images(images, &controller, &ui_weak, &host);
            }
            if !icon_done.get() {
                if let Some(ui) = ui_weak.upgrade() {
                    app_icon::set_window_icon(&ui);
                    window_chrome::apply(&ui, controller.borrow().prefs.is_dark());
                    icon_done.set(true);
                }
            }
            if !controller.borrow().editor_open {
                ime_queue.set_board_owns(false);
                last_geo.set(None);
                return;
            }
            let hint = controller.borrow().engine.borrow().frame_hint();
            let period = if hint == 0 {
                Duration::from_millis(8)
            } else {
                Duration::from_millis(1000 / u64::from(hint.max(1)))
            };
            let now = Instant::now();
            let present = now.duration_since(last_present.get()) >= period;
            if present {
                last_present.set(now);
            }
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            ime::tick(&ui, &ui_weak, &controller, &ime_queue, &ime_sync);
            let snapshot = board_geo(&ui);
            let geo_changed = last_geo.get() != Some(snapshot.key);
            if present || geo_changed {
                sync_board_geometry(&ui, &host, &snapshot);
                last_geo.set(Some(snapshot.key));
            }
            if present && !snapshot.overlay {
                host.borrow_mut().render();
            }
            if !present {
                return;
            }
            host.borrow_mut().reconcile_cursor();
            let ctrl = controller.borrow();
            let next = camera_signature(&ctrl);
            if !ui.get_loading() && (*camera.borrow() != next || geo_changed) {
                *camera.borrow_mut() = next;
                sync_rulers(&ui, &ctrl);
                sync_zoom_chrome(&ui, &ctrl);
            }
        }
    });

    let stats = slint::Timer::default();
    stats.start(slint::TimerMode::Repeated, Duration::from_millis(500), {
        let ui_weak = ui_weak.clone();
        let controller = controller.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                if !controller.borrow().editor_open {
                    return;
                }
                let mut ctrl = controller.borrow_mut();
                ui.set_memory_value(
                    shell::format_bytes(ctrl.engine.borrow().resident_memory_bytes(), &ctrl.l10n)
                        .into(),
                );
                sync_layer_rows(&ui, &mut ctrl);
            }
        }
    });

    vec![board, stats]
}
