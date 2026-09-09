use super::layout::BoardRect;
use anyhow::{Context, Result};
use objc2::rc::Retained;
use objc2::{define_class, msg_send, ClassType, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSAutoresizingMaskOptions, NSBezierPath, NSView, NSWindowOrderingMode};
use objc2_core_graphics::CGColor;
use objc2_foundation::{NSPoint, NSRect, NSSize};
use objc2_quartz_core::{kCAFillRuleEvenOdd, CAMetalLayer, CAShapeLayer};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::ffi::c_void;
use std::ptr::NonNull;

define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "MiwBoardView"]
    pub struct BoardView;

    impl BoardView {
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> *mut NSView {
            std::ptr::null_mut()
        }
    }
);

pub struct BoardSurface {
    view: Retained<BoardView>,
    layer: Retained<CAMetalLayer>,
    scale: f64,
}

impl BoardSurface {
    pub fn install(winit_window: &winit::window::Window) -> Result<Self> {
        let handle = winit_window
            .window_handle()
            .context("the Slint window has no native handle yet")?;
        let ns_view: Retained<NSView> = match handle.as_raw() {
            RawWindowHandle::AppKit(appkit) => {
                unsafe { Retained::retain(appkit.ns_view.as_ptr().cast()) }
                    .context("retaining NSView")?
            }
            other => anyhow::bail!("expected an AppKit window handle, got {other:?}"),
        };
        let scale = winit_window.scale_factor();
        let mtm = MainThreadMarker::new()
            .context("the board surface must be installed on the main thread")?;
        let board: Retained<BoardView> = unsafe {
            objc2::msg_send![
                BoardView::alloc(mtm),
                initWithFrame: NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1.0, 1.0))
            ]
        };
        board.setAutoresizingMask(NSAutoresizingMaskOptions::empty());
        board.setHidden(true);
        ns_view.addSubview_positioned_relativeTo(&board, NSWindowOrderingMode::Below, None);
        board.setWantsLayer(true);
        let layer = CAMetalLayer::layer();
        board.setLayer(Some(&layer));
        layer.setContentsScale(scale);
        layer.setDrawableSize(NSSize::new(scale, scale));
        Ok(Self {
            view: board,
            layer,
            scale,
        })
    }

    pub fn set_frame(&self, x: f64, y_top: f64, width: f64, height: f64, content_height: f64) {
        if width < 1.0 || height < 1.0 {
            self.view.setHidden(true);
            return;
        }
        let host = unsafe { self.view.superview() };
        let flipped = host.as_ref().is_some_and(|view| view.isFlipped());
        let y = if flipped {
            y_top
        } else {
            content_height - y_top - height
        };
        let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(width, height));
        self.view.setFrame(frame);
        self.view.setHidden(false);
        self.layer
            .setDrawableSize(NSSize::new(width * self.scale, height * self.scale));
    }

    pub fn set_hidden(&self, hidden: bool) {
        self.view.setHidden(hidden);
    }

    pub fn set_holes(&self, holes: &[BoardRect]) {
        if holes.is_empty() {
            unsafe {
                self.layer.setMask(None);
            }
            return;
        }
        let bounds = self.view.bounds();
        let path = NSBezierPath::bezierPathWithRect(bounds);
        let flipped = self.view.isFlipped();
        for hole in holes {
            let y = if flipped {
                hole.y as f64
            } else {
                bounds.size.height - hole.y as f64 - hole.height as f64
            };
            let rect = NSRect::new(
                NSPoint::new(hole.x as f64, y),
                NSSize::new(hole.width as f64, hole.height as f64),
            );
            let radius = hole.radius as f64;
            if radius > 0.0 {
                path.appendBezierPathWithRoundedRect_xRadius_yRadius(rect, radius, radius);
            } else {
                path.appendBezierPathWithRect(rect);
            }
        }
        let mask: Retained<CAShapeLayer> = unsafe { msg_send![CAShapeLayer::class(), layer] };
        mask.setFrame(bounds);
        unsafe {
            mask.setFillRule(kCAFillRuleEvenOdd);
        }
        let white = CGColor::new_generic_gray(1.0, 1.0);
        mask.setFillColor(Some(&white));
        mask.setPath(Some(&path.CGPath()));
        unsafe {
            self.layer.setMask(Some(mask.as_super()));
        }
    }

    pub fn scale(&self) -> f64 {
        self.scale
    }

    pub fn layer_ptr(&self) -> *mut c_void {
        NonNull::from(&*self.layer).as_ptr().cast()
    }
}
