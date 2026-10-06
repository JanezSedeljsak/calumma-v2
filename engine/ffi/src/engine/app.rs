use super::Inner;
use crate::surface::{CalmNativeSurface, CalmSurfaceKind};
use anyhow::Result;
use parking_lot::Mutex;
use std::ffi::c_void;
use std::path::Path;
use std::sync::Arc;

mod background;
mod camera;
mod colors;
mod guides;
mod history;
mod knobs;
mod layer_list;
mod layers;
mod ops;
mod paste;
mod pointer;
mod projects;
mod shell;
mod text;
mod tools;

pub use background::BackgroundNotice;
pub use guides::GuideInfo;
pub use layer_list::LayerSummary;
pub use projects::ProjectSummary;
pub use text::FontFamilyInfo;

pub enum NativeSurface {
    MetalLayer {
        layer: *mut c_void,
    },
    Win32Hwnd {
        hwnd: *mut c_void,
    },
    Xlib {
        display: *mut c_void,
        window: *mut c_void,
    },
    Wayland {
        display: *mut c_void,
        surface: *mut c_void,
    },
}

impl NativeSurface {
    fn to_calm(&self) -> CalmNativeSurface {
        match self {
            Self::MetalLayer { layer } => CalmNativeSurface {
                kind: CalmSurfaceKind::MetalLayer as u32,
                display: std::ptr::null_mut(),
                window: *layer,
            },
            Self::Win32Hwnd { hwnd } => CalmNativeSurface {
                kind: CalmSurfaceKind::Win32Hwnd as u32,
                display: std::ptr::null_mut(),
                window: *hwnd,
            },
            Self::Xlib { display, window } => CalmNativeSurface {
                kind: CalmSurfaceKind::Xlib as u32,
                display: *display,
                window: *window,
            },
            Self::Wayland { display, surface } => CalmNativeSurface {
                kind: CalmSurfaceKind::Wayland as u32,
                display: *display,
                window: *surface,
            },
        }
    }
}

pub struct Engine {
    pub(crate) inner: Arc<Mutex<Inner>>,
}

impl Drop for Engine {
    fn drop(&mut self) {
        let thread = self.inner.lock().autosave_thread.take();
        if let Some(thread) = thread {
            thread.stop();
        }
        let mut inner = self.inner.lock();
        if let Some(mut doc) = inner.doc.take() {
            if let Err(err) = inner.store.save(&mut doc) {
                eprintln!("miw: saving project {} on quit failed: {err}", doc.id);
            }
        }
    }
}

impl Engine {
    pub fn new(db_path: Option<&Path>) -> Result<Self> {
        let path = db_path.map(|p| p.to_string_lossy().into_owned());
        let inner = Arc::new(Mutex::new(
            Inner::new(path.as_deref()).map_err(|e| anyhow::anyhow!(e))?,
        ));
        let thread = crate::autosave::spawn(Arc::downgrade(&inner));
        inner.lock().autosave_thread = Some(thread);
        Ok(Self { inner })
    }

    pub fn attach(
        &mut self,
        surface: NativeSurface,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<()> {
        let calm = surface.to_calm();
        unsafe { self.inner.lock().attach(&calm, width, height, scale) }
    }

    pub fn resize(&mut self, width: u32, height: u32, scale: f32) {
        let mut inner = self.inner.lock();
        inner.remember_viewport(width as f32, height as f32, scale);
        let mut camera_changed = false;
        if let Some(doc) = &mut inner.doc {
            let before = doc.camera;
            doc.resize_viewport(width as f32, height as f32, scale);
            camera_changed = doc.camera != before;
        }
        if camera_changed {
            inner.invalidate_camera();
        }
        if let Some(renderer) = &mut inner.renderer {
            let dpr = scale.max(1.0);
            renderer.resize(
                ((width as f32 * dpr).round() as u32).max(1),
                ((height as f32 * dpr).round() as u32).max(1),
            );
        }
    }

    pub fn render(&mut self) {
        self.inner.lock().render_frame();
    }

    pub fn frame_hint(&self) -> u32 {
        let inner = self.inner.lock();
        match (inner.renderer.as_ref(), inner.doc.as_ref()) {
            (Some(renderer), Some(doc)) => renderer.frame_hint(doc),
            _ => calumma_core::limits::FRAME_HINT_DISPLAY_MAX,
        }
    }

    pub fn flush_save(&mut self) {
        let mut inner = self.inner.lock();
        if let Some(mut doc) = inner.doc.take() {
            match inner.store.save(&mut doc) {
                Ok(()) => inner.dirty_save = false,
                Err(err) => eprintln!("miw: saving project {} failed: {err}", doc.id),
            }
            inner.doc = Some(doc);
        }
    }

    pub fn resident_memory_bytes(&self) -> u64 {
        self.inner.lock().resident_memory_bytes()
    }
}
