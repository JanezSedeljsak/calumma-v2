use super::{schedule_toast_hide, wake};
use crate::shell::SharedController;
use crate::ui_bridge::{
    apply_filter_readout, apply_opacity_readout, sync_layer_settings, sync_layers, sync_shell,
    AppWindow, FilterDebounce, LayerChrome, LayerListChrome, SharedUi,
};
use slint::ComponentHandle;
use std::rc::Rc;

pub fn wire(
    ui: &AppWindow,
    controller: SharedController,
    ui_weak: SharedUi,
    filter_debounce: Rc<FilterDebounce>,
) {
    ui.global::<LayerListChrome>().on_toggle_visible({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |index| {
            controller.borrow_mut().toggle_layer_visible(index as usize);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerListChrome>().on_open_settings({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let filter_debounce = filter_debounce.clone();
        move |index, anchor_x, anchor_y| {
            if let Some(ui) = ui_weak.upgrade() {
                filter_debounce.commit(&controller, &ui);
            }
            controller
                .borrow_mut()
                .open_layer_settings(index as usize, anchor_x, anchor_y);
            if let Some(ui) = ui_weak.upgrade() {
                let ctrl = controller.borrow();
                sync_shell(&ui, &ctrl);
                ui.set_layer_settings_open(true);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerListChrome>().on_delete_layer({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |index| {
            controller.borrow_mut().remove_layer(index as usize);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerListChrome>().on_rename_layer({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |index, name| {
            controller.borrow_mut().rename_layer(index as usize, &name);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_layer_settings(&ui, &ctrl);
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerListChrome>().on_reorder_layer({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |from_row, to_row| {
            controller
                .borrow_mut()
                .move_layer_row(from_row as usize, to_row as usize);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerListChrome>().on_hover_layer({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |index| {
            controller.borrow_mut().set_layer_hover(index as usize);
            if let Some(ui) = ui_weak.upgrade() {
                let ctrl = controller.borrow();
                sync_layer_settings(&ui, &ctrl);
            }
        }
    });
    ui.global::<LayerListChrome>().on_clear_hover({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().clear_layer_hover();
            if let Some(ui) = ui_weak.upgrade() {
                let ctrl = controller.borrow();
                sync_layer_settings(&ui, &ctrl);
            }
        }
    });
    ui.global::<LayerChrome>().on_settings_dismissed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let filter_debounce = filter_debounce.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                filter_debounce.commit(&controller, &ui);
            }
            let mut ctrl = controller.borrow_mut();
            ctrl.layer_settings_open = false;
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerChrome>().on_toggle_visibility({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let index = ctrl.layer_settings_index;
            let visible = ctrl
                .layer_settings_summary()
                .map(|layer| layer.visible)
                .unwrap_or(true);
            ctrl.set_layer_visible(index, !visible);
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerChrome>().on_toggle_lock({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let index = ctrl.layer_settings_index;
            let locked = ctrl
                .layer_settings_summary()
                .map(|layer| layer.locked)
                .unwrap_or(false);
            ctrl.set_layer_locked(index, !locked);
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerChrome>().on_opacity_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let filter_debounce = filter_debounce.clone();
        move |opacity| {
            let index = controller.borrow().layer_settings_index;
            controller.borrow_mut().layer_settings_dragging = true;
            if let Some(ui) = ui_weak.upgrade() {
                apply_opacity_readout(&ui, opacity);
            }
            filter_debounce.stage_opacity(index, opacity);
        }
    });
    ui.global::<LayerChrome>().on_opacity_committed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let filter_debounce = filter_debounce.clone();
        move |opacity| {
            let index = controller.borrow().layer_settings_index;
            filter_debounce.stage_opacity(index, opacity);
            if let Some(ui) = ui_weak.upgrade() {
                filter_debounce.commit(&controller, &ui);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerChrome>().on_export_layer({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let index = controller.borrow().layer_settings_index;
            let mut ctrl = controller.borrow_mut();
            let result = ctrl.try_save_layer_export(index);
            ctrl.notify_layer_export(result);
            ctrl.layer_settings_open = false;
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
            }
            if ctrl.toast_visible {
                schedule_toast_hide(&ui_weak, controller.clone());
            }
        }
    });
    ui.global::<LayerChrome>().on_duplicate_layer({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let index = controller.borrow().layer_settings_index;
            controller.borrow_mut().duplicate_layer(index);
            controller.borrow_mut().layer_settings_open = false;
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_shell(&ui, &ctrl);
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerChrome>().on_delete_layer({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let index = controller.borrow().layer_settings_index;
            controller.borrow_mut().remove_layer(index);
            controller.borrow_mut().layer_settings_open = false;
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_shell(&ui, &ctrl);
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerChrome>().on_blend_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |mode| {
            let mut ctrl = controller.borrow_mut();
            let index = ctrl.layer_settings_index;
            ctrl.set_layer_blend_mode(index, mode);
            if let Some(ui) = ui_weak.upgrade() {
                sync_layer_settings(&ui, &ctrl);
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerChrome>().on_filter_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let filter_debounce = filter_debounce.clone();
        move |kind, value| {
            let index = controller.borrow().layer_settings_index;
            controller.borrow_mut().layer_settings_dragging = true;
            if let Some(ui) = ui_weak.upgrade() {
                apply_filter_readout(&ui, kind, value);
            }
            filter_debounce.stage_filter(index, kind, value);
        }
    });
    ui.global::<LayerChrome>().on_filter_committed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let filter_debounce = filter_debounce.clone();
        move |kind, value| {
            let index = controller.borrow().layer_settings_index;
            filter_debounce.stage_filter(index, kind, value);
            if let Some(ui) = ui_weak.upgrade() {
                filter_debounce.commit(&controller, &ui);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerChrome>().on_reset_filters({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let filter_debounce = filter_debounce.clone();
        move || {
            filter_debounce.cancel_filter();
            let mut ctrl = controller.borrow_mut();
            let index = ctrl.layer_settings_index;
            ctrl.layer_settings_dragging = false;
            ctrl.reset_layer_filters(index);
            if let Some(ui) = ui_weak.upgrade() {
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerChrome>().on_rename_layer({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |name| {
            let mut ctrl = controller.borrow_mut();
            let index = ctrl.layer_settings_index;
            ctrl.rename_layer(index, &name);
            if let Some(ui) = ui_weak.upgrade() {
                sync_layer_settings(&ui, &ctrl);
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerChrome>().on_toggle_clip({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let index = ctrl.layer_settings_index;
            ctrl.toggle_layer_clip(index);
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerChrome>().on_toggle_mask({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let index = ctrl.layer_settings_index;
            ctrl.toggle_layer_mask(index);
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerChrome>().on_flatten_clip({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let index = ctrl.layer_settings_index;
            if ctrl.flatten_layer_clip(index) {
                ctrl.layer_settings_open = false;
            }
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerChrome>().on_merge_down({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let index = ctrl.layer_settings_index;
            if ctrl.merge_layer_down(index) {
                ctrl.layer_settings_open = false;
            }
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerChrome>().on_reset_transform({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let index = ctrl.layer_settings_index;
            ctrl.reset_layer_transform(index);
            if let Some(ui) = ui_weak.upgrade() {
                sync_layer_settings(&ui, &ctrl);
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerChrome>().on_move_up({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let index = ctrl.layer_settings_index;
            ctrl.move_layer_up(index);
            if let Some(ui) = ui_weak.upgrade() {
                sync_layer_settings(&ui, &ctrl);
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerChrome>().on_move_down({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let index = ctrl.layer_settings_index;
            ctrl.move_layer_down(index);
            if let Some(ui) = ui_weak.upgrade() {
                sync_layer_settings(&ui, &ctrl);
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerChrome>().on_rasterize_layer({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let index = ctrl.layer_settings_index;
            ctrl.rasterize_layer(index);
            if let Some(ui) = ui_weak.upgrade() {
                sync_layer_settings(&ui, &ctrl);
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
}
