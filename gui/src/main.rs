mod app_icon;
mod board;
mod input;
mod shell;
mod ui_bridge;
mod window_chrome;

use board::{board_layout, hole_in_board, BoardHost, BoardRect, ModifierState};
use calumma_io::RasterFormat;
use i_slint_backend_winit::WinitWindowAccessor;
use input::{
    handle_key_press_for_modifiers, handle_key_release, handle_shell_key, EditorKeyAction,
    KeyPressModifierAction, KeyReleaseAction, Modifiers, ShellKeyAction,
};
use shell::{pick_artwork_file, shared, workspace_root, SharedController, TabCloseResult, Theme};
use slint::ComponentHandle;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};
use ui_bridge::{
    camera_signature, form_accent, init_form_defaults, parse_dimension, random_accent_index,
    refresh_landing, set_editor_open, sync_editor, sync_guide_readout, sync_guides,
    sync_layer_rows, sync_layer_settings, sync_layers, sync_project_tabs, sync_rulers, sync_shell,
    sync_zoom_chrome, AppWindow, SharedUi, ToolChrome, DEFAULT_HEIGHT, DEFAULT_WIDTH,
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

fn init_platform() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = i_slint_backend_winit::Backend::builder();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::WindowAttributesExtMacOS;
        builder = builder.with_window_attributes_hook(|attributes| {
            attributes
                .with_titlebar_transparent(true)
                .with_fullsize_content_view(true)
                .with_title_hidden(true)
        });
    }
    slint::platform::set_platform(Box::new(builder.build()?))?;
    #[cfg(feature = "mcp-devtools")]
    if let Err(err) = i_slint_backend_testing::mcp_server::init() {
        eprintln!("mcp-devtools: failed to start Slint MCP server: {err:?}");
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_platform()?;

    let root = workspace_root();
    let window_metrics = Theme::window_metrics(&root)?;
    let icons_root = root.join("design/icons");
    let controller = shared(root.clone())?;
    let host = Rc::new(RefCell::new(BoardHost::new(
        controller.borrow().engine.clone(),
        icons_root,
    )));

    let ui = AppWindow::new()?;
    set_window_icon(&ui);
    window_chrome::apply(&ui, controller.borrow().prefs.is_dark());
    {
        ui.window().set_size(slint::LogicalSize::new(
            window_metrics.width as f32,
            window_metrics.height as f32,
        ));
    }
    let ui_weak = ui.as_weak();

    let restored = {
        let mut ctrl = controller.borrow_mut();
        refresh_landing(&ui, &ctrl);
        init_form_defaults(&ui, &ctrl.l10n);
        ui.set_layers_open(ctrl.prefs.layers_panel_open);
        let restored = ctrl.restore_open_tabs();
        if restored.is_some() {
            ctrl.editor_open = true;
            ui.set_editor_open(true);
            host.borrow_mut().set_active(true);
        } else {
            ctrl.editor_open = false;
            ui.set_editor_open(false);
            host.borrow_mut().set_active(false);
        }
        restored
    };
    if let Some(summary) = restored {
        deferred_load_project(summary, controller.clone(), ui_weak.clone(), host.clone());
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
            let width = parse_dimension(ui.get_width_text().as_ref(), DEFAULT_WIDTH);
            let height = parse_dimension(ui.get_height_text().as_ref(), DEFAULT_HEIGHT);
            let accent = form_accent(&ui);
            let mut ctrl = controller.borrow_mut();
            if ctrl
                .create_project(&name, width, height, Some(accent))
                .is_ok()
            {
                ui.set_project_accent_index(random_accent_index());
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
            let summary = {
                let mut ctrl = controller.borrow_mut();
                ctrl.prepare_switch_to(&id)
            };
            if let Some(summary) = summary {
                deferred_load_project(summary, controller.clone(), ui_weak.clone(), host.clone());
            } else {
                let ctrl = controller.borrow();
                sync_project_tabs(&ui, &ctrl);
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
            let result = controller.borrow_mut().delete_project(&id);
            handle_tab_close_result(
                result,
                controller.clone(),
                ui_weak.clone(),
                host.clone(),
                &ui,
            );
        }
    });

    ui.on_clear_recents({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let host = host.clone();
        move || {
            let ui = ui_weak.upgrade().unwrap();
            let mut ctrl = controller.borrow_mut();
            let title = ctrl.l10n.get("clearAllRecentsTitle");
            let message = ctrl.l10n.get("clearAllRecentsMessage");
            let ok = ctrl.l10n.get("clearAllRecents");
            let cancel = ctrl.l10n.get("cancel");
            let cleared = ctrl
                .clear_recents_confirmed(&title, &message, &ok, &cancel)
                .unwrap_or(false);
            if cleared {
                host.borrow_mut().set_active(false);
                ui.set_editor_open(false);
                ui.set_loading(false);
                refresh_landing(&ui, &ctrl);
            } else {
                sync_shell(&ui, &ctrl);
            }
        }
    });

    ui.on_app_icon_clicked(shell::play_meow);

    ui.on_paste_artwork_clicked({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let host = host.clone();
        move || {
            let filter = controller.borrow().l10n.get("imagesFilter");
            let Some(bytes) = pick_artwork_file(&filter) else {
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
    ui.on_switch_project_tab({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let host = host.clone();
        move |id| {
            let id = id.to_string();
            let summary = {
                let mut ctrl = controller.borrow_mut();
                ctrl.bump_load_generation();
                ctrl.prepare_switch_to(&id)
            };
            if let Some(summary) = summary {
                deferred_load_project(summary, controller.clone(), ui_weak.clone(), host.clone());
            }
        }
    });

    ui.on_close_project_tab({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let host = host.clone();
        move |id| {
            let ui = ui_weak.upgrade().unwrap();
            let id = id.to_string();
            let result = controller.borrow_mut().close_project_tab(&id);
            handle_tab_close_result(
                result,
                controller.clone(),
                ui_weak.clone(),
                host.clone(),
                &ui,
            );
        }
    });

    ui.on_edit_project_tab({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |id, anchor_x, anchor_y| {
            let id = id.to_string();
            let mut ctrl = controller.borrow_mut();
            ctrl.open_project_settings(&id, anchor_x, anchor_y);
            let name = ctrl
                .engine
                .borrow()
                .project_summary(&id)
                .map(|summary| summary.name)
                .unwrap_or_default();
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_project_settings_name(slint::SharedString::from(name));
                sync_shell(&ui, &ctrl);
            }
        }
    });

    ui.on_pick_tool({
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
            ctrl.commit_brush_size(text.as_ref());
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

    ui.on_pick_brush({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |id| {
            if let Some(brush) = calumma_core::Brush::from_u32(id as u32) {
                controller.borrow_mut().engine.borrow_mut().set_brush(brush);
                defer_sync_editor_only(&ui_weak, &controller);
            }
        }
    });
    ui.on_blur_changed({
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
    ui.on_hardness_changed({
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
    ui.on_tolerance_changed({
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
    ui.on_eyedropper_changed({
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
    ui.on_fill_toggled({
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
    ui.on_stroke_toggled({
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
    ui.global::<ToolChrome>().on_text_family_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |family| {
            controller
                .borrow_mut()
                .engine
                .borrow_mut()
                .set_text_family(family.as_str());
            if let Some(ui) = ui_weak.upgrade() {
                ui.global::<ToolChrome>()
                    .set_text_font_query(slint::SharedString::from(""));
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.global::<ToolChrome>().on_text_size_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |unit| {
            controller
                .borrow_mut()
                .engine
                .borrow_mut()
                .set_text_size_unit(unit);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.global::<ToolChrome>().on_text_size_committed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |text| {
            if let Ok(size) = text.trim().parse::<f32>() {
                controller
                    .borrow_mut()
                    .engine
                    .borrow_mut()
                    .set_text_size(size);
            }
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.global::<ToolChrome>().on_text_line_height_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |value| {
            controller
                .borrow_mut()
                .engine
                .borrow_mut()
                .set_text_line_height(value);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.global::<ToolChrome>().on_text_wrap_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |width| {
            controller
                .borrow_mut()
                .engine
                .borrow_mut()
                .set_text_wrap_width(width);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.global::<ToolChrome>().on_text_bold_toggled({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let next = !controller.borrow().engine.borrow().text_bold();
            controller
                .borrow_mut()
                .engine
                .borrow_mut()
                .set_text_bold(next);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.global::<ToolChrome>().on_text_italic_toggled({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let next = !controller.borrow().engine.borrow().text_italic();
            controller
                .borrow_mut()
                .engine
                .borrow_mut()
                .set_text_italic(next);
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.global::<ToolChrome>().on_text_align_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |align| {
            if let Some(align) = calumma_core::TextAlign::from_u32(align as u32) {
                controller
                    .borrow_mut()
                    .engine
                    .borrow_mut()
                    .set_text_align(align);
            }
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.global::<ToolChrome>().on_text_font_search({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |_| {
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
        }
    });
    ui.on_vector_toggled({
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
    ui.on_aligned_toggled({
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
    ui.on_transform_toggled({
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
    ui.on_crop_aspect_changed({
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
    ui.on_crop_overlay_changed({
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
    ui.on_commit_crop({
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
    ui.on_cancel_crop({
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
    ui.on_commit_layer_bounds({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |x, y, w, h| {
            controller.borrow_mut().commit_layer_bounds(
                &x.to_string(),
                &y.to_string(),
                &w.to_string(),
                &h.to_string(),
            );
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_layers(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.on_commit_canvas_size({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |w, h| {
            controller
                .borrow_mut()
                .commit_canvas_size(&w.to_string(), &h.to_string());
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                sync_editor(&ui, &mut ctrl);
            }
            wake(&ui_weak);
        }
    });

    ui.on_zoom_changed({
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

    ui.on_guide_pressed({
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
    ui.on_guide_moved({
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
    ui.on_guide_released({
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

    ui.on_pointer_pressed({
        let host = host.clone();
        let input = input.clone();
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |x, y, middle, meta, alt, shift| {
            let mods = {
                let mut state = input.borrow_mut();
                state.mods.meta_held = meta;
                state.mods.alt_held = alt;
                state.mods.shift_held = shift;
                state.mods
            };
            host.borrow_mut().pointer_pressed(x, y, mods, middle);
            refresh_board_cursor(&host, &controller, &input);
            if controller.borrow().editor_open {
                host.borrow_mut().render();
            }
            if let Some(ui) = ui_weak.upgrade() {
                let mut ctrl = controller.borrow_mut();
                ctrl.announce_tool_block_if_any();
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
            if controller.borrow().editor_open {
                host.borrow_mut().render();
            }
            if controller.borrow().engine.borrow().is_dragging_guide() {
                if let Some(ui) = ui_weak.upgrade() {
                    sync_guide_readout(&ui, &controller.borrow());
                }
            }
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
                sync_guide_readout(&ui, &ctrl);
                sync_guides(&ui, &ctrl);
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
        let controller = controller.clone();
        let input = input.clone();
        let ui_weak = ui_weak.clone();
        move |x, y, dx, dy, alt, meta| {
            if controller.borrow().pinch_zoom.is_some() {
                return;
            }
            input.borrow_mut().mods.alt_held = alt;
            input.borrow_mut().mods.meta_held = meta;
            host.borrow_mut().scroll(x, y, dx, dy, alt, meta);
            if controller.borrow().editor_open {
                host.borrow_mut().render();
            }
            wake(&ui_weak);
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
        let host = host.clone();
        let ui_weak = ui_weak.clone();
        move |x, y, scale| {
            controller.borrow_mut().pinch_updated(x, y, scale);
            if controller.borrow().editor_open {
                host.borrow_mut().render();
            }
            wake(&ui_weak);
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

fn defer_sync_editor(
    ui_weak: &SharedUi,
    controller: &SharedController,
    host: &Rc<RefCell<BoardHost>>,
    input: &Rc<RefCell<InputState>>,
) {
    let ui_weak = ui_weak.clone();
    let controller = controller.clone();
    let host = host.clone();
    let input = input.clone();
    slint::Timer::single_shot(Duration::ZERO, move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut ctrl = controller.borrow_mut();
            sync_editor(&ui, &mut ctrl);
            drop(ctrl);
            refresh_board_cursor(&host, &controller, &input);
            wake(&ui_weak);
        }
    });
}

fn defer_sync_editor_only(ui_weak: &SharedUi, controller: &SharedController) {
    let ui_weak = ui_weak.clone();
    let controller = controller.clone();
    slint::Timer::single_shot(Duration::ZERO, move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut ctrl = controller.borrow_mut();
            sync_editor(&ui, &mut ctrl);
            wake(&ui_weak);
        }
    });
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
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().new_project_open = true;
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &controller.borrow());
            }
        }
    });
    ui.on_menu_settings({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            ctrl.open_settings();
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
            }
            wake(&ui_weak);
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
    ui.on_titlebar_drag({
        let ui_weak = ui_weak.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                window_chrome::drag(&ui);
            }
        }
    });
    ui.on_titlebar_zoom({
        let ui_weak = ui_weak.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                window_chrome::zoom(&ui);
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
        move |index, anchor_x, anchor_y| {
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
    ui.on_rename_layer({
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
    ui.on_reorder_layer({
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
            let mut ctrl = controller.borrow_mut();
            ctrl.layer_settings_open = false;
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
            }
            wake(&ui_weak);
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
    ui.on_layer_settings_toggle_lock({
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
    ui.on_layer_settings_opacity_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |opacity| {
            let mut ctrl = controller.borrow_mut();
            let index = ctrl.layer_settings_index;
            ctrl.set_layer_opacity(index, opacity);
            if let Some(ui) = ui_weak.upgrade() {
                sync_layer_settings(&ui, &ctrl);
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
    ui.on_layer_settings_blend_changed({
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
    ui.on_layer_settings_filter_changed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |kind, value| {
            let mut ctrl = controller.borrow_mut();
            let index = ctrl.layer_settings_index;
            ctrl.set_layer_filter(index, kind, value);
            if let Some(ui) = ui_weak.upgrade() {
                sync_layer_settings(&ui, &ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.on_layer_settings_reset_filters({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let index = ctrl.layer_settings_index;
            ctrl.reset_layer_filters(index);
            if let Some(ui) = ui_weak.upgrade() {
                sync_layer_settings(&ui, &ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.on_layer_settings_rename({
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
    ui.on_layer_settings_toggle_clip({
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
    ui.on_layer_settings_flatten_clip({
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
    ui.on_layer_settings_merge_down({
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
    ui.on_layer_settings_reset_transform({
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
    ui.on_layer_settings_move_up({
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
    ui.on_layer_settings_move_down({
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
    ui.on_layer_settings_rasterize({
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

fn wire_tools(ui: &AppWindow, controller: SharedController, ui_weak: SharedUi) {
    ui.on_run_upscale({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            let mut ctrl = controller.borrow_mut();
            let result = ctrl.run_upscale();
            ctrl.complete_smart_op("upscaleSuccess", "upscaleFailed", result);
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
            let hex = ctrl.commit_color_hex(text.as_ref());
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
                if ctrl.editor_open {
                    sync_editor(&ui, &mut ctrl);
                }
                window_chrome::apply_appearance(&ui, false);
            }
            wake(&ui_weak);
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
                if ctrl.editor_open {
                    sync_editor(&ui, &mut ctrl);
                }
                window_chrome::apply_appearance(&ui, true);
            }
            wake(&ui_weak);
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
                if ctrl.editor_open {
                    sync_editor(&ui, &mut ctrl);
                }
            }
            wake(&ui_weak);
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

    ui.on_open_guides({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().open_guides();
            if let Some(ui) = ui_weak.upgrade() {
                let ctrl = controller.borrow();
                if ui.get_add_guide_offset().is_empty() {
                    ui.set_add_guide_offset(slint::SharedString::from("0"));
                }
                sync_shell(&ui, &ctrl);
                sync_guides(&ui, &ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.on_guides_dismissed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().guides_open = false;
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &controller.borrow());
            }
        }
    });
    ui.on_add_guide({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                let horizontal = ui.get_add_guide_horizontal();
                let offset = ui.get_add_guide_offset().to_string();
                controller
                    .borrow_mut()
                    .add_guide_from_card(horizontal, &offset);
                let ctrl = controller.borrow();
                sync_guides(&ui, &ctrl);
            }
            wake(&ui_weak);
        }
    });
    ui.on_remove_guide({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |index| {
            controller.borrow_mut().remove_guide(index as usize);
            if let Some(ui) = ui_weak.upgrade() {
                sync_guides(&ui, &controller.borrow());
            }
            wake(&ui_weak);
        }
    });
    ui.on_clear_guides({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().clear_guides();
            if let Some(ui) = ui_weak.upgrade() {
                sync_guides(&ui, &controller.borrow());
            }
            wake(&ui_weak);
        }
    });
    ui.on_set_guide_axis({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |index, horizontal| {
            controller
                .borrow_mut()
                .set_guide_axis(index as usize, horizontal);
            if let Some(ui) = ui_weak.upgrade() {
                sync_guides(&ui, &controller.borrow());
            }
            wake(&ui_weak);
        }
    });
    ui.on_set_guide_offset({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |index, text| {
            controller
                .borrow_mut()
                .set_guide_offset(index as usize, text.as_str());
            if let Some(ui) = ui_weak.upgrade() {
                sync_guides(&ui, &controller.borrow());
            }
            wake(&ui_weak);
        }
    });
    ui.on_set_guide_color({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |index, palette_index| {
            controller
                .borrow_mut()
                .set_guide_color(index as usize, palette_index as usize);
            if let Some(ui) = ui_weak.upgrade() {
                sync_guides(&ui, &controller.borrow());
            }
            wake(&ui_weak);
        }
    });

    ui.on_create_from_modal({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        let host = host.clone();
        move || {
            let ui = ui_weak.upgrade().unwrap();
            let name = ui.get_project_name().to_string();
            let width = parse_dimension(ui.get_width_text().as_ref(), DEFAULT_WIDTH);
            let height = parse_dimension(ui.get_height_text().as_ref(), DEFAULT_HEIGHT);
            let accent = form_accent(&ui);
            let mut ctrl = controller.borrow_mut();
            ctrl.new_project_open = false;
            if ctrl
                .create_project(&name, width, height, Some(accent))
                .is_ok()
            {
                ui.set_project_accent_index(random_accent_index());
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

    ui.on_project_settings_dismissed({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move || {
            controller.borrow_mut().project_settings_open = false;
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &controller.borrow());
            }
        }
    });

    ui.on_rename_project({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |name| {
            let mut ctrl = controller.borrow_mut();
            if ctrl.rename_open_project_settings(name.as_str()).is_err() {
                return;
            }
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
                sync_project_tabs(&ui, &ctrl);
            }
        }
    });

    ui.on_recolor_project({
        let controller = controller.clone();
        let ui_weak = ui_weak.clone();
        move |index| {
            let mut ctrl = controller.borrow_mut();
            if ctrl.recolor_open_project_settings(index as usize).is_err() {
                return;
            }
            if let Some(ui) = ui_weak.upgrade() {
                sync_shell(&ui, &ctrl);
                sync_project_tabs(&ui, &ctrl);
            }
            wake(&ui_weak);
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
            controller
                .borrow_mut()
                .refresh_guide_shift(input.borrow().mods.shift_held);
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
            host.borrow_mut()
                .modifiers_changed(input.borrow().mods, controller.borrow().any_modal_open());
            controller
                .borrow_mut()
                .refresh_guide_shift(input.borrow().mods.shift_held);
            if let Some(ui) = ui_weak.upgrade() {
                sync_guide_readout(&ui, &controller.borrow());
            }
        }
    });
}

fn deferred_load_project(
    summary: calumma_app::ProjectSummary,
    controller: SharedController,
    ui_weak: SharedUi,
    host: Rc<RefCell<BoardHost>>,
) {
    let id = summary.id.clone();
    let ui = ui_weak.upgrade().unwrap();
    ui.set_editor_open(true);
    ui.set_loading(true);
    ui.set_loading_doc_width(summary.width as f32);
    ui.set_loading_doc_height(summary.height as f32);
    host.borrow_mut().set_active(true);
    setup_board(&ui_weak, &host);
    sync_project_tabs(&ui, &controller.borrow());

    let gen = controller.borrow_mut().bump_load_generation();
    let controller = controller.clone();
    let ui_weak = ui_weak.clone();
    let host = host.clone();
    slint::Timer::single_shot(Duration::ZERO, move || {
        let Some(ui) = ui_weak.upgrade() else {
            return;
        };
        let mut ctrl = controller.borrow_mut();
        if ctrl.load_generation != gen {
            return;
        }
        if ctrl.load_project(&id).is_ok() {
            set_editor_open(&ui, &mut ctrl, true);
        } else {
            ui.set_editor_open(false);
            host.borrow_mut().set_active(false);
        }
        sync_shell(&ui, &ctrl);
        drop(ctrl);

        let ui_weak = ui_weak.clone();
        slint::Timer::single_shot(Duration::from_millis(200), move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_loading(false);
            }
        });
    });
}

fn handle_tab_close_result(
    result: TabCloseResult,
    controller: SharedController,
    ui_weak: SharedUi,
    host: Rc<RefCell<BoardHost>>,
    ui: &AppWindow,
) {
    match result {
        TabCloseResult::Unchanged => {
            let ctrl = controller.borrow();
            sync_project_tabs(ui, &ctrl);
            sync_shell(ui, &ctrl);
        }
        TabCloseResult::SwitchTo(next) => {
            let summary = controller.borrow_mut().prepare_switch_to(&next);
            if let Some(summary) = summary {
                deferred_load_project(summary, controller, ui_weak, host);
            }
        }
        TabCloseResult::ShowLanding => {
            host.borrow_mut().set_active(false);
            ui.set_editor_open(false);
            ui.set_loading(false);
            refresh_landing(ui, &controller.borrow());
        }
    }
}

fn setup_board(ui_weak: &SharedUi, host: &Rc<RefCell<BoardHost>>) {
    if let Some(ui) = ui_weak.upgrade() {
        sync_board_geometry(&ui, host, true);
    }
}

fn sync_board_geometry(ui: &AppWindow, host: &Rc<RefCell<BoardHost>>, present: bool) {
    let scale = ui.window().scale_factor();
    let content_height = ui.window().size().to_logical(scale).height;
    let overlay = ui.get_overlay_chrome_open();
    let layout = board_layout(
        ui.get_board_x(),
        ui.get_board_y(),
        ui.get_board_width(),
        ui.get_board_height(),
    );
    let mut holes = Vec::new();
    let mut punch = |chrome: BoardRect| {
        if let Some(hole) = hole_in_board(&layout, chrome) {
            holes.push(hole);
        }
    };
    if !ui.get_layer_settings_open() {
        punch(BoardRect {
            x: ui.get_zoom_chrome_x(),
            y: ui.get_zoom_chrome_y(),
            width: ui.get_zoom_chrome_width(),
            height: ui.get_zoom_chrome_height(),
            radius: ui.get_zoom_chrome_radius(),
        });
    }
    if ui.get_hover_preview_visible() && !ui.get_layer_settings_open() {
        punch(BoardRect {
            x: ui.get_hover_chrome_x(),
            y: ui.get_hover_chrome_y(),
            width: ui.get_hover_chrome_width(),
            height: ui.get_hover_chrome_height(),
            radius: ui.get_hover_chrome_radius(),
        });
    }
    if ui.get_guides_open() {
        punch(BoardRect {
            x: ui.get_guides_chrome_x(),
            y: ui.get_guides_chrome_y(),
            width: ui.get_guides_chrome_width(),
            height: ui.get_guides_chrome_height(),
            radius: ui.get_guides_chrome_radius(),
        });
    }
    if ui.get_new_project_open() {
        punch(BoardRect {
            x: ui.get_new_project_chrome_x(),
            y: ui.get_new_project_chrome_y(),
            width: ui.get_new_project_chrome_width(),
            height: ui.get_new_project_chrome_height(),
            radius: ui.get_new_project_chrome_radius(),
        });
    }
    if ui.get_layer_settings_open() {
        punch(BoardRect {
            x: ui.get_layer_settings_chrome_x(),
            y: ui.get_layer_settings_chrome_y(),
            width: ui.get_layer_settings_chrome_width(),
            height: ui.get_layer_settings_chrome_height(),
            radius: ui.get_layer_settings_chrome_radius(),
        });
    }
    if ui.get_project_settings_open() {
        punch(BoardRect {
            x: ui.get_project_settings_chrome_x(),
            y: ui.get_project_settings_chrome_y(),
            width: ui.get_project_settings_chrome_width(),
            height: ui.get_project_settings_chrome_height(),
            radius: ui.get_project_settings_chrome_radius(),
        });
    }
    if ui.get_tip_visible() {
        punch(BoardRect {
            x: ui.get_tip_chrome_x(),
            y: ui.get_tip_chrome_y(),
            width: ui.get_tip_chrome_width(),
            height: ui.get_tip_chrome_height(),
            radius: ui.get_tip_chrome_radius(),
        });
    }
    ui.window().with_winit_window(|winit_window| {
        host.borrow_mut().sync_geometry(
            winit_window,
            &layout,
            content_height,
            scale,
            overlay,
            &holes,
        );
        if present && !overlay {
            host.borrow_mut().render();
        }
    });
}

fn set_window_icon(ui: &AppWindow) {
    if let Ok(loaded) = image::load_from_memory(app_icon::dock_png()) {
        let rgba = loaded.to_rgba8();
        let (width, height) = rgba.dimensions();
        let raw = rgba.into_raw();
        ui.window().with_winit_window(|window| {
            if let Ok(icon) = winit::window::Icon::from_rgba(raw.clone(), width, height) {
                window.set_window_icon(Some(icon));
            }
        });
    }
    #[cfg(target_os = "macos")]
    set_dock_icon(app_icon::dock_png());
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
    let image: Option<Retained<NSImage>> = NSImage::initWithData(NSImage::alloc(), &data);
    if let Some(image) = image {
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(objc2_app_kit::NSApplicationActivationPolicy::Regular);
        unsafe { app.setApplicationIconImage(Some(&image)) };
    }
}

fn start_frame_loop(
    ui_weak: SharedUi,
    host: Rc<RefCell<BoardHost>>,
    controller: SharedController,
) -> Vec<slint::Timer> {
    let icon_done = Rc::new(Cell::new(false));
    let board = slint::Timer::default();
    let camera = Rc::new(RefCell::new((f32::NAN, f32::NAN, f32::NAN)));
    let last_present = Rc::new(Cell::new(Instant::now() - Duration::from_secs(1)));
    board.start(slint::TimerMode::Repeated, Duration::from_millis(8), {
        let ui_weak = ui_weak.clone();
        let host = host.clone();
        let controller = controller.clone();
        let icon_done = icon_done.clone();
        let last_present = last_present.clone();
        move || {
            if !icon_done.get() {
                if let Some(ui) = ui_weak.upgrade() {
                    set_window_icon(&ui);
                    window_chrome::apply(&ui, controller.borrow().prefs.is_dark());
                    icon_done.set(true);
                }
            }
            if !controller.borrow().editor_open {
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
            if let Some(ui) = ui_weak.upgrade() {
                sync_board_geometry(&ui, &host, present);
                let ctrl = controller.borrow();
                let next = camera_signature(&ctrl);
                if *camera.borrow() != next {
                    *camera.borrow_mut() = next;
                    sync_rulers(&ui, &ctrl);
                    sync_zoom_chrome(&ui, &ctrl);
                }
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

fn wake(ui_weak: &SharedUi) {
    if let Some(ui) = ui_weak.upgrade() {
        ui.window().request_redraw();
    }
}
