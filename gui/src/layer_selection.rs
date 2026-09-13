use calumma_core::{AlignEdge, DistributeAxis};
use slint::ComponentHandle;

use crate::shell::SharedController;
use crate::ui_bridge::{sync_layers, sync_shell, AppWindow, LayerChrome, SharedUi};

pub fn wire(ui: &AppWindow, controller: SharedController, ui_weak: SharedUi) {
    let chrome = ui.global::<LayerChrome>();
    chrome.on_toggle_layer_selected({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |index| {
            controller
                .borrow_mut()
                .toggle_layer_selected(index as usize);
            refresh(&controller, &ui_weak);
        }
    });
    chrome.on_align({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |edge| {
            let edge = match edge {
                0 => AlignEdge::Left,
                1 => AlignEdge::CenterH,
                2 => AlignEdge::Right,
                3 => AlignEdge::Top,
                4 => AlignEdge::CenterV,
                _ => AlignEdge::Bottom,
            };
            controller
                .borrow()
                .engine
                .borrow_mut()
                .align_selected_layers(edge);
            refresh(&controller, &ui_weak);
        }
    });
    chrome.on_distribute(move |horizontal| {
        let axis = if horizontal {
            DistributeAxis::Horizontal
        } else {
            DistributeAxis::Vertical
        };
        controller
            .borrow()
            .engine
            .borrow_mut()
            .distribute_selected_layers(axis);
        refresh(&controller, &ui_weak);
    });
}

fn refresh(controller: &SharedController, ui_weak: &SharedUi) {
    let Some(ui) = ui_weak.upgrade() else { return };
    let mut ctrl = controller.borrow_mut();
    sync_shell(&ui, &ctrl);
    sync_layers(&ui, &mut ctrl);
    drop(ctrl);
    ui.window().request_redraw();
}
