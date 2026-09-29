use super::{paste_from_clipboard, refresh_board_cursor, schedule_toast_hide, wake, InputState};
use crate::board::BoardHost;
use crate::input::{
    apply_text_key, handle_key_press_for_modifiers, handle_key_release, handle_shell_key,
    handle_text_key, EditorKeyAction, KeyPressModifierAction, KeyReleaseAction, Modifiers,
    ShellKeyAction,
};
use crate::shell::SharedController;
use crate::ui_bridge::{
    sync_editor, sync_guide_readout, sync_layers, sync_shell, AppWindow, SharedUi,
};
use std::cell::RefCell;
use std::rc::Rc;

fn route_text_key(
    text: &str,
    mods: Modifiers,
    controller: &SharedController,
    ui_weak: &SharedUi,
) -> bool {
    let engine = {
        let ctrl = controller.borrow();
        if !ctrl.editor_open || ctrl.any_modal_open() {
            return false;
        }
        ctrl.engine.clone()
    };
    if !engine.borrow().text_editing() {
        return false;
    }
    if !apply_text_key(&mut engine.borrow_mut(), handle_text_key(text, mods)) {
        return false;
    }
    if let Some(ui) = ui_weak.upgrade() {
        let mut ctrl = controller.borrow_mut();
        sync_editor(&ui, &mut ctrl);
        sync_layers(&ui, &mut ctrl);
    }
    wake(ui_weak);
    true
}

pub fn wire(
    ui: &AppWindow,
    controller: SharedController,
    ui_weak: SharedUi,
    input: Rc<RefCell<InputState>>,
    host: Rc<RefCell<BoardHost>>,
) {
    ui.on_shell_key_pressed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let input = input.clone();
        let host = host.clone();
        move |text, control, meta, shift, alt| {
            let mods = Modifiers {
                control,
                meta,
                shift,
                alt,
            };
            if route_text_key(&text, mods, &controller, &ui_weak) {
                refresh_board_cursor(&host, &controller, &input);
                return;
            }
            {
                let mut state = input.borrow_mut();
                match handle_key_press_for_modifiers(&text, mods) {
                    KeyPressModifierAction::Space(held) => state.mods.space_held = held,
                    KeyPressModifierAction::Alt(held) => state.mods.alt_held = held,
                    KeyPressModifierAction::None => {}
                }
                state.merge_shell(control, meta, shift, alt);
            }
            host.borrow_mut().modifiers_changed(
                input.borrow().effective(),
                controller.borrow().any_modal_open(),
            );
            controller
                .borrow_mut()
                .refresh_guide_shift(input.borrow().effective().shift_held);
            if let Some(ui) = ui_weak.upgrade() {
                sync_guide_readout(&ui, &controller.borrow());
            }

            let ctrl = controller.borrow();
            let action = handle_shell_key(
                &text,
                mods,
                ctrl.any_modal_open(),
                ctrl.editor_open,
                ctrl.can_undo(),
                ctrl.can_redo(),
            );
            drop(ctrl);

            match action {
                ShellKeyAction::None => {}
                ShellKeyAction::Return => {
                    let engine = controller.borrow().engine.clone();
                    if !engine.borrow().text_editing() {
                        if engine.borrow().active_tool() == Some(calumma_core::Tool::Crop) {
                            controller.borrow_mut().commit_crop();
                        } else if engine.borrow().transform_active() {
                            engine.borrow_mut().set_move_transform(false);
                            refresh_board_cursor(&host, &controller, &input);
                        }
                    }
                }
                ShellKeyAction::Escape => {
                    let mut ctrl = controller.borrow_mut();
                    if ctrl.toast_visible {
                        ctrl.dismiss_toast();
                    } else if ctrl.any_modal_open() {
                        ctrl.dismiss_modals();
                    } else if ctrl.engine.borrow().active_tool() == Some(calumma_core::Tool::Crop) {
                        ctrl.cancel_crop();
                    }
                }
                ShellKeyAction::OpenSettings => {
                    controller.borrow_mut().open_settings();
                }
                ShellKeyAction::Paste => {
                    paste_from_clipboard(&controller, &ui_weak, &host);
                }
                ShellKeyAction::NewProject => {
                    controller.borrow_mut().new_project_open = true;
                }
                ShellKeyAction::ToggleLayers => {
                    let mut ctrl = controller.borrow_mut();
                    let _ = ctrl.toggle_layers_panel();
                }
                ShellKeyAction::ClipLayer => {
                    if let Some(index) = controller.borrow().engine.borrow().active_layer_index() {
                        controller.borrow_mut().toggle_layer_clip(index);
                    }
                }
                ShellKeyAction::Editor(editor_action) => match editor_action {
                    EditorKeyAction::None => {}
                    EditorKeyAction::Undo => controller.borrow_mut().undo(),
                    EditorKeyAction::Redo => controller.borrow_mut().redo(),
                    EditorKeyAction::Save => {
                        controller.borrow_mut().save();
                        controller.borrow_mut().show_toast_key("saved", false);
                        schedule_toast_hide(&ui_weak, controller.clone());
                    }
                    EditorKeyAction::ToggleTransform => {
                        controller
                            .borrow_mut()
                            .engine
                            .borrow_mut()
                            .set_move_transform(true);
                        refresh_board_cursor(&host, &controller, &input);
                    }
                    EditorKeyAction::ZoomIn => {
                        controller.borrow_mut().step_zoom(true);
                    }
                    EditorKeyAction::ZoomOut => {
                        controller.borrow_mut().step_zoom(false);
                    }
                    EditorKeyAction::FitZoom => {
                        controller.borrow_mut().fit_to_view();
                    }
                    EditorKeyAction::PickTool(tool) => {
                        controller.borrow_mut().pick_tool(tool);
                    }
                },
            }

            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_shell(&ui, &ctrl);
                if ctrl.editor_open {
                    sync_editor(&ui, &mut ctrl);
                    sync_layers(&ui, &mut ctrl);
                    ui.set_layers_open(ctrl.prefs.layers_panel_open);
                }
            }
            wake(&ui_weak);
        }
    });

    ui.on_shell_key_released({
        let input = input.clone();
        let host = host.clone();
        let controller = controller.clone();
        move |text, control, meta, shift, alt| {
            let mods = Modifiers {
                control,
                meta,
                shift,
                alt,
            };
            {
                let mut state = input.borrow_mut();
                match handle_key_release(&text, mods) {
                    KeyReleaseAction::Space(held) => state.mods.space_held = held,
                    KeyReleaseAction::Alt(held) => state.mods.alt_held = held,
                    KeyReleaseAction::Meta(held) => state.mods.meta_held = held,
                    KeyReleaseAction::Shift(held) => state.mods.shift_held = held,
                    KeyReleaseAction::None => {}
                }
                state.merge_shell(control, meta, shift, alt);
            }
            host.borrow_mut().modifiers_changed(
                input.borrow().effective(),
                controller.borrow().any_modal_open(),
            );
            controller
                .borrow_mut()
                .refresh_guide_shift(input.borrow().effective().shift_held);
            if let Some(ui) = ui_weak.upgrade() {
                sync_guide_readout(&ui, &controller.borrow());
            }
        }
    });
}
