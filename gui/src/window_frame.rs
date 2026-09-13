use i_slint_backend_winit::WinitWindowAccessor;
use slint::{ComponentHandle, LogicalPosition, LogicalSize};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use crate::input::FrameSignal;
use crate::shell::{EditorWindow, SharedController, ShellPrefs, WindowBounds};
use crate::ui_bridge::AppWindow;

const REMEMBER_DEBOUNCE: Duration = Duration::from_millis(500);
const FULLSCREEN_EXIT_SETTLE: Duration = Duration::from_millis(700);

pub fn wire(
    ui: &AppWindow,
    controller: SharedController,
    frame_changed: FrameSignal,
    landing: LogicalSize,
    editor_open: bool,
) -> slint::Timer {
    let showing_editor = Rc::new(Cell::new(editor_open));
    ui.on_editor_open_changed({
        let controller = controller.clone();
        let ui_weak = ui.as_weak();
        let showing_editor = showing_editor.clone();
        move |open| {
            let Some(ui) = ui_weak.upgrade() else { return };
            if showing_editor.replace(open) == open {
                return;
            }
            if open {
                apply_editor(&ui, controller.borrow().prefs.editor_window);
            } else {
                remember_editor(&ui, &mut controller.borrow_mut().prefs);
                apply_landing(&ui, landing);
            }
        }
    });

    let timer = slint::Timer::default();
    let ui_weak = ui.as_weak();
    timer.start(slint::TimerMode::Repeated, REMEMBER_DEBOUNCE, move || {
        if !frame_changed.take() || !showing_editor.get() {
            return;
        }
        let Some(ui) = ui_weak.upgrade() else { return };
        let Ok(mut ctrl) = controller.try_borrow_mut() else {
            frame_changed.mark();
            return;
        };
        remember_editor(&ui, &mut ctrl.prefs);
    });
    timer
}

pub fn apply_editor(ui: &AppWindow, saved: Option<EditorWindow>) {
    let window = ui.window();
    let Some(saved) = saved else {
        set_maximized(ui, true);
        return;
    };
    if let Some(bounds) = saved.bounds {
        window.set_size(LogicalSize::new(bounds.width as f32, bounds.height as f32));
        window.set_position(LogicalPosition::new(bounds.x as f32, bounds.y as f32));
    }
    if saved.fullscreen {
        set_fullscreen(ui, true);
    } else if saved.maximized || saved.bounds.is_none() {
        set_maximized(ui, true);
    }
}

pub fn apply_landing(ui: &AppWindow, size: LogicalSize) {
    let was_fullscreen = ui
        .window()
        .with_winit_window(|window| window.fullscreen().is_some())
        .unwrap_or(false);
    set_fullscreen(ui, false);
    set_maximized(ui, false);
    fit_landing(ui, size);
    if was_fullscreen {
        let ui_weak = ui.as_weak();
        slint::Timer::single_shot(FULLSCREEN_EXIT_SETTLE, move || {
            if let Some(ui) = ui_weak.upgrade() {
                fit_landing(&ui, size);
            }
        });
    }
}

fn fit_landing(ui: &AppWindow, size: LogicalSize) {
    ui.window().set_size(size);
    ui.window().with_winit_window(|window| {
        let Some(monitor) = window.current_monitor() else {
            return;
        };
        let scale = monitor.scale_factor();
        let origin = monitor.position().to_logical::<f64>(scale);
        let area = monitor.size().to_logical::<f64>(scale);
        window.set_outer_position(winit::dpi::LogicalPosition::new(
            origin.x + (area.width - size.width as f64) / 2.0,
            origin.y + (area.height - size.height as f64) / 2.0,
        ));
    });
}

fn remember_editor(ui: &AppWindow, prefs: &mut ShellPrefs) {
    let Some(frame) = capture(ui, prefs.editor_window) else {
        return;
    };
    if prefs.editor_window != Some(frame) {
        prefs.editor_window = Some(frame);
        let _ = prefs.save();
    }
}

fn capture(ui: &AppWindow, previous: Option<EditorWindow>) -> Option<EditorWindow> {
    let window = ui.window();
    let (maximized, fullscreen) = window
        .with_winit_window(|window| (window.is_maximized(), window.fullscreen().is_some()))?;
    let bounds = if maximized || fullscreen {
        previous.and_then(|frame| frame.bounds)
    } else {
        let scale = window.scale_factor();
        let position = window.position().to_logical(scale);
        let size = window.size().to_logical(scale);
        Some(WindowBounds {
            x: position.x.round() as i32,
            y: position.y.round() as i32,
            width: size.width.round() as u32,
            height: size.height.round() as u32,
        })
    };
    Some(EditorWindow {
        maximized,
        fullscreen,
        bounds,
    })
}

fn set_maximized(ui: &AppWindow, maximized: bool) {
    let applied = ui
        .window()
        .with_winit_window(|window| window.set_maximized(maximized));
    if applied.is_none() {
        ui.window().set_maximized(maximized);
    }
}

fn set_fullscreen(ui: &AppWindow, fullscreen: bool) {
    let applied = ui.window().with_winit_window(|window| {
        window.set_fullscreen(fullscreen.then_some(winit::window::Fullscreen::Borderless(None)));
    });
    if applied.is_none() {
        ui.window().set_fullscreen(fullscreen);
    }
}
