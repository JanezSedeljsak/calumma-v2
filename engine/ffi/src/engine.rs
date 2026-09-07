use crate::active_renderer::ActiveRenderer;
use crate::platform::CalmPlatformOps;
use crate::surface::CalmNativeSurface;
use anyhow::bail;
#[cfg(not(test))]
use anyhow::{anyhow, Context};
use calumma_core::limits::AUTOSAVE_INTERVAL_MS;
use calumma_core::{Document, Tool};
use calumma_io::ProjectStore;
use calumma_ops::{OpRegistry, SeamCarveOp, SmartMatteOp, UpscaleOp};
use parking_lot::Mutex;
use std::time::{Duration, Instant};

pub(crate) struct Inner {
    pub(crate) doc: Option<Document>,
    pub(crate) store: ProjectStore,
    renderer: Option<ActiveRenderer>,
    instance: wgpu::Instance,
    last_save: Instant,
    dirty_save: bool,
    autosave_thread: Option<crate::autosave::AutosaveThread>,
    registry: OpRegistry,
    platform_ops: Option<CalmPlatformOps>,
    last_shape_tool: Tool,
    last_select_tool: Tool,
    viewport_width: f32,
    viewport_height: f32,
    viewport_dpr: f32,
    pending_pan_dx: f32,
    pending_pan_dy: f32,
    pending_scroll_dx: f32,
    pending_scroll_dy: f32,
    pending_scroll_precise: bool,
}

fn _assert_inner_send() {
    fn assert_send<T: Send>() {}
    assert_send::<Mutex<Inner>>();
}

impl Inner {
    pub(crate) fn render_frame(&mut self) {
        self.flush_pending_camera();
        if self.doc.is_none() {
            return;
        }
        if let Some(renderer) = &mut self.renderer {
            if let Some(doc) = &mut self.doc {
                renderer.render(doc);
            }
        }
    }

