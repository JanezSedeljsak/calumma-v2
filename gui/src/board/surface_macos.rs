use anyhow::{Context, Result};
use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2::MainThreadOnly;
use objc2_app_kit::{NSAutoresizingMaskOptions, NSView, NSWindowOrderingMode};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use objc2_quartz_core::CAMetalLayer;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::ffi::c_void;
use std::ptr::NonNull;

pub struct BoardSurface {
    view: Retained<NSView>,
    layer: Retained<CAMetalLayer>,
    scale: f64,
}

impl BoardSurface {
    pub fn install(winit_window: &winit::window::Window) -> Result<Self> {
        let handle = winit_window
            .window_handle()
            .context("the Slint window has no native handle yet")?;
        let ns_view: Retained<NSView> = match handle.as_raw() {
            RawWindowHandle::AppKit(appkit) => unsafe {
                Retained::retain(appkit.ns_view.as_ptr().cast()).context("retaining NSView")?
            },
            other => anyhow::bail!("expected an AppKit window handle, got {other:?}"),
        };
        let scale = winit_window.scale_factor();
        let mtm = MainThreadMarker::new()
            .context("the board surface must be installed on the main thread")?;
        let board = unsafe { NSView::initWithFrame(NSView::alloc(mtm), NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1.0, 1.0))) };
        unsafe {
            board.setAutoresizingMask(NSAutoresizingMaskOptions::empty());
            board.setHidden(true);
            ns_view.addSubview_positioned_relativeTo(&board, NSWindowOrderingMode::Below, None);
        }
        board.setWantsLayer(true);
        let layer = unsafe { CAMetalLayer::layer() };
        unsafe {
            board.setLayer(Some(&layer));
            layer.setContentsScale(scale);
            layer.setDrawableSize(NSSize::new(scale, scale));
        }
        Ok(Self {
            view: board,
            layer,
            scale,
        })
    }

    pub fn set_frame(&self, x: f64, y_top: f64, width: f64, height: f64, window_height: f64) {
        if width < 1.0 || height < 1.0 {
            self.view.setHidden(true);
            return;
        }
        let y = window_height - y_top - height;
        let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(width, height));
        self.view.setFrame(frame);
        self.view.setHidden(false);
        self.layer.setDrawableSize(NSSize::new(width * self.scale, height * self.scale));
    }

    pub fn set_hidden(&self, hidden: bool) {
        self.view.setHidden(hidden);
    }

    pub fn scale(&self) -> f64 {
        self.scale
    }

    pub fn layer_ptr(&self) -> *mut c_void {
        NonNull::from(&*self.layer).as_ptr().cast()
    }
}
