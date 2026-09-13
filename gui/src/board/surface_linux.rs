use super::layout::BoardRect;
use anyhow::{Context, Result};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use std::os::raw::c_int;
use std::ptr;
use x11_dl::xfixes::{Xlib as XFixes, XserverRegion};
use x11_dl::xlib::{self, Display, Window, XRectangle, Xlib};

const SHAPE_BOUNDING: c_int = 0;
const SHAPE_INPUT: c_int = 2;

fn px(value: f64, scale: f64) -> i32 {
    (value * scale).round() as i32
}

pub struct BoardSurface {
    xlib: Xlib,
    xfixes: XFixes,
    display: *mut Display,
    window: Window,
    scale: f64,
}

impl BoardSurface {
    pub fn install(winit_window: &winit::window::Window) -> Result<Self> {
        let display_handle = winit_window
            .display_handle()
            .context("the Slint window has no display handle yet")?;
        let window_handle = winit_window
            .window_handle()
            .context("the Slint window has no native handle yet")?;
        let (display, parent) = match (display_handle.as_raw(), window_handle.as_raw()) {
            (RawDisplayHandle::Xlib(display), RawWindowHandle::Xlib(window)) => {
                let Some(ptr) = display.display else {
                    anyhow::bail!("X11 display handle was null");
                };
                (ptr.as_ptr().cast::<Display>(), window.window as Window)
            }
            (RawDisplayHandle::Wayland(_), _) | (_, RawWindowHandle::Wayland(_)) => {
                anyhow::bail!("Linux board embed needs the X11 backend")
            }
            other => anyhow::bail!("expected an X11 window handle, got {other:?}"),
        };
        let xlib = Xlib::open().context("loading libX11")?;
        let xfixes = XFixes::open().context("loading libXfixes")?;
        let window = unsafe { (xlib.XCreateSimpleWindow)(display, parent, 0, 0, 1, 1, 0, 0, 0) };
        if window == 0 {
            anyhow::bail!("XCreateSimpleWindow returned None");
        }
        unsafe {
            (xlib.XSelectInput)(display, window, 0);
            let empty = (xfixes.XFixesCreateRegion)(display, ptr::null_mut(), 0);
            (xfixes.XFixesSetWindowShapeRegion)(display, window, SHAPE_INPUT, 0, 0, empty);
            (xfixes.XFixesDestroyRegion)(display, empty);
            (xlib.XFlush)(display);
        }
        Ok(Self {
            xlib,
            xfixes,
            display,
            window,
            scale: winit_window.scale_factor(),
        })
    }

    pub fn set_frame(&self, x: f64, y_top: f64, width: f64, height: f64, _content_height: f64) {
        if width < 1.0 || height < 1.0 {
            self.set_hidden(true);
            return;
        }
        let scale = self.scale;
        unsafe {
            (self.xlib.XMoveResizeWindow)(
                self.display,
                self.window,
                px(x, scale),
                px(y_top, scale),
                px(width, scale).max(1) as u32,
                px(height, scale).max(1) as u32,
            );
            (self.xlib.XMapWindow)(self.display, self.window);
            (self.xlib.XFlush)(self.display);
        }
    }

    pub fn set_hidden(&self, hidden: bool) {
        unsafe {
            if hidden {
                (self.xlib.XUnmapWindow)(self.display, self.window);
            } else {
                (self.xlib.XMapWindow)(self.display, self.window);
            }
            (self.xlib.XFlush)(self.display);
        }
    }

    pub fn set_holes(&self, holes: &[BoardRect]) {
        let mut attrs: xlib::XWindowAttributes = unsafe { std::mem::zeroed() };
        let status =
            unsafe { (self.xlib.XGetWindowAttributes)(self.display, self.window, &mut attrs) };
        if status == 0 || attrs.width <= 0 || attrs.height <= 0 {
            return;
        }
        let mut full = XRectangle {
            x: 0,
            y: 0,
            width: attrs.width as u16,
            height: attrs.height as u16,
        };
        unsafe {
            let bounding = (self.xfixes.XFixesCreateRegion)(self.display, &mut full, 1);
            let scale = self.scale;
            for hole in holes {
                let mut rect = XRectangle {
                    x: px(hole.x as f64, scale) as i16,
                    y: px(hole.y as f64, scale) as i16,
                    width: px(hole.width as f64, scale).max(1) as u16,
                    height: px(hole.height as f64, scale).max(1) as u16,
                };
                let punch = (self.xfixes.XFixesCreateRegion)(self.display, &mut rect, 1);
                (self.xfixes.XFixesSubtractRegion)(self.display, bounding, bounding, punch);
                (self.xfixes.XFixesDestroyRegion)(self.display, punch);
            }
            (self.xfixes.XFixesSetWindowShapeRegion)(
                self.display,
                self.window,
                SHAPE_BOUNDING,
                0,
                0,
                bounding,
            );
            (self.xfixes.XFixesDestroyRegion)(self.display, bounding);
            let empty: XserverRegion =
                (self.xfixes.XFixesCreateRegion)(self.display, ptr::null_mut(), 0);
            (self.xfixes.XFixesSetWindowShapeRegion)(
                self.display,
                self.window,
                SHAPE_INPUT,
                0,
                0,
                empty,
            );
            (self.xfixes.XFixesDestroyRegion)(self.display, empty);
            (self.xlib.XFlush)(self.display);
        }
    }

    pub fn scale(&self) -> f64 {
        self.scale
    }

    pub fn native(&self) -> calumma_app::NativeSurface {
        calumma_app::NativeSurface::Xlib {
            display: self.display.cast(),
            window: self.window as *mut std::ffi::c_void,
        }
    }
}

impl Drop for BoardSurface {
    fn drop(&mut self) {
        unsafe {
            (self.xlib.XDestroyWindow)(self.display, self.window);
            (self.xlib.XFlush)(self.display);
        }
    }
}
