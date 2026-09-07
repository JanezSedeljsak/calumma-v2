use i_slint_backend_winit::WinitWindowAccessor;
use slint::ComponentHandle;

use crate::ui_bridge::{AppWindow, Tokens};

#[cfg(target_os = "macos")]
const TITLEBAR_LEADING: f32 = 80.0;
#[cfg(target_os = "macos")]
const TITLEBAR_HEIGHT: f32 = 52.0;

#[cfg(not(target_os = "macos"))]
const TITLEBAR_LEADING: f32 = 0.0;
#[cfg(not(target_os = "macos"))]
const TITLEBAR_HEIGHT: f32 = 36.0;

pub fn apply(ui: &AppWindow) {
    let tokens = ui.global::<Tokens>();
    tokens.set_titlebar_leading(TITLEBAR_LEADING);
    tokens.set_titlebar_height(TITLEBAR_HEIGHT);
    ui.window().with_winit_window(|window| {
        #[cfg(target_os = "macos")]
        apply_macos(window);
        #[cfg(not(target_os = "macos"))]
        let _ = window;
    });
}

pub fn drag(ui: &AppWindow) {
    ui.window().with_winit_window(|window| {
        let _ = window.drag_window();
    });
}

pub fn zoom(ui: &AppWindow) {
    ui.window().with_winit_window(|window| {
        window.set_maximized(!window.is_maximized());
    });
}

#[cfg(target_os = "macos")]
fn apply_macos(window: &winit::window::Window) {
    use objc2::rc::Retained;
    use objc2_app_kit::{NSView, NSWindowStyleMask, NSWindowTitleVisibility};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let ns_view: Retained<NSView> = match handle.as_raw() {
        RawWindowHandle::AppKit(appkit) => {
            match unsafe { Retained::retain(appkit.ns_view.as_ptr().cast()) } {
                Some(view) => view,
                None => return,
            }
        }
        _ => return,
    };
    let Some(ns_window) = ns_view.window() else {
        return;
    };
    ns_window.setTitle(&objc2_foundation::NSString::from_str(""));
    ns_window.setTitlebarAppearsTransparent(true);
    ns_window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
    ns_window.setStyleMask(ns_window.styleMask() | NSWindowStyleMask::FullSizeContentView);
    ns_window.setMovableByWindowBackground(false);
    #[allow(deprecated)]
    objc2_foundation::NSProcessInfo::processInfo()
        .setProcessName(&objc2_foundation::NSString::from_str("Calumma"));
}
