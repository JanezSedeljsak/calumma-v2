use super::{defer_sync_editor, defer_sync_editor_only, refresh_board_cursor, wake, InputState};
use crate::board::BoardHost;
use crate::shell::SharedController;
use crate::ui_bridge::{
    sync_editor, sync_layers, sync_smart_tools, AppWindow, LayerListChrome, SharedUi, ToolChrome,
};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;

pub fn wire(
    ui: &AppWindow,
    controller: SharedController,
    host: Rc<RefCell<BoardHost>>,
    ui_weak: SharedUi,
    input: Rc<RefCell<InputState>>,
) {
    ui.global::<ToolChrome>().on_pick_tool({
        let controller = controller.clone();
        let host = host.clone();
        let input = input.clone();
        let ui_weak = ui_weak.clone();
        move |tool| {
            if let Some(tool) = calumma_core::Tool::from_u32(tool as u32) {
                controller.borrow_mut().pick_tool(tool);
                defer_sync_editor(&ui_weak, &controller, &host, &input);
            }
        }
    });

    ui.global::<ToolChrome>().on_brush_size_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |unit| {
            let mut ctrl = controller.borrow_mut();
            ctrl.set_brush_size_unit(unit);
            if let Some(ui) = ui_weak.upgrade() {
                sync_editor(&ui, &mut ctrl);
            }
        }
    });

    ui.global::<ToolChrome>().on_brush_size_committed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |text| {
            let mut ctrl = controller.borrow_mut();
            ctrl.commit_brush_size(text.as_ref());
            if let Some(ui) = ui_weak.upgrade() {
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });

    ui.global::<ToolChrome>().on_ink_opacity_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |opacity| {
            let mut ctrl = controller.borrow_mut();
            ctrl.set_ink_opacity(opacity);
            if let Some(ui) = ui_weak.upgrade() {
                sync_editor(&ui, &mut ctrl);
            }
        }
    });

    ui.global::<ToolChrome>().on_pick_brush({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |id| {
            if let Some(brush) = calumma_core::Brush::from_u32(id as u32) {
                controller.borrow_mut().engine.borrow_mut().set_brush(brush);
                defer_sync_editor_only(&ui_weak, &controller);
            }
        }
    });
    ui.global::<ToolChrome>().on_blur_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |value| {
            controller
                .borrow_mut()
                .engine
                .borrow_mut()
                .set_blur_strength(value);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.global::<ToolChrome>().on_hardness_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |value| {
            controller
                .borrow_mut()
                .engine
                .borrow_mut()
                .set_eraser_hardness(value);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.global::<ToolChrome>().on_tolerance_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |value| {
            controller
                .borrow_mut()
                .engine
                .borrow_mut()
                .set_tolerance(value);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.global::<ToolChrome>().on_eyedropper_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |value| {
            controller
                .borrow_mut()
                .engine
                .borrow_mut()
                .set_eyedropper_radius(value);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.global::<ToolChrome>().on_fill_toggled({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let next = !controller.borrow().engine.borrow().shape_fill();
            controller
                .borrow_mut()
                .engine
                .borrow_mut()
                .set_shape_fill(next);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.global::<ToolChrome>().on_stroke_toggled({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let next = !controller.borrow().engine.borrow().shape_stroke();
            controller
                .borrow_mut()
                .engine
                .borrow_mut()
                .set_shape_stroke(next);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.global::<ToolChrome>().on_vector_toggled({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let next = !controller.borrow().engine.borrow().vector_mode();
            controller
                .borrow_mut()
                .engine
                .borrow_mut()
                .set_vector_mode(next);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.global::<ToolChrome>().on_aligned_toggled({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let next = !controller.borrow().engine.borrow().clone_aligned();
            controller
                .borrow_mut()
                .engine
                .borrow_mut()
                .set_clone_aligned(next);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.global::<ToolChrome>().on_transform_toggled({
        let controller = controller.clone();
        let host = host.clone();
        let input = input.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let next = !controller.borrow().engine.borrow().transform_active();
            controller
                .borrow_mut()
                .engine
                .borrow_mut()
                .set_move_transform(next);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
                drop(ctrl);
                refresh_board_cursor(&host, &controller, &input);
            }
        }
    });
    ui.global::<ToolChrome>().on_crop_aspect_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |index| {
            controller.borrow_mut().set_crop_aspect(index);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<ToolChrome>().on_crop_overlay_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |index| {
            controller.borrow_mut().set_crop_overlay(index);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<ToolChrome>().on_commit_crop({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().commit_crop();
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<ToolChrome>().on_cancel_crop({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().cancel_crop();
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<ToolChrome>().on_remove_background({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let Some(index) = controller.borrow().engine.borrow().active_layer_index() else {
                return;
            };
            let started = controller
                .borrow_mut()
                .engine
                .borrow_mut()
                .remove_background(index);
            if !started {
                return;
            }
            if let Some(ui) = ui_weak.upgrade() {
                sync_smart_tools(&ui, &controller.borrow());
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerListChrome>().on_commit_bounds({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |x, y, w, h| {
            controller.borrow_mut().commit_layer_bounds(
                x.as_ref(),
                y.as_ref(),
                w.as_ref(),
                h.as_ref(),
            );
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<LayerListChrome>().on_commit_canvas_size({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |w, h| {
            controller
                .borrow_mut()
                .commit_canvas_size(w.as_ref(), h.as_ref());
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
}
