use super::{cursor_context, refresh_board_cursor, schedule_toast_hide, wake, InputState};
use crate::board::BoardHost;
use crate::shell::SharedController;
use crate::ui_bridge::{
    sync_editor, sync_guide_readout, sync_guides, sync_layers, sync_shell, sync_zoom_chrome,
    AppWindow, FilterDebounce, GuideChrome, SharedUi, ZoomChrome,
};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

pub fn wire(
    ui: &AppWindow,
    controller: SharedController,
    host: Rc<RefCell<BoardHost>>,
    ui_weak: SharedUi,
    input: Rc<RefCell<InputState>>,
    filter_debounce: Rc<FilterDebounce>,
) {
    ui.global::<ZoomChrome>().on_zoom_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |unit| {
            controller.borrow_mut().set_zoom_unit(unit);
            if let Some(ui) = ui_weak.upgrade() {
                sync_zoom_chrome(&ui, &controller.borrow());
            }
            wake(&ui_weak);
        }
    });

    ui.global::<ZoomChrome>().on_step_zoom({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |zoom_in| {
            controller.borrow_mut().step_zoom(zoom_in);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });

    ui.global::<ZoomChrome>().on_fit_zoom({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().fit_to_view();
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });

    ui.global::<GuideChrome>().on_pressed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let input = input.clone();
        move |horizontal, x, y| {
            let shift = input.borrow().mods.shift_held;
            controller
                .borrow_mut()
                .begin_guide_drag(horizontal, x, y, shift);
            if let Some(ui) = ui_weak.upgrade() {
                sync_guide_readout(&ui, &controller.borrow());
            }
            wake(&ui_weak);
        }
    });
    ui.global::<GuideChrome>().on_moved({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let input = input.clone();
        move |x, y| {
            let shift = input.borrow().mods.shift_held;
            controller.borrow_mut().update_guide_drag(x, y, shift);
            if let Some(ui) = ui_weak.upgrade() {
                sync_guide_readout(&ui, &controller.borrow());
            }
            wake(&ui_weak);
        }
    });
    ui.global::<GuideChrome>().on_released({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().end_guide_drag();
            if let Some(ui) = ui_weak.upgrade() {
                let ctrl = controller.borrow();
                sync_guide_readout(&ui, &ctrl);
                sync_guides(&ui, &ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<GuideChrome>().on_ruler_has_guide({
        let controller = controller.clone();
        move |x, y| {
            controller
                .borrow()
                .engine
                .borrow()
                .guide_axis_at(x, y)
                .is_some()
        }
    });

    ui.on_pointer_pressed({
        let host = host.clone();
        let input = input.clone();
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let filter_debounce = filter_debounce.clone();
        move |x, y, middle, meta, alt, shift| {
            let mods = {
                let mut state = input.borrow_mut();
                state.mods.meta_held = meta;
                state.mods.alt_held = alt;
                state.mods.shift_held = shift;
                state.effective()
            };
            host.borrow_mut().pointer_pressed(x, y, mods, middle);
            refresh_board_cursor(&host, &controller, &input);
            if controller.borrow().editor_open {
                host.borrow_mut().render();
            }
            if let Some(ui) = ui_weak.upgrade() {
                filter_debounce.commit(&controller, &ui);
                let mut ctrl = controller.borrow_mut();
                ctrl.announce_tool_block_if_any();
                ctrl.retarget_layer_settings_to_active();
                sync_layers(&ui, &mut ctrl);
                if ctrl.toast_visible {
                    sync_shell(&ui, &ctrl);
                    schedule_toast_hide(&ui_weak, controller.clone());
                }
            }
            wake(&ui_weak);
        }
    });
    ui.on_pointer_moved({
        let host = host.clone();
        let input = input.clone();
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |x, y| {
            let (mods, modal) = cursor_context(&controller, &input);
            host.borrow_mut().pointer_moved(x, y, mods, modal);
            if controller.borrow().engine.borrow().is_dragging_guide() {
                if let Some(ui) = ui_weak.upgrade() {
                    sync_guide_readout(&ui, &controller.borrow());
                }
            }
        }
    });
    ui.on_pointer_released({
        let host = host.clone();
        let controller = controller.clone();
        let input = input.clone();
        let ui_weak = ui_weak.clone();
        move |x, y| {
            let (mods, modal) = cursor_context(&controller, &input);
            host.borrow_mut().pointer_released(x, y, mods, modal);
            if controller.borrow().editor_open {
                host.borrow_mut().render();
            }
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                ctrl.retarget_layer_settings_to_active();
                sync_layers(&ui, &mut ctrl);
                sync_guide_readout(&ui, &ctrl);
                sync_guides(&ui, &ctrl);
            }
            let host = host.clone();
            let controller = controller.clone();
            let input = input.clone();
            slint::Timer::single_shot(Duration::ZERO, move || {
                refresh_board_cursor(&host, &controller, &input);
            });
            wake(&ui_weak);
        }
    });
    ui.on_pointer_enter({
        let host = host.clone();
        let controller = controller.clone();
        let input = input.clone();
        move || {
            host.borrow_mut().set_pointer_inside(true);
            refresh_board_cursor(&host, &controller, &input);
        }
    });
    ui.on_pointer_exit({
        let host = host.clone();
        let controller = controller.clone();
        let input = input.clone();
        move || {
            host.borrow_mut().set_pointer_inside(false);
            refresh_board_cursor(&host, &controller, &input);
        }
    });
    ui.on_scrolled({
        let host = host.clone();
        let controller = controller.clone();
        let input = input.clone();
        move |x, y, dx, dy, alt, meta| {
            if controller.borrow().pinch_zoom.is_some() {
                return;
            }
            let mods = {
                let mut state = input.borrow_mut();
                state.mods.alt_held = alt;
                state.mods.meta_held = meta;
                state.effective()
            };
            host.borrow_mut()
                .scroll(x, y, dx, dy, mods.alt_held, mods.meta_held);
        }
    });
    ui.on_pinch_started({
        let controller = controller.clone();
        move || {
            controller.borrow_mut().pinch_started();
        }
    });
    ui.on_pinch_updated({
        let controller = controller.clone();
        move |x, y, scale| {
            controller.borrow_mut().pinch_updated(x, y, scale);
        }
    });
    ui.on_pinch_ended({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().pinch_ended();
            if let Some(ui) = ui_weak.upgrade() {
                sync_zoom_chrome(&ui, &controller.borrow());
            }
            wake(&ui_weak);
        }
    });
}
