use crate::ui_bridge::AppWindow;
use i_slint_backend_winit::WinitWindowAccessor;
use image::{Rgba, RgbaImage};
use slint::ComponentHandle;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::sync::OnceLock;

const APP_ICON_PNG: &[u8] = include_bytes!("../../design/icon.png");
const DOCK_ICON_PNG: &[u8] = include_bytes!("../../design/icon-rounded.png");
const BACKDROP: u8 = 22;

pub fn mark_image() -> Image {
    rgba_image(&mark_bitmap())
}

pub fn dock_png() -> &'static [u8] {
    DOCK_ICON_PNG
}

fn mark_bitmap() -> RgbaImage {
    static BITMAP: OnceLock<RgbaImage> = OnceLock::new();
    BITMAP
        .get_or_init(|| {
            let mut src = image::load_from_memory(APP_ICON_PNG)
                .expect("app icon png")
                .to_rgba8();
            knockout_backdrop(&mut src);
            src
        })
        .clone()
}

fn knockout_backdrop(img: &mut RgbaImage) {
    let (w, h) = img.dimensions();
    let mut seen = vec![false; (w * h) as usize];
    let mut stack: Vec<(u32, u32)> = Vec::new();
    for x in 0..w {
        stack.push((x, 0));
        if h > 1 {
            stack.push((x, h - 1));
        }
    }
    for y in 0..h {
        stack.push((0, y));
        if w > 1 {
            stack.push((w - 1, y));
        }
    }
    while let Some((x, y)) = stack.pop() {
        let i = (y * w + x) as usize;
        if seen[i] {
            continue;
        }
        seen[i] = true;
        let pixel = img.get_pixel(x, y).0;
        if !is_backdrop(pixel) {
            continue;
        }
        img.put_pixel(x, y, Rgba([0, 0, 0, 0]));
        if x > 0 {
            stack.push((x - 1, y));
        }
        if x + 1 < w {
            stack.push((x + 1, y));
        }
        if y > 0 {
            stack.push((x, y - 1));
        }
        if y + 1 < h {
            stack.push((x, y + 1));
        }
    }
}

fn is_backdrop(pixel: [u8; 4]) -> bool {
    pixel[3] > 0 && pixel[0] < BACKDROP && pixel[1] < BACKDROP && pixel[2] < BACKDROP
}

fn rgba_image(img: &RgbaImage) -> Image {
    let (w, h) = img.dimensions();
    let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(img.as_raw(), w, h);
    Image::from_rgba8(buffer)
}

pub fn set_window_icon(ui: &AppWindow) {
    if let Ok(loaded) = image::load_from_memory(dock_png()) {
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
    set_dock_icon(dock_png());
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