    pub(crate) fn new(db_path: Option<&str>) -> Result<Self, String> {
        let path = match db_path {
            Some(p) if !p.is_empty() => std::path::PathBuf::from(p),
            _ => ProjectStore::default_path(),
        };
        let store = ProjectStore::open(path).map_err(|e| e.to_string())?;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: crate::surface::preferred_backends(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        Ok(Self {
            doc: None,
            store,
            renderer: None,
            instance,
            last_save: Instant::now() - Duration::from_secs(60),
            dirty_save: false,
            autosave_thread: None,
            registry: base_op_registry(),
            platform_ops: None,
            last_shape_tool: Tool::Rect,
            last_select_tool: Tool::SelectRect,
            viewport_width: 0.0,
            viewport_height: 0.0,
            viewport_dpr: 1.0,
            pending_pan_dx: 0.0,
            pending_pan_dy: 0.0,
            pending_scroll_dx: 0.0,
            pending_scroll_dy: 0.0,
            pending_scroll_precise: false,
        })
    }

    fn remember_viewport(&mut self, width: f32, height: f32, dpr: f32) {
        self.viewport_width = width.max(1.0);
        self.viewport_height = height.max(1.0);
        self.viewport_dpr = dpr.max(1.0);
    }

    fn apply_stored_viewport(&self, doc: &mut Document) {
        if self.viewport_width > 0.0 && self.viewport_height > 0.0 {
            doc.resize_viewport(self.viewport_width, self.viewport_height, self.viewport_dpr);
        }
    }

    pub(crate) fn invalidate_renderer(&mut self) {
        if let Some(r) = &mut self.renderer {
            r.invalidate();
        }
    }

    /// For chrome that moved and nothing else: redraw the overlays, keep every cache. Moving a
    /// guide is the case this exists for — the tiles, the layer stack and the camera are all
    /// exactly as they were, and `write_guides` rebuilds the guide buffer every frame anyway.
    pub(crate) fn invalidate_overlay(&mut self) {
        if let Some(r) = &mut self.renderer {
            r.invalidate_overlay();
        }
    }

    pub(crate) fn invalidate_camera(&mut self) {
        if let Some(r) = &mut self.renderer {
            r.invalidate_camera();
        }
    }

    fn flush_pending_camera(&mut self) {
        let Some(doc) = &mut self.doc else {
            self.pending_pan_dx = 0.0;
            self.pending_pan_dy = 0.0;
            self.pending_scroll_dx = 0.0;
            self.pending_scroll_dy = 0.0;
            return;
        };
        let (w, h) = (doc.width as f32, doc.height as f32);
        if self.pending_pan_dx != 0.0 || self.pending_pan_dy != 0.0 {
            doc.camera
                .pan_by(self.pending_pan_dx, self.pending_pan_dy, w, h);
            self.pending_pan_dx = 0.0;
            self.pending_pan_dy = 0.0;
        }
        if self.pending_scroll_dx != 0.0 || self.pending_scroll_dy != 0.0 {
            doc.camera.pan_by_scroll(
                self.pending_scroll_dx,
                self.pending_scroll_dy,
                self.pending_scroll_precise,
                w,
                h,
            );
            self.pending_scroll_dx = 0.0;
            self.pending_scroll_dy = 0.0;
            self.pending_scroll_precise = false;
        }
    }

    /// Save and let go. One document is resident at a time, and the moment it stops being
    /// the open one, everything it cost — tiles, history, and the GPU textures the renderer
    /// cached for it — goes back: nothing of a project you are not working on stays in
    /// memory.
    pub(crate) fn close_document(&mut self) {
        if let Some(mut doc) = self.doc.take() {
            self.last_shape_tool = doc.last_shape_tool;
            self.last_select_tool = doc.last_select_tool;
            if doc.camera.viewport_width > 0.0 && doc.camera.viewport_height > 0.0 {
                self.remember_viewport(
                    doc.camera.viewport_width,
                    doc.camera.viewport_height,
                    doc.camera.dpr,
                );
            }
            let _ = self.store.save(&mut doc);
        }
        self.release_gpu_resources();
    }

    pub(crate) fn install_document(&mut self, mut doc: Document) {
        doc.last_shape_tool = self.last_shape_tool;
        doc.last_select_tool = self.last_select_tool;
        self.apply_stored_viewport(&mut doc);
        if doc.camera.viewport_width > 0.0 {
            doc.fit_to_view();
        }
        self.doc = Some(doc);
        self.dirty_save = false;
        self.invalidate_renderer();
        if let Some(r) = &mut self.renderer {
            r.request_overview_prewarm();
        }
    }

    pub(crate) fn release_gpu_resources(&mut self) {
        if let Some(r) = &mut self.renderer {
            r.release_document();
        }
    }

    pub(crate) fn resident_memory_bytes(&self) -> u64 {
        use calumma_core::memory::document_memory;
        let doc_bytes = self
            .doc
            .as_ref()
            .map(|doc| document_memory(doc).total() as u64)
            .unwrap_or(0);
        let gpu = self.renderer.as_ref().map_or(0, |r| r.gpu_tile_bytes()) as u64;
        doc_bytes + gpu
    }

    pub(crate) fn autosave(&mut self) {
        if self.doc.as_ref().is_some_and(|d| d.stroke_active) {
            return;
        }
        // The autosave tick is this codebase's "the user is idle enough for background work"
        // signal, and it already runs off the render thread — so it is also where the undo
        // stack shrinks its cold tiles. It runs whether or not there is anything to save: a
        // document being traversed with undo/redo dirties no bytes but still goes cold.
        if let Some(doc) = self.doc.as_mut() {
            doc.history.compact_cold();
        }
        if !self.dirty_save {
            return;
        }
        if self.last_save.elapsed() < Duration::from_millis(AUTOSAVE_INTERVAL_MS) {
            return;
        }
        let Inner {
            doc,
            store,
            last_save,
            dirty_save,
            ..
        } = self;
        let Some(doc) = doc.as_mut() else {
            return;
        };
        if store.save(doc).is_ok() {
            *dirty_save = false;
            *last_save = Instant::now();
        }
    }

    /// # Safety
    ///
    /// `surface`'s handles must be live and of the kind it claims — see
    /// [`crate::surface::surface_target`].
    pub(crate) unsafe fn attach(
        &mut self,
        surface: &CalmNativeSurface,
        w: u32,
        h: u32,
        scale: f32,
    ) -> anyhow::Result<()> {
        if surface.window.is_null() || w == 0 || h == 0 {
            bail!("attach needs a non-null window and a non-empty size, got {w}x{h}");
        }
        let (pw, ph) = {
            let dpr = scale.max(1.0);
            (
                ((w as f32 * dpr).round() as u32).max(1),
                ((h as f32 * dpr).round() as u32).max(1),
            )
        };
        #[cfg(test)]
        {
            let _ = (surface, &self.instance, pw, ph);
            self.renderer = Some(ActiveRenderer::Stub);
        }
        #[cfg(not(test))]
        {
            let target = unsafe { crate::surface::surface_target(surface) }?;
            let surface = unsafe { self.instance.create_surface_unsafe(target) }
                .context("creating a wgpu surface for the shell's window")?;
            let renderer = calumma_render::Renderer::from_surface(surface, &self.instance, pw, ph)
                .map_err(|e| anyhow!("creating the renderer: {e}"))?;
            self.renderer = Some(ActiveRenderer::Gpu(renderer));
        }
        self.remember_viewport(w as f32, h as f32, scale);
        if let Some(doc) = &mut self.doc {
            doc.resize_viewport(w as f32, h as f32, scale);
            doc.fit_to_view();
        }
        Ok(())
    }
}

pub(crate) fn base_op_registry() -> OpRegistry {
    let mut registry = OpRegistry::new();
    registry.register_core(Box::new(UpscaleOp));
    registry.register_core(Box::new(SeamCarveOp));
    registry.register_core(Box::new(SmartMatteOp));
    registry
}

mod app;
pub use app::{Engine, LayerSummary, NativeSurface, ProjectSummary};

#[cfg(test)]
mod stub_renderer_tests {
    use super::app::{Engine, NativeSurface};
    use calumma_core::Tool;
    use std::ffi::c_void;

    #[test]
    fn stub_attach_drives_resize_render_and_invalidate() {
        let dir = tempfile::tempdir().unwrap();
        let mut engine = Engine::new(Some(dir.path().join("t.sqlite").as_path())).expect("engine");
        engine
            .create_project("StubGpu", 64, 64)
            .expect("create project");
        let layer = 0x1 as *mut c_void;
        engine
            .attach(NativeSurface::MetalLayer { layer }, 128, 96, 2.0)
            .expect("attach");
        engine.resize(160, 120, 1.0);
        engine.render();
        engine.set_tool(Tool::Pen);
        engine.pointer_down(10.0, 10.0);
        engine.pointer_move(20.0, 20.0);
        engine.pointer_up(20.0, 20.0);
        engine.pan(2.0, 2.0);
        engine.zoom(30.0, 30.0, 1.1);
        engine.fit_to_view();
        engine.render();
    }
}
