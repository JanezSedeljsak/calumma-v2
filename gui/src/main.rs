mod board;
mod input;
mod shell;
mod ui_bridge;

use board::{board_layout, BoardHost, ModifierState};
use calumma_io::RasterFormat;
use i_slint_backend_winit::WinitWindowAccessor;
use input::{
    handle_key_press_for_modifiers, handle_key_release, handle_shell_key, EditorKeyAction,
    KeyPressModifierAction, KeyReleaseAction, Modifiers, ShellKeyAction,
};
use shell::{pick_artwork_file, shared, workspace_root, SharedController, Theme};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use ui_bridge::{
    init_form_defaults, load_app_icon, parse_dimension, refresh_landing, set_editor_open,
    sync_editor, sync_layer_settings, sync_layers, sync_shell, AppWindow, SharedUi, DEFAULT_HEIGHT,
    DEFAULT_WIDTH,
};

struct InputState {
    mods: ModifierState,
}

impl InputState {
    fn merge_shell(&mut self, control: bool, meta: bool, shift: bool, alt: bool) {
        self.mods.meta_held = meta || control;
        self.mods.shift_held = shift;
        self.mods.alt_held = alt;
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .select()
        .map_err(|e| format!("selecting the winit backend: {e}"))?;

    let root = workspace_root();
    let window_metrics = Theme::window_metrics(&root)?;
    let icons_root = root.join("design/icons");
    let controller = shared(root.clone())?;
    let host = Rc::new(RefCell::new(BoardHost::new(
        controller.borrow().engine.clone(),
        icons_root,
    )));

    let ui = AppWindow::new()?;
    set_window_icon(&ui, &root);
    load_app_icon(&ui, &root);
    {
        ui.window().set_size(slint::LogicalSize::new(
            window_metrics.width as f32,
            window_metrics.height as f32,
        ));
    }
    let ui_weak = ui.as_weak();

    {
        let mut ctrl = controller.borrow_mut();
        refresh_landing(&ui, &ctrl);
        init_form_defaults(&ui, &ctrl.l10n);
        ui.set_layers_open(ctrl.prefs.layers_panel_open);
        if !ctrl.try_restore_last() {
            ctrl.editor_open = false;
        }
        ui.set_editor_open(ctrl.editor_open);
        host.borrow_mut().set_active(ctrl.editor_open);
        if ctrl.editor_open {
            sync_editor(&ui, &mut ctrl);
        }
    }

    wire_landing_callbacks(&ui, controller.clone(), host.clone(), ui_weak.clone());
    let input = Rc::new(RefCell::new(InputState {
        mods: ModifierState::default(),
    }));
    wire_editor_callbacks(
        &ui,
        controller.clone(),
        host.clone(),
        ui_weak.clone(),
        input.clone(),
    );
    wire_modals(&ui, controller.clone(), host.clone(), ui_weak.clone());
    wire_menus(&ui, controller.clone(), ui_weak.clone());
    wire_exports(&ui, controller.clone(), ui_weak.clone());
    wire_layer_actions(&ui, controller.clone(), ui_weak.clone());
    wire_tools(&ui, controller.clone(), ui_weak.clone());
    wire_color_picker(&ui, controller.clone(), ui_weak.clone());
    wire_shell_keys(
        &ui,
        controller.clone(),
        ui_weak.clone(),
        input,
        host.clone(),
    );

    let _frame_timers = start_frame_loop(ui_weak, host, controller);

    ui.run()?;
    Ok(())
}

fn wire_landing_callbacks(
    ui: &AppWindow,
    controller: SharedController,
    host: Rc<RefCell<BoardHost>>,
    ui_weak: SharedUi,
) {
    ui.on_create_project({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let host = host.clone();
        move || {
            let ui = ui_weak.upgrade().unwrap();
            let name = ui.get_project_name().to_string();
            let width = parse_dimension(&ui.get_width_text().to_string(), DEFAULT_WIDTH);
            let height = parse_dimension(&ui.get_height_text().to_string(), DEFAULT_HEIGHT);
            let mut ctrl = controller.borrow_mut();
            if ctrl.create_project(&name, width, height).is_ok() {
                set_editor_open(&ui, &mut ctrl, true);
                host.borrow_mut().set_active(true);
                setup_board(&ui_weak, &host);
            }
            sync_shell(&ui, &ctrl);
        }
    });

    ui.on_preset_size({
        let ui_weak = ui_weak.clone();
        move |width, height| {
            let ui = ui_weak.upgrade().unwrap();
            ui.set_width_text(width.to_string().into());
            ui.set_height_text(height.to_string().into());
        }
    });

    ui.on_open_recent({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let host = host.clone();
        move |id| {
            let ui = ui_weak.upgrade().unwrap();
            let id = id.to_string();
            let mut ctrl = controller.borrow_mut();
            if ctrl.open_project(&id).is_ok() {
                set_editor_open(&ui, &mut ctrl, true);
                host.borrow_mut().set_active(true);
                setup_board(&ui_weak, &host);
            }
        }
    });

    ui.on_delete_recent({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let host = host.clone();
        move |id| {
            let ui = ui_weak.upgrade().unwrap();
            let id = id.to_string();
            let mut ctrl = controller.borrow_mut();
            let _ = ctrl.delete_project(&id);
            host.borrow_mut().set_active(ctrl.editor_open);
            ui.set_editor_open(ctrl.editor_open);
            sync_shell(&ui, &ctrl);
        }
    });

    ui.on_clear_recents({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let host = host.clone();
        move || {
            let ui = ui_weak.upgrade().unwrap();
            let mut ctrl = controller.borrow_mut();
            let _ = ctrl.clear_recents();
            host.borrow_mut().set_active(false);
            ui.set_editor_open(false);
            sync_shell(&ui, &ctrl);
        }
    });

    ui.on_paste_artwork_clicked({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let host = host.clone();
        move || {
            let Some(bytes) = pick_artwork_file() else {
                return;
            };
            let ui = ui_weak.upgrade().unwrap();
            let mut ctrl = controller.borrow_mut();
            if ctrl.import_artwork(&bytes).is_ok() {
                set_editor_open(&ui, &mut ctrl, true);
                host.borrow_mut().set_active(true);
                setup_board(&ui_weak, &host);
            } else {
                ctrl.show_toast_key("artworkImportFailed", true);
            }
            sync_shell(&ui, &ctrl);
            if ctrl.toast_visible {
                schedule_toast_hide(&ui_weak, controller.clone());
            }
        }
    });
}

fn wire_editor_callbacks(
    ui: &AppWindow,
    controller: SharedController,
    host: Rc<RefCell<BoardHost>>,
    ui_weak: SharedUi,
    input: Rc<RefCell<InputState>>,
) {
    ui.on_back_to_landing({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let host = host.clone();
        move || {
            let ui = ui_weak.upgrade().unwrap();
            let mut ctrl = controller.borrow_mut();
            ctrl.close_editor();
            host.borrow_mut().set_active(false);
            ui.set_editor_open(false);
            refresh_landing(&ui, &ctrl);
        }
    });

    ui.on_pick_tool({
        let controller = controller.clone();
        let host = host.clone();
        let input = input.clone();
        let ui_weak = ui_weak.clone();
        move |tool| {
            if let Some(tool) = calumma_core::Tool::from_u32(tool as u32) {
                let mut ctrl = controller.borrow_mut();
                ctrl.pick_tool(tool);
                if let Some(ui) = ui_weak.upgrade() {
                    let mut ctrl = controller.borrow_mut();
                    sync_editor(&ui, &mut ctrl);
                    refresh_board_cursor(&host, &controller, &input);
                }
            }
        }
    });

    ui.on_brush_size_changed({
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

    ui.on_brush_size_committed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |text| {
            let mut ctrl = controller.borrow_mut();
            ctrl.commit_brush_size(&text.to_string());
            if let Some(ui) = ui_weak.upgrade() {
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });

    ui.on_ink_opacity_changed({
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

    ui.on_zoom_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |unit| {
            controller.borrow_mut().set_zoom_unit(unit);
            wake(&ui_weak);
        }
    });

    ui.on_step_zoom({
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

    ui.on_fit_zoom({
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

    ui.on_pointer_pressed({
        let host = host.clone();
        let input = input.clone();
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |x, y, middle| {
            let mods = input.borrow().mods;
            host.borrow_mut().pointer_pressed(x, y, mods, middle);
            refresh_board_cursor(&host, &controller, &input);
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
            wake(&ui_weak);
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
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_layers(&ui, &mut ctrl);
            }
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
        let input = input.clone();
        let ui_weak = ui_weak.clone();
        move |x, y, delta| {
            let alt = input.borrow().mods.alt_held;
            host.borrow_mut().scroll(x, y, delta, false, alt);
            wake(&ui_weak);
        }
    });
}

fn cursor_context(
    controller: &SharedController,
    input: &Rc<RefCell<InputState>>,
) -> (ModifierState, bool) {
    let ctrl = controller.borrow();
    (input.borrow().mods, ctrl.any_modal_open())
}

fn refresh_board_cursor(
    host: &Rc<RefCell<BoardHost>>,
    controller: &SharedController,
    input: &Rc<RefCell<InputState>>,
) {
    let (mods, modal) = cursor_context(controller, input);
    host.borrow_mut().refresh_cursor(modal, mods);
}

fn wire_menus(ui: &AppWindow, controller: SharedController, ui_weak: SharedUi) {
    let toggle_layers = {
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let _ = ctrl.toggle_layers_panel();
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_layers_open(ctrl.prefs.layers_panel_open);
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    };

    ui.on_toggle_layers(toggle_layers.clone());
    ui.on_add_layer({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().add_layer();
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.on_pick_layer({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |index| {
            controller.borrow_mut().pick_layer(index as usize);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.on_menu_fit_view({
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
    ui.on_menu_new_project({
        let controller = controller.clone();
        move || {
            controller.borrow_mut().new_project_open = true;
        }
    });
    ui.on_menu_settings({
        let controller = controller.clone();
        move || {
            controller.borrow_mut().settings_open = true;
        }
    });
    ui.on_menu_undo({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().undo();
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_shell(&ui, &ctrl);
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.on_menu_redo({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().redo();
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_shell(&ui, &ctrl);
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.on_menu_fullscreen({
        let ui_weak = ui_weak.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.window().with_winit_window(|window| {
                    if window.fullscreen().is_some() {
                        window.set_fullscreen(None);
                    } else {
                        window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
                    }
                });
            }
        }
    });
}

fn wire_exports(ui: &AppWindow, controller: SharedController, ui_weak: SharedUi) {
    let export_raster = |format: RasterFormat, ext: &'static str| {
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let result = ctrl.try_save_composite(format, ext);
            ctrl.notify_export(result);
            if ctrl.toast_visible {
                if let Some(ui) = ui_weak.upgrade() {
                    sync_shell(&ui, &ctrl);
                }
                schedule_toast_hide(&ui_weak, controller.clone());
            }
        }
    };

    ui.on_export_png(export_raster(RasterFormat::Png, "png"));
    ui.on_export_jpeg(export_raster(RasterFormat::Jpeg, "jpg"));
    ui.on_export_webp(export_raster(RasterFormat::Webp, "webp"));
    ui.on_export_avif(export_raster(RasterFormat::Avif, "avif"));
    ui.on_export_heic(export_raster(RasterFormat::Heic, "heic"));

    ui.on_export_psd({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let result = ctrl.try_save_psd();
            ctrl.notify_export(result);
            if ctrl.toast_visible {
                if let Some(ui) = ui_weak.upgrade() {
                    sync_shell(&ui, &ctrl);
                }
                schedule_toast_hide(&ui_weak, controller.clone());
            }
        }
    });
    ui.on_export_svg({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let result = ctrl.try_save_svg();
            ctrl.notify_export(result);
            if ctrl.toast_visible {
                if let Some(ui) = ui_weak.upgrade() {
                    sync_shell(&ui, &ctrl);
                }
                schedule_toast_hide(&ui_weak, controller.clone());
            }
        }
    });
    ui.on_export_pdf({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let result = ctrl.try_save_pdf();
            ctrl.notify_export(result);
            if ctrl.toast_visible {
                if let Some(ui) = ui_weak.upgrade() {
                    sync_shell(&ui, &ctrl);
                }
                schedule_toast_hide(&ui_weak, controller.clone());
            }
        }
    });
}

fn wire_layer_actions(ui: &AppWindow, controller: SharedController, ui_weak: SharedUi) {
    ui.on_toggle_layer_visible({
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
    ui.on_open_layer_settings({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |index| {
            controller.borrow_mut().open_layer_settings(index as usize);
            if let Some(ui) = ui_weak.upgrade() {
                let ctrl = controller.borrow();
                sync_shell(&ui, &ctrl);
                ui.set_layer_settings_open(true);
            }
            wake(&ui_weak);
        }
    });
    ui.on_delete_layer({
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
    ui.on_hover_layer({
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
    ui.on_clear_layer_hover({
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
    ui.on_layer_settings_dismissed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().layer_settings_open = false;
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &controller.borrow());
            }
        }
    });
    ui.on_layer_settings_toggle_visibility({
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
    ui.on_layer_settings_export_layer({
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
    ui.on_layer_settings_duplicate({
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
    ui.on_layer_settings_delete({
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
}

fn wire_tools(ui: &AppWindow, controller: SharedController, ui_weak: SharedUi) {
    ui.on_toggle_tools_menu({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().toggle_tools_menu();
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.on_run_upscale({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let result = ctrl.run_upscale();
            ctrl.complete_smart_op("upscaleSuccess", "upscaleFailed", result);
            ctrl.tools_menu_open = false;
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
                sync_editor(&ui, &mut ctrl);
            }
            schedule_toast_hide(&ui_weak, controller.clone());
            wake(&ui_weak);
        }
    });
    ui.on_run_smart_matte({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let result = ctrl.run_smart_matte();
            ctrl.complete_smart_op("smartMatteSuccess", "smartMatteFailed", result);
            ctrl.tools_menu_open = false;
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
                sync_editor(&ui, &mut ctrl);
            }
            schedule_toast_hide(&ui_weak, controller.clone());
            wake(&ui_weak);
        }
    });
    ui.on_run_seam_carve({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let result = ctrl.run_seam_carve();
            ctrl.complete_smart_op("seamCarveSuccess", "seamCarveFailed", result);
            ctrl.tools_menu_open = false;
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
                sync_editor(&ui, &mut ctrl);
            }
            schedule_toast_hide(&ui_weak, controller.clone());
            wake(&ui_weak);
        }
    });
}

fn wire_color_picker(ui: &AppWindow, controller: SharedController, ui_weak: SharedUi) {
    ui.on_select_color_swatch({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |index| {
            let mut ctrl = controller.borrow_mut();
            ctrl.select_quick_color(index as usize);
            if let Some(ui) = ui_weak.upgrade() {
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.on_color_sb_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |x, y| {
            let mut ctrl = controller.borrow_mut();
            ctrl.set_color_sb(x, 1.0 - y);
            if let Some(ui) = ui_weak.upgrade() {
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.on_color_hue_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |hue| {
            let mut ctrl = controller.borrow_mut();
            ctrl.set_color_hue(hue);
            if let Some(ui) = ui_weak.upgrade() {
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.on_color_hex_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |text| {
            let mut ctrl = controller.borrow_mut();
            let hex = ctrl.commit_color_hex(&text.to_string());
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_color_hex_text(hex.into());
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
}

fn wire_modals(
    ui: &AppWindow,
    controller: SharedController,
    host: Rc<RefCell<BoardHost>>,
    ui_weak: SharedUi,
) {
    ui.on_settings_dismissed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().settings_open = false;
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &controller.borrow());
            }
        }
    });

    ui.on_set_theme_light({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let _ = ctrl.set_theme_dark(false);
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
            }
        }
    });

    ui.on_set_theme_dark({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let _ = ctrl.set_theme_dark(true);
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
            }
        }
    });

    ui.on_set_language_en({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let _ = ctrl.set_language("en");
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
            }
        }
    });

    ui.on_new_project_dismissed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().new_project_open = false;
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &controller.borrow());
            }
        }
    });

    ui.on_create_from_modal({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let host = host.clone();
        move || {
            let ui = ui_weak.upgrade().unwrap();
            let name = ui.get_project_name().to_string();
            let width = parse_dimension(&ui.get_width_text().to_string(), DEFAULT_WIDTH);
            let height = parse_dimension(&ui.get_height_text().to_string(), DEFAULT_HEIGHT);
            let mut ctrl = controller.borrow_mut();
            ctrl.new_project_open = false;
            if ctrl.create_project(&name, width, height).is_ok() {
                set_editor_open(&ui, &mut ctrl, true);
                host.borrow_mut().set_active(true);
                setup_board(&ui_weak, &host);
                ctrl.show_toast_key("projectCreated", false);
            }
            sync_shell(&ui, &ctrl);
            schedule_toast_hide(&ui_weak, controller.clone());
        }
    });

    ui.on_toast_dismissed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().dismiss_toast();
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &controller.borrow());
            }
        }
    });
}

fn schedule_toast_hide(ui_weak: &SharedUi, controller: SharedController) {
    slint::Timer::single_shot(Duration::from_secs(3), {
        let ui_weak = ui_weak.clone();
        let controller = controller.clone();
        move || {
            controller.borrow_mut().dismiss_toast();
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &controller.borrow());
            }
        }
    });
}

fn wire_shell_keys(
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
            {
                let mut state = input.borrow_mut();
                match handle_key_press_for_modifiers(&text, mods) {
                    KeyPressModifierAction::Space(held) => state.mods.space_held = held,
                    KeyPressModifierAction::Alt(held) => state.mods.alt_held = held,
                    KeyPressModifierAction::None => {}
                }
                state.merge_shell(control, meta, shift, alt);
            }
            host.borrow_mut()
                .modifiers_changed(input.borrow().mods, controller.borrow().any_modal_open());

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
                ShellKeyAction::Escape => {
                    let mut ctrl = controller.borrow_mut();
                    if ctrl.toast_visible {
                        ctrl.dismiss_toast();
                    } else if ctrl.any_modal_open() {
                        ctrl.dismiss_modals();
                    }
                }
                ShellKeyAction::OpenSettings => {
                    controller.borrow_mut().settings_open = true;
                }
                ShellKeyAction::NewProject => {
                    controller.borrow_mut().new_project_open = true;
                }
                ShellKeyAction::ToggleLayers => {
                    let mut ctrl = controller.borrow_mut();
                    let _ = ctrl.toggle_layers_panel();
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
                            .pick_tool(calumma_core::Tool::Transform);
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
            host.borrow_mut()
                .modifiers_changed(input.borrow().mods, controller.borrow().any_modal_open());
        }
    });
}

fn setup_board(ui_weak: &SharedUi, host: &Rc<RefCell<BoardHost>>) {
    if let Some(ui) = ui_weak.upgrade() {
        sync_board_geometry(&ui, host);
    }
}

fn sync_board_geometry(ui: &AppWindow, host: &Rc<RefCell<BoardHost>>) {
    let scale = ui.window().scale_factor();
    let content_height = ui.window().size().to_logical(scale).height;
    let layout = board_layout(
        ui.get_board_x(),
        ui.get_board_y(),
        ui.get_board_width(),
        ui.get_board_height(),
    );
    ui.window().with_winit_window(|winit_window| {
        host.borrow_mut()
            .sync_geometry(winit_window, &layout, content_height, scale);
        host.borrow_mut().render();
    });
}

fn set_window_icon(ui: &AppWindow, root: &std::path::Path) {
    let path = root.join("design").join("icon.png");
    let bytes = std::fs::read(&path).ok();
    ui.window().with_winit_window(|window| {
        if let Some(bytes) = &bytes {
            if let Ok(image) = image::load_from_memory(bytes) {
                let rgba = image.to_rgba8();
                let (width, height) = rgba.dimensions();
                if let Ok(icon) = winit::window::Icon::from_rgba(rgba.into_raw(), width, height) {
                    window.set_window_icon(Some(icon));
                }
            }
        }
    });
    // winit's window icon is a no-op on macOS (title bars don't carry one) — the Dock icon is
    // a separate, process-wide setting only AppKit exposes.
    #[cfg(target_os = "macos")]
    if let Some(bytes) = bytes {
        set_dock_icon(&bytes);
    }
}

#[cfg(target_os = "macos")]
fn set_dock_icon(png_bytes: &[u8]) {
    use objc2::rc::Retained;
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let data = NSData::with_bytes(png_bytes);
    let image: Option<Retained<NSImage>> =
        unsafe { NSImage::initWithData(NSImage::alloc(), &data) };
    if let Some(image) = image {
        let app = NSApplication::sharedApplication(mtm);
        unsafe { app.setApplicationIconImage(Some(&image)) };
    }
}

fn start_frame_loop(
    ui_weak: SharedUi,
    host: Rc<RefCell<BoardHost>>,
    controller: SharedController,
) -> Vec<slint::Timer> {
    let board = slint::Timer::default();
    board.start(slint::TimerMode::Repeated, Duration::from_millis(16), {
        let ui_weak = ui_weak.clone();
        let host = host.clone();
        let controller = controller.clone();
        move || {
            if !controller.borrow().editor_open {
                return;
            }
            if let Some(ui) = ui_weak.upgrade() {
                sync_board_geometry(&ui, &host);
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
                    shell::format_bytes(ctrl.engine.borrow().resident_memory_bytes()).into(),
                );
                sync_layers(&ui, &mut ctrl);
            }
        }
    });

    vec![board, stats]
}

fn wake(ui_weak: &SharedUi) {
    if let Some(ui) = ui_weak.upgrade() {
        ui.window().request_redraw();
    }
}
