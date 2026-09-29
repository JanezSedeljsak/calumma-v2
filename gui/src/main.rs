mod app_icon;
mod board;
mod board_geometry;
mod frame_loop;
mod input;
mod layer_selection;
mod shell;
mod ui_bridge;
mod window_chrome;
mod window_frame;
mod wiring;

use board::{BoardHost, ModifierState};
use input::{DropHandler, DropQueue, FrameSignal, ImeQueue, ShellEvents};
use shell::{shared, workspace_root, Theme};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use ui_bridge::{init_form_defaults, refresh_landing, AppWindow, FilterDebounce};
use wiring::{deferred_load_project, InputState};

fn init_platform(
    drops: &DropQueue,
    frame_changed: &FrameSignal,
    window_mods: Rc<RefCell<ModifierState>>,
    ime: &ImeQueue,
) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "linux")]
    if std::env::var_os("WINIT_UNIX_BACKEND").is_none() {
        unsafe {
            std::env::set_var("WINIT_UNIX_BACKEND", "x11");
        }
    }
    let mut builder = i_slint_backend_winit::Backend::builder().with_custom_application_handler(
        Box::new(ShellEvents {
            drops: DropHandler(drops.clone()),
            frame_changed: frame_changed.clone(),
            modifiers: window_mods,
            ime: ime.clone(),
        }),
    );
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
    let drops = DropQueue::default();
    let frame_changed = FrameSignal::default();
    let window_mods = Rc::new(RefCell::new(ModifierState::default()));
    let ime = ImeQueue::default();
    init_platform(&drops, &frame_changed, window_mods.clone(), &ime)?;

    let root = workspace_root();
    let window_metrics = Theme::window_metrics(&root)?;
    let icons_root = root.join("design/icons");
    let controller = shared(root.clone())?;
    let host = Rc::new(RefCell::new(BoardHost::new(
        controller.borrow().engine.clone(),
        icons_root,
    )));

    let ui = AppWindow::new()?;
    app_icon::set_window_icon(&ui);
    window_chrome::apply(&ui, controller.borrow().prefs.is_dark());
    let landing_size =
        slint::LogicalSize::new(window_metrics.width as f32, window_metrics.height as f32);
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
    let editor_open_at_start = restored.is_some();
    if editor_open_at_start {
        window_frame::apply_editor(&ui, controller.borrow().prefs.editor_window);
    } else {
        ui.window().set_size(landing_size);
    }
    let _window_frame_timer = window_frame::wire(
        &ui,
        controller.clone(),
        frame_changed,
        landing_size,
        editor_open_at_start,
    );
    if let Some(summary) = restored {
        deferred_load_project(summary, controller.clone(), ui_weak.clone(), host.clone());
    }

    slint::Timer::single_shot(Duration::ZERO, shell::preload_meow);

    wiring::landing::wire(&ui, controller.clone(), host.clone(), ui_weak.clone());
    let input = Rc::new(RefCell::new(InputState {
        mods: ModifierState::default(),
        window: window_mods.clone(),
    }));
    let filter_debounce = FilterDebounce::new();
    wiring::projects::wire(&ui, controller.clone(), host.clone(), ui_weak.clone());
    wiring::tool_options::wire(
        &ui,
        controller.clone(),
        host.clone(),
        ui_weak.clone(),
        input.clone(),
    );
    wiring::text_options::wire(&ui, controller.clone(), ui_weak.clone());
    wiring::board_input::wire(
        &ui,
        controller.clone(),
        host.clone(),
        ui_weak.clone(),
        input.clone(),
        filter_debounce.clone(),
    );
    wiring::modals::wire(&ui, controller.clone(), host.clone(), ui_weak.clone());
    wiring::menus::wire(
        &ui,
        controller.clone(),
        ui_weak.clone(),
        filter_debounce.clone(),
    );
    wiring::exports::wire(&ui, controller.clone(), ui_weak.clone());
    wiring::layer_actions::wire(&ui, controller.clone(), ui_weak.clone(), filter_debounce);
    layer_selection::wire(&ui, controller.clone(), ui_weak.clone());
    wiring::color_picker::wire(&ui, controller.clone(), ui_weak.clone());
    wiring::keys::wire(
        &ui,
        controller.clone(),
        ui_weak.clone(),
        input,
        host.clone(),
    );

    let _frame_timers = frame_loop::start(ui_weak, host, controller, drops, ime);

    ui.run()?;
    Ok(())
}
