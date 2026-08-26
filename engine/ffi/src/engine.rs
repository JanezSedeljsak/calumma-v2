use crate::active_renderer::ActiveRenderer;
use crate::platform::{parse_op_kind, CalmPlatformOps, PlatformOp};
use anyhow::{anyhow, bail, Context};
use calumma_core::limits::{AUTOSAVE_INTERVAL_MS, BRUSH_SIZE_MAX, BRUSH_SIZE_MIN, IMPORT_MAX_SIDE};
use calumma_core::{
    pack_rgba, project_color, unpack_rgba, unpremultiply_rgba, AdjustmentKind, Adjustments,
    BlendMode, BoardColors, Brush, Document, Tool, PROJECT_COLORS,
};
use calumma_io::{encode_pdf, encode_psd, encode_svg, ProjectListItem, ProjectStore};
use calumma_ops::{
    apply_output, layer_input, run_op_on_document, Backend, Op, OpParams, OpRegistry,
};
use parking_lot::Mutex;
use std::ffi::{c_char, c_void, CStr, CString};
use std::os::raw::c_int;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::time::{Duration, Instant};

#[repr(C)]
pub struct CalmEngine {
    _private: [u8; 0],
}

#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalmStatus {
    Ok = 0,
    Error = 1,
    Null = 2,
}

#[repr(C)]
pub struct CalmState {
    pub width: u32,
    pub height: u32,
    pub zoom: f32,
    pub min_zoom: f32,
    pub max_zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub active_layer: u32,
    pub layer_count: u32,
    pub can_undo: u8,
    pub can_redo: u8,
    pub stroke_active: u8,
    pub dark_theme: u8,
    pub accent: u32,
    pub zoom_unit: f32,
    pub last_shape_tool: u32,
    pub last_select_tool: u32,
    pub is_fit: u8,
    /// Whether the active layer is inside `⌘T`. The shell mirrors it on the Transform tool
    /// button, which is a mode the engine owns rather than a tool the shell selects.
    pub transform_active: u8,
}

#[repr(C)]
pub struct CalmLayerInfo {
    pub index: u32,
    pub visible: u8,
    pub name: *mut c_char,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CalmRulerTick {
    pub doc: f32,
    pub major: u8,
}

#[repr(C)]
pub struct CalmProjectInfo {
    pub id: *mut c_char,
    pub name: *mut c_char,
    pub width: u32,
    pub height: u32,
    pub opened_at: i64,
    pub accent: u32,
}

pub(crate) use calumma_core::{pack_rgb, unpack_rgb};

pub(crate) fn write_boxed(bytes: Vec<u8>, out: *mut *mut u8, out_len: *mut usize) {
    let mut boxed = bytes.into_boxed_slice();
    let len = boxed.len();
    let ptr = boxed.as_mut_ptr();
    std::mem::forget(boxed);
    unsafe {
        *out = ptr;
        *out_len = len;
    }
}

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
    fn new(db_path: Option<&str>) -> Result<Self, String> {
        let path = match db_path {
            Some(p) if !p.is_empty() => std::path::PathBuf::from(p),
            _ => ProjectStore::default_path(),
        };
        let store = ProjectStore::open(path).map_err(|e| e.to_string())?;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::METAL,
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
            registry: OpRegistry::new(),
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

    pub(crate) fn mark_dirty_save(&mut self) {
        self.dirty_save = true;
    }

    pub(crate) fn invalidate_renderer(&mut self) {
        if let Some(r) = &mut self.renderer {
            r.invalidate();
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

    /// What every document mutation owes the rest of the app: the next frame is stale and so
    /// is the file on disk.
    pub(crate) fn edited(&mut self) {
        self.mark_dirty_save();
        self.invalidate_renderer();
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

    pub(crate) fn switch_workspace(
        &mut self,
        workspace_id: &str,
        project_id: Option<&str>,
    ) -> anyhow::Result<()> {
        self.close_document();
        if let Some(id) = project_id {
            let doc = self
                .store
                .open_project(id)
                .with_context(|| format!("opening project {id}"))?;
            self.install_document(doc);
        }
        self.store
            .set_workspace_active_project(workspace_id, project_id)
            .context("setting active project")?;
        self.store
            .touch_workspace(workspace_id)
            .context("touching workspace")?;
        Ok(())
    }

    pub(crate) fn release_gpu_resources(&mut self) {
        if let Some(r) = &mut self.renderer {
            r.release_document();
        }
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
}

/// The C ABI can only carry a `CalmStatus`, so nothing downstream ever matches on an error
/// variant — every failure in here is "it broke, and here is why". That is exactly what
/// `anyhow` is for: `?` composes `rusqlite`, `io`, `StoreError` and friends without an enum
/// per module, and the context chain survives all the way up to this one place.
fn report(err: &anyhow::Error) {
    // No logging framework in the engine yet; in debug builds the chain at least reaches
    // the Xcode console instead of being discarded at the ABI boundary.
    #[cfg(debug_assertions)]
    eprintln!("calumma-ffi: {err:#}");
    #[cfg(not(debug_assertions))]
    let _ = err;
}

pub(crate) fn with_inner<F>(engine: *mut CalmEngine, f: F) -> CalmStatus
where
    F: FnOnce(&mut Inner) -> anyhow::Result<()> + std::panic::UnwindSafe,
{
    if engine.is_null() {
        return CalmStatus::Null;
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let mut inner = mutex.lock();
        f(&mut inner)
    })) {
        Ok(Ok(())) => CalmStatus::Ok,
        Ok(Err(err)) => {
            report(&err);
            CalmStatus::Error
        }
        Err(_) => {
            report(&anyhow!("panic caught at the FFI boundary"));
            CalmStatus::Error
        }
    }
}

/// Read-only counterpart to `with_inner`, for the getters whose C signature carries a value
/// rather than a `CalmStatus`: locks, reads the document, and answers `fallback` for every
/// failure the ABI cannot describe (null engine, no project, poisoned lock, panic).
pub(crate) fn read_doc<T>(
    engine: *mut CalmEngine,
    fallback: T,
    f: impl FnOnce(&Document) -> T,
) -> T {
    if engine.is_null() {
        return fallback;
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock();
        let doc = inner.doc.as_ref()?;
        Some(f(doc))
    })) {
        Ok(Some(value)) => value,
        _ => fallback,
    }
}

/// GPU-side bytes, read under the same lock as everything else. Separate from `read_doc`
/// because the renderer outlives any one document — it answers even when nothing is open.
pub(crate) fn renderer_gpu_bytes(engine: *mut CalmEngine) -> usize {
    if engine.is_null() {
        return 0;
    }
    catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock();
        Some(inner.renderer.as_ref().map_or(0, |r| r.gpu_tile_bytes()))
    }))
    .ok()
    .flatten()
    .unwrap_or(0)
}

pub(crate) fn cstring(s: &str) -> *mut c_char {
    CString::new(s)
        .map(|c| c.into_raw())
        .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn calm_string_free(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
        drop(CString::from_raw(s));
    }));
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_new(db_path: *const c_char) -> *mut CalmEngine {
    let path = if db_path.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(db_path) }.to_str().ok()
    };
    match catch_unwind(AssertUnwindSafe(|| Inner::new(path))) {
        Ok(Ok(inner)) => {
            let ptr = Box::into_raw(Box::new(Mutex::new(inner)));
            let thread = crate::autosave::spawn(ptr as *const Mutex<Inner>);
            unsafe { (*ptr).lock().autosave_thread = Some(thread) };
            ptr as *mut CalmEngine
        }
        _ => ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_free(engine: *mut CalmEngine) {
    if engine.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { Box::from_raw(engine as *mut Mutex<Inner>) };
        let thread = mutex.lock().autosave_thread.take();
        if let Some(thread) = thread {
            thread.stop();
        }
        {
            let mut inner = mutex.lock();
            if let Some(mut doc) = inner.doc.take() {
                let _ = inner.store.save(&mut doc);
            }
        }
        drop(mutex);
    }));
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_attach_surface(
    engine: *mut CalmEngine,
    layer: *mut c_void,
    w: u32,
    h: u32,
    scale: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if layer.is_null() || w == 0 || h == 0 {
            bail!("attach needs a non-null layer and a non-empty size, got {w}x{h}");
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
            let _ = (layer, &inner.instance, pw, ph);
            inner.renderer = Some(ActiveRenderer::Stub);
        }
        #[cfg(not(test))]
        {
            let surface = unsafe {
                inner
                    .instance
                    .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(layer))
                    .context("creating a wgpu surface for the CoreAnimation layer")?
            };
            let renderer = calumma_render::Renderer::from_surface(surface, &inner.instance, pw, ph)
                .map_err(|e| anyhow!("creating the Metal renderer: {e}"))?;
            inner.renderer = Some(ActiveRenderer::Gpu(renderer));
        }
        inner.remember_viewport(w as f32, h as f32, scale);
        if let Some(doc) = &mut inner.doc {
            doc.resize_viewport(w as f32, h as f32, scale);
            doc.fit_to_view();
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_resize(
    engine: *mut CalmEngine,
    w: u32,
    h: u32,
    scale: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        inner.remember_viewport(w as f32, h as f32, scale);
        if let Some(doc) = &mut inner.doc {
            doc.resize_viewport(w as f32, h as f32, scale);
        }
        if let Some(renderer) = &mut inner.renderer {
            let dpr = scale.max(1.0);
            renderer.resize(
                ((w as f32 * dpr).round() as u32).max(1),
                ((h as f32 * dpr).round() as u32).max(1),
            );
            renderer.invalidate_camera();
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_resize_document(
    engine: *mut CalmEngine,
    width: u32,
    height: u32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        doc.resize(width, height);
        inner.dirty_save = true;
        if let Some(r) = &mut inner.renderer {
            r.invalidate();
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_render(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        inner.flush_pending_camera();
        if inner.doc.is_none() {
            return Ok(());
        }
        if let Some(renderer) = &mut inner.renderer {
            if let Some(doc) = &mut inner.doc {
                renderer.render(doc);
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_pointer_down(
    engine: *mut CalmEngine,
    x: f32,
    y: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.pointer_down(x, y);
            inner.dirty_save = true;
            if let Some(r) = &mut inner.renderer {
                r.invalidate();
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_pointer_move(
    engine: *mut CalmEngine,
    x: f32,
    y: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            let changed_content = doc.pointer_move(x, y);
            if let Some(r) = &mut inner.renderer {
                if changed_content {
                    r.invalidate();
                } else {
                    r.invalidate_overlay();
                }
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_pointer_up(
    engine: *mut CalmEngine,
    x: f32,
    y: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.pointer_up(x, y);
            inner.dirty_save = true;
            if let Some(r) = &mut inner.renderer {
                r.invalidate();
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_end_camera_motion(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(renderer) = &mut inner.renderer {
            renderer.end_camera_motion();
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_pan(engine: *mut CalmEngine, dx: f32, dy: f32) -> CalmStatus {
    with_inner(engine, |inner| {
        inner.pending_pan_dx += dx;
        inner.pending_pan_dy += dy;
        inner.invalidate_camera();
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_pan_scroll(
    engine: *mut CalmEngine,
    dx: f32,
    dy: f32,
    precise: u8,
) -> CalmStatus {
    with_inner(engine, |inner| {
        inner.pending_scroll_dx += dx;
        inner.pending_scroll_dy += dy;
        if precise != 0 {
            inner.pending_scroll_precise = true;
        }
        inner.invalidate_camera();
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_zoom_scroll(
    engine: *mut CalmEngine,
    x: f32,
    y: f32,
    delta: f32,
    precise: u8,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            let (w, h) = (doc.width as f32, doc.height as f32);
            doc.camera.zoom_by_scroll(x, y, delta, precise != 0, w, h);
            inner.invalidate_camera();
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_zoom(
    engine: *mut CalmEngine,
    x: f32,
    y: f32,
    factor: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            let next = doc.camera.zoom * factor;
            let (w, h) = (doc.width as f32, doc.height as f32);
            doc.camera.zoom_at(x, y, next, w, h);
            inner.invalidate_camera();
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_fit(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.fit_to_view();
            inner.invalidate_camera();
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_zoom(engine: *mut CalmEngine, zoom: f32) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            let (w, h) = (doc.width as f32, doc.height as f32);
            doc.camera.zoom_to_center(zoom, w, h);
            inner.invalidate_camera();
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_step_zoom(engine: *mut CalmEngine, zoom_in: u8) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            let (w, h) = (doc.width as f32, doc.height as f32);
            doc.camera.step_zoom(zoom_in != 0, w, h);
            inner.invalidate_camera();
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_zoom_unit(
    engine: *mut CalmEngine,
    unit: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            let (w, h) = (doc.width as f32, doc.height as f32);
            let zoom = doc.camera.zoom_from_unit(unit, w, h);
            doc.camera.zoom_to_center(zoom, w, h);
            inner.invalidate_camera();
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_board_colors(
    engine: *mut CalmEngine,
    desk: u32,
    grid: u32,
    paper_border: u32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            let next = BoardColors {
                desk: unpack_rgba(desk),
                grid: unpack_rgba(grid),
                paper_border: unpack_rgba(paper_border),
            };
            if doc.board_colors != next {
                doc.board_colors = next;
                if let Some(r) = &mut inner.renderer {
                    r.invalidate();
                }
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn calm_palette_count() -> u32 {
    PROJECT_COLORS.len() as u32
}

#[no_mangle]
pub extern "C" fn calm_palette_color(index: u32) -> u32 {
    pack_rgb(project_color(index as usize))
}

#[no_mangle]
pub unsafe extern "C" fn calm_project_rename(
    engine: *mut CalmEngine,
    id: *const c_char,
    name: *const c_char,
) -> CalmStatus {
    if id.is_null() || name.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let id = unsafe { CStr::from_ptr(id) }
            .to_str()
            .context("project id is not valid UTF-8")?;
        let name = unsafe { CStr::from_ptr(name) }
            .to_str()
            .context("project name is not valid UTF-8")?;
        let name = name.trim();
        if name.is_empty() {
            bail!("a project name cannot be empty");
        }
        inner.store.rename(id, name).context("renaming project")?;
        if let Some(doc) = inner.doc.as_mut() {
            if doc.id == id {
                doc.name = name.to_string();
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_project_set_accent(
    engine: *mut CalmEngine,
    id: *const c_char,
    accent: u32,
) -> CalmStatus {
    if id.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let id = unsafe { CStr::from_ptr(id) }
            .to_str()
            .context("project id is not valid UTF-8")?;
        let rgb = unpack_rgb(accent);
        inner
            .store
            .set_accent(id, rgb)
            .context("setting project accent")?;
        if let Some(doc) = inner.doc.as_mut() {
            if doc.id == id {
                doc.accent = rgb;
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_tool(engine: *mut CalmEngine, tool: u32) -> CalmStatus {
    with_inner(engine, |inner| {
        let next = Tool::from_u32(tool).with_context(|| format!("unknown tool id {tool}"))?;
        if let Some(doc) = &mut inner.doc {
            doc.set_tool(next);
            inner.last_shape_tool = doc.last_shape_tool;
            inner.last_select_tool = doc.last_select_tool;
        } else if next.is_shape() {
            inner.last_shape_tool = next;
        } else if next.is_selection() {
            inner.last_select_tool = next;
        }
        inner.invalidate_renderer();
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_color(
    engine: *mut CalmEngine,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.set_color([r, g, b, a]);
        }
        inner.invalidate_renderer();
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_sample_color(
    engine: *mut CalmEngine,
    x: f32,
    y: f32,
    out_rgba: *mut u32,
) -> CalmStatus {
    if out_rgba.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let doc = inner.doc.as_ref().context("no project is open")?;
        let (dx, dy) = doc.camera.to_doc(x, y);
        let Some(color) = doc.sample_color(dx, dy) else {
            bail!("no opaque color under the cursor");
        };
        unsafe {
            *out_rgba = pack_rgba(color);
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_pick_color(
    engine: *mut CalmEngine,
    x: f32,
    y: f32,
    out_rgba: *mut u32,
) -> CalmStatus {
    if out_rgba.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        let (dx, dy) = doc.camera.to_doc(x, y);
        let Some(color) = doc.pick_color(dx, dy) else {
            bail!("no opaque color under the cursor");
        };
        unsafe {
            *out_rgba = pack_rgba(color);
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_brush(engine: *mut CalmEngine, size: f32) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.brush_size = size.clamp(BRUSH_SIZE_MIN, BRUSH_SIZE_MAX);
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_ink_opacity(
    engine: *mut CalmEngine,
    opacity: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.set_ink_opacity(opacity);
        }
        inner.invalidate_renderer();
        Ok(())
    })
}

/// Blur brush strength. Not an ink knob — the blur has no color — so it is its own setter
/// rather than another meaning for `set_ink_opacity`.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_blur_strength(
    engine: *mut CalmEngine,
    strength: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.set_blur_strength(strength);
        }
        Ok(())
    })
}

/// Which brush the pen lays ink down with. Size stays `calm_engine_set_brush`; this is the
/// character of the ink, not how much of it there is.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_brush_kind(
    engine: *mut CalmEngine,
    brush: u32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let next = Brush::from_u32(brush).with_context(|| format!("unknown brush id {brush}"))?;
        if let Some(doc) = &mut inner.doc {
            doc.set_brush(next);
        }
        inner.invalidate_renderer();
        Ok(())
    })
}

/// How sharp the eraser's rim is. Its own knob rather than the pen's brush — grain and flow
/// describe ink going down, and this tool takes ink away.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_eraser_hardness(
    engine: *mut CalmEngine,
    hardness: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.set_eraser_hardness(hardness);
        }
        inner.invalidate_renderer();
        Ok(())
    })
}

/// Flood tolerance, shared by the bucket and the magic wand.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_tolerance(
    engine: *mut CalmEngine,
    tolerance: u8,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.set_tolerance(tolerance);
        }
        Ok(())
    })
}

/// The eyedropper's sample area, as a radius in document pixels — pixels within
/// `radius + 0.5` of the clicked centre are averaged.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_eyedropper_radius(
    engine: *mut CalmEngine,
    radius: u32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.set_eyedropper_radius(radius);
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_fill(engine: *mut CalmEngine, fill: u8) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.fill = fill != 0;
        }
        inner.invalidate_renderer();
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_stroke(engine: *mut CalmEngine, stroke: u8) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.stroke = stroke != 0;
        }
        inner.invalidate_renderer();
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_stroke_color(
    engine: *mut CalmEngine,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.stroke_color = [r, g, b, a];
        }
        inner.invalidate_renderer();
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_shape_fill_color(
    engine: *mut CalmEngine,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.shape_fill_color = [r, g, b, a];
        }
        inner.invalidate_renderer();
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_dark(engine: *mut CalmEngine, dark: u8) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.dark_theme = dark != 0;
            if let Some(r) = &mut inner.renderer {
                r.invalidate();
            }
        }
        Ok(())
    })
}

#[no_mangle]
/// Shift is a live modifier, not a knob read at commit time: a shape being dragged squares
/// off the moment it goes down and relaxes the moment it comes up, so the board has to
/// redraw on the change itself rather than waiting for the next pointer event.
pub unsafe extern "C" fn calm_engine_set_shift(engine: *mut CalmEngine, held: u8) -> CalmStatus {
    with_inner(engine, |inner| {
        let Some(doc) = &mut inner.doc else {
            return Ok(());
        };
        if doc.shift_held == (held != 0) {
            return Ok(());
        }
        doc.set_shift_held(held != 0);
        inner.invalidate_renderer();
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_reset_layer_transform(
    engine: *mut CalmEngine,
    index: u32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        doc.reset_layer_transform(index as usize);
        inner.dirty_save = true;
        if let Some(r) = &mut inner.renderer {
            r.invalidate();
        }
        Ok(())
    })
}

/// Where the pointer is when no button is down, in screen coordinates — the brush cursor is
/// the only thing that reads it. Cheap on purpose: it invalidates the overlay and nothing
/// else, so a hovering pointer costs one overlay pass a frame and never touches a tile.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_pointer_hover(
    engine: *mut CalmEngine,
    x: f32,
    y: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.set_pointer_hover(x, y);
            if let Some(r) = &mut inner.renderer {
                r.invalidate_overlay();
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_clear_pointer_hover(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.clear_pointer_hover();
            if let Some(r) = &mut inner.renderer {
                r.invalidate_overlay();
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_toggle_transform(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        doc.toggle_transform();
        if let Some(r) = &mut inner.renderer {
            r.invalidate();
        }
        Ok(())
    })
}

/// Enters `⌘T` on the active layer, unlike `calm_engine_toggle_transform`, which would leave
/// it again if the caller happened to already be in it. Pasting needs the idempotent one: an
/// image dropped onto a board that is already transforming still has to end up transforming
/// the layer that just arrived.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_enter_transform(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        doc.enter_transform();
        if let Some(r) = &mut inner.renderer {
            r.invalidate();
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_exit_transform(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.exit_transform();
            if let Some(r) = &mut inner.renderer {
                r.invalidate();
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_undo(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.undo();
            inner.dirty_save = true;
            if let Some(r) = &mut inner.renderer {
                r.invalidate();
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_redo(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.redo();
            inner.dirty_save = true;
            if let Some(r) = &mut inner.renderer {
                r.invalidate();
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_add_layer(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            let n = doc.layers.iter().filter(|l| l.content.is_raster()).count() + 1;
            doc.add_layer(calumma_core::names::numbered_layer(n));
            inner.dirty_save = true;
            if let Some(r) = &mut inner.renderer {
                r.invalidate();
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_remove_layer(
    engine: *mut CalmEngine,
    index: u32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            if !doc.remove_layer(index as usize) {
                bail!("layer {index} cannot be removed");
            }
            inner.dirty_save = true;
            if let Some(r) = &mut inner.renderer {
                r.invalidate();
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_layer_visible(
    engine: *mut CalmEngine,
    index: u32,
    visible: u8,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.set_layer_visible(index as usize, visible != 0);
            inner.dirty_save = true;
            if let Some(r) = &mut inner.renderer {
                r.invalidate();
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_layer_visible(engine: *mut CalmEngine, index: u32) -> c_int {
    if engine.is_null() {
        return -1;
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock();
        let doc = inner.doc.as_ref().context("no project is open")?;
        let layer = doc
            .layers
            .get(index as usize)
            .with_context(|| format!("layer index {index} out of range"))?;
        Ok::<c_int, anyhow::Error>(if layer.visible { 1 } else { 0 })
    })) {
        Ok(Ok(v)) => v,
        _ => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_active_layer(
    engine: *mut CalmEngine,
    index: u32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.set_active_layer(index as usize);
            if let Some(r) = &mut inner.renderer {
                r.invalidate();
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_duplicate_layer(
    engine: *mut CalmEngine,
    index: u32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        if !doc.duplicate_layer(index as usize) {
            bail!("layer {index} cannot be duplicated");
        }
        inner.dirty_save = true;
        if let Some(r) = &mut inner.renderer {
            r.invalidate();
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_move_layer_up(
    engine: *mut CalmEngine,
    index: u32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        if !doc.move_layer_up(index as usize) {
            bail!("layer {index} cannot move up");
        }
        inner.dirty_save = true;
        if let Some(r) = &mut inner.renderer {
            r.invalidate();
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_move_layer_down(
    engine: *mut CalmEngine,
    index: u32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        if !doc.move_layer_down(index as usize) {
            bail!("layer {index} cannot move down");
        }
        inner.dirty_save = true;
        if let Some(r) = &mut inner.renderer {
            r.invalidate();
        }
        Ok(())
    })
}

/// Drag-reorder, stated in the panel's own top-first rows. The engine owns the flip to its
/// bottom-first stack so the shell never computes a layer index.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_move_layer_row(
    engine: *mut CalmEngine,
    from_row: u32,
    to_row: u32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        if !doc.move_layer_row(from_row as usize, to_row as usize) {
            bail!("row {from_row} cannot move to {to_row}");
        }
        inner.dirty_save = true;
        if let Some(r) = &mut inner.renderer {
            r.invalidate();
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_layer_name(
    engine: *mut CalmEngine,
    index: u32,
    name: *const c_char,
) -> CalmStatus {
    if name.is_null() {
        return CalmStatus::Null;
    }
    let Ok(text) = (unsafe { CStr::from_ptr(name) }).to_str() else {
        return CalmStatus::Error;
    };
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        if !doc.set_layer_name(index as usize, text) {
            bail!("layer {index} cannot be named {text:?}");
        }
        inner.dirty_save = true;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_layer_locked(
    engine: *mut CalmEngine,
    index: u32,
    locked: u8,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        if !doc.set_layer_locked(index as usize, locked != 0) {
            bail!("layer {index} lock is already {locked}");
        }
        inner.dirty_save = true;
        if let Some(r) = &mut inner.renderer {
            r.invalidate();
        }
        Ok(())
    })
}

/// Whether this layer is the board's backing sheet. `Layer::is_paper` is name-matched, so the
/// shell asking the engine rather than comparing the label it was handed is what keeps the
/// rule in one place — and the label it was handed is localised.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_layer_is_paper(engine: *mut CalmEngine, index: u32) -> c_int {
    if engine.is_null() {
        return -1;
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock();
        let doc = inner.doc.as_ref().context("no project is open")?;
        let layer = doc
            .layers
            .get(index as usize)
            .with_context(|| format!("layer index {index} out of range"))?;
        Ok::<c_int, anyhow::Error>(if layer.is_paper() { 1 } else { 0 })
    })) {
        Ok(Ok(v)) => v,
        _ => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_layer_locked(engine: *mut CalmEngine, index: u32) -> c_int {
    if engine.is_null() {
        return -1;
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock();
        let doc = inner.doc.as_ref().context("no project is open")?;
        let layer = doc
            .layers
            .get(index as usize)
            .with_context(|| format!("layer index {index} out of range"))?;
        Ok::<c_int, anyhow::Error>(if layer.locked { 1 } else { 0 })
    })) {
        Ok(Ok(v)) => v,
        _ => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_layer_opacity(
    engine: *mut CalmEngine,
    index: u32,
    opacity: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        doc.set_layer_opacity(index as usize, opacity);
        inner.dirty_save = true;
        if let Some(r) = &mut inner.renderer {
            r.invalidate();
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_layer_blend_mode(
    engine: *mut CalmEngine,
    index: u32,
    mode: u32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        let mode =
            BlendMode::from_u32(mode).with_context(|| format!("unknown blend mode id {mode}"))?;
        doc.set_layer_blend_mode(index as usize, mode);
        inner.dirty_save = true;
        if let Some(r) = &mut inner.renderer {
            r.invalidate();
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_layer_opacity(engine: *mut CalmEngine, index: u32) -> f32 {
    if engine.is_null() {
        return 1.0;
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock();
        let doc = inner.doc.as_ref().context("no project is open")?;
        let layer = doc
            .layers
            .get(index as usize)
            .with_context(|| format!("layer index {index} out of range"))?;
        Ok::<f32, anyhow::Error>(layer.opacity)
    })) {
        Ok(Ok(v)) => v,
        _ => 1.0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_layer_blend_mode(engine: *mut CalmEngine, index: u32) -> u32 {
    if engine.is_null() {
        return 0;
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock();
        let doc = inner.doc.as_ref().context("no project is open")?;
        let layer = doc
            .layers
            .get(index as usize)
            .with_context(|| format!("layer index {index} out of range"))?;
        Ok::<u32, anyhow::Error>(layer.blend_mode.as_u32())
    })) {
        Ok(Ok(v)) => v,
        _ => 0,
    }
}

#[repr(C)]
pub struct CalmAdjustments {
    pub brightness: f32,
    pub contrast: f32,
    pub vibrance: f32,
    pub saturation: f32,
    pub levels_gamma: f32,
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_layer_adjustments(
    engine: *mut CalmEngine,
    index: u32,
    brightness: f32,
    contrast: f32,
    vibrance: f32,
    saturation: f32,
    levels_gamma: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        doc.set_layer_adjustments(
            index as usize,
            Adjustments {
                brightness,
                contrast,
                vibrance,
                saturation,
                levels_gamma,
            },
        );
        inner.dirty_save = true;
        if let Some(r) = &mut inner.renderer {
            r.invalidate();
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_nudge_layer_adjustment(
    engine: *mut CalmEngine,
    index: u32,
    kind: u32,
    steps: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        let kind = AdjustmentKind::from_u32(kind)
            .with_context(|| format!("unknown adjustment kind {kind}"))?;
        if !doc.nudge_layer_adjustment(index as usize, kind, steps) {
            return Ok(());
        }
        inner.dirty_save = true;
        if let Some(r) = &mut inner.renderer {
            r.invalidate();
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_layer_adjustments(
    engine: *mut CalmEngine,
    index: u32,
    out: *mut CalmAdjustments,
) -> CalmStatus {
    if out.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let doc = inner.doc.as_ref().context("no project is open")?;
        let layer = doc
            .layers
            .get(index as usize)
            .with_context(|| format!("layer index {index} out of range"))?;
        let adj = layer.adjustments.unwrap_or_default();
        unsafe {
            *out = CalmAdjustments {
                brightness: adj.brightness,
                contrast: adj.contrast,
                vibrance: adj.vibrance,
                saturation: adj.saturation,
                levels_gamma: adj.levels_gamma,
            };
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_hover_layer(
    engine: *mut CalmEngine,
    index: c_int,
) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            let next = if index < 0 {
                None
            } else {
                Some(index as usize)
            };
            // Scrolling the layers panel drags row after row under a stationary cursor, so this
            // arrives constantly. Nothing but the outline changes, and only when the index
            // actually moves — anything heavier here is paid once per row crossed.
            if doc.hover_layer == next {
                return Ok(());
            }
            doc.hover_layer = next;
            if let Some(r) = &mut inner.renderer {
                r.invalidate_overlay();
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_clear_layer(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.clear_active_layer();
            inner.dirty_save = true;
            if let Some(r) = &mut inner.renderer {
                r.invalidate();
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_state(
    engine: *mut CalmEngine,
    out: *mut CalmState,
) -> CalmStatus {
    if out.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let Some(doc) = &inner.doc else {
            unsafe {
                *out = CalmState {
                    width: 0,
                    height: 0,
                    zoom: 1.0,
                    min_zoom: 1.0,
                    max_zoom: 1.0,
                    pan_x: 0.0,
                    pan_y: 0.0,
                    active_layer: 0,
                    layer_count: 0,
                    can_undo: 0,
                    can_redo: 0,
                    stroke_active: 0,
                    dark_theme: 1,
                    accent: 0,
                    zoom_unit: 0.0,
                    last_shape_tool: inner.last_shape_tool as u32,
                    last_select_tool: inner.last_select_tool as u32,
                    is_fit: 0,
                    transform_active: 0,
                };
            }
            return Ok(());
        };
        let (dw, dh) = (doc.width as f32, doc.height as f32);
        unsafe {
            *out = CalmState {
                width: doc.width,
                height: doc.height,
                zoom: doc.camera.zoom,
                min_zoom: doc.camera.min_zoom(dw, dh),
                max_zoom: doc.camera.max_zoom(dw, dh),
                pan_x: doc.camera.pan_x,
                pan_y: doc.camera.pan_y,
                active_layer: doc.active_layer as u32,
                layer_count: doc.layers.len() as u32,
                can_undo: doc.history.can_undo() as u8,
                can_redo: doc.history.can_redo() as u8,
                stroke_active: doc.stroke_active as u8,
                dark_theme: doc.dark_theme as u8,
                accent: pack_rgb(doc.accent),
                zoom_unit: doc.camera.zoom_unit(dw, dh),
                last_shape_tool: doc.last_shape_tool as u32,
                last_select_tool: doc.last_select_tool as u32,
                is_fit: doc.camera.is_fit(dw, dh) as u8,
                transform_active: doc.transform_active as u8,
            };
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_layer_name(
    engine: *mut CalmEngine,
    index: u32,
) -> *mut c_char {
    if engine.is_null() {
        return ptr::null_mut();
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock();
        inner
            .doc
            .as_ref()
            .and_then(|d| d.layers.get(index as usize))
            .map(|l| cstring(&l.name))
    })) {
        Ok(Some(p)) => p,
        _ => ptr::null_mut(),
    }
}

/// The layer's stable identity. Names are user-editable and can collide; the shell keys its
/// per-layer caches on this so reordering or renaming never makes one layer wear another's
/// preview.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_layer_id(engine: *mut CalmEngine, index: u32) -> *mut c_char {
    if engine.is_null() {
        return ptr::null_mut();
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock();
        inner
            .doc
            .as_ref()
            .and_then(|d| d.layers.get(index as usize))
            .map(|l| cstring(&l.id))
    })) {
        Ok(Some(p)) => p,
        _ => ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn calm_buffer_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)));
    }));
}

/// A layer's document-space box, as the layers panel shows it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CalmLayerBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_layer_bounds(
    engine: *mut CalmEngine,
    index: u32,
    out: *mut CalmLayerBounds,
) -> CalmStatus {
    if engine.is_null() || out.is_null() {
        return CalmStatus::Null;
    }
    unsafe { *out = CalmLayerBounds::default() };
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock();
        let doc = inner.doc.as_ref()?;
        let (x, y, x1, y1) = doc.layer_bounds(index as usize)?;
        Some(CalmLayerBounds {
            x,
            y,
            width: x1 - x,
            height: y1 - y,
        })
    })) {
        Ok(Some(bounds)) => {
            unsafe { *out = bounds };
            CalmStatus::Ok
        }
        _ => CalmStatus::Error,
    }
}

/// Moves the layer to `(x, y)` and crops it to `width` × `height`. Size never scales the layer
/// up — see `Document::set_layer_bounds`; the shell reads the bounds back to see what stuck.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_layer_bounds(
    engine: *mut CalmEngine,
    index: u32,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().context("no project is open")?;
        if !doc.set_layer_bounds(index as usize, x, y, width, height) {
            bail!("layer index {index} has no bounds to set");
        }
        inner.dirty_save = true;
        if let Some(r) = &mut inner.renderer {
            r.invalidate();
        }
        Ok(())
    })
}

/// A counter that changes only when this layer's own pixels change. The shell caches one
/// rendered thumbnail per layer against it, which is what guarantees a preview is a function of
/// that layer's content and nothing else — showing or hiding any layer never moves it.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_layer_preview_revision(
    engine: *mut CalmEngine,
    index: u32,
) -> u64 {
    if engine.is_null() {
        return 0;
    }
    catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock();
        let layer = inner.doc.as_ref()?.layers.get(index as usize)?;
        match layer.tiles() {
            Some(grid) => Some(grid.content_revision()),
            // A vector layer's thumbnail is a flat swatch of its item's color, so presence
            // of that item is all the shell has to notice a change in.
            None => Some(u64::from(layer.content.item().is_some())),
        }
    }))
    .ok()
    .flatten()
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_layer_thumbnail(
    engine: *mut CalmEngine,
    layer_index: u32,
    max_side: u32,
    out_rgba: *mut *mut u8,
    out_w: *mut u32,
    out_h: *mut u32,
) -> CalmStatus {
    if engine.is_null() || out_rgba.is_null() || out_w.is_null() || out_h.is_null() {
        return CalmStatus::Null;
    }
    unsafe {
        *out_rgba = ptr::null_mut();
        *out_w = 0;
        *out_h = 0;
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let mut inner = mutex.lock();
        let doc = inner.doc.as_mut().context("no project is open")?;
        let layer = doc
            .layers
            .get_mut(layer_index as usize)
            .with_context(|| format!("layer index {layer_index} out of range"))?;
        // Served from the layer's cached preview, so asking for the same layer again at any
        // size costs a resample of at most `LAYER_PREVIEW_MAX_SIDE` rather than another scan of
        // the whole layer. The cache rebuilds itself the first time it is asked for after an
        // edit; nothing here has to know when that was.
        let (w, h, rgba) = if let Some(tiles) = layer.tiles_mut() {
            tiles.preview().scaled(max_side.max(1))
        } else if let Some(item) = layer.content.item() {
            let side = max_side.clamp(1, 64);
            let color = item.color();
            let mut rgba = vec![0u8; (side * side * 4) as usize];
            for px in rgba.chunks_exact_mut(4) {
                px.copy_from_slice(&color);
            }
            (side, side, rgba)
        } else {
            bail!("no project is open and no accent color to fall back on");
        };
        let mut boxed = rgba.into_boxed_slice();
        let ptr = boxed.as_mut_ptr();
        std::mem::forget(boxed);
        unsafe {
            *out_rgba = ptr;
            *out_w = w;
            *out_h = h;
        }
        Ok::<(), anyhow::Error>(())
    })) {
        Ok(Ok(())) => CalmStatus::Ok,
        _ => CalmStatus::Error,
    }
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_composite_rgba(
    engine: *mut CalmEngine,
    out_rgba: *mut *mut u8,
    out_w: *mut u32,
    out_h: *mut u32,
) -> CalmStatus {
    if engine.is_null() || out_rgba.is_null() || out_w.is_null() || out_h.is_null() {
        return CalmStatus::Null;
    }
    unsafe {
        *out_rgba = ptr::null_mut();
        *out_w = 0;
        *out_h = 0;
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock();
        let doc = inner.doc.as_ref().context("no project is open")?;
        let (w, h, rgba) = doc.composite_rgba();
        let mut boxed = rgba.into_boxed_slice();
        let ptr = boxed.as_mut_ptr();
        std::mem::forget(boxed);
        unsafe {
            *out_rgba = ptr;
            *out_w = w;
            *out_h = h;
        }
        Ok::<(), anyhow::Error>(())
    })) {
        Ok(Ok(())) => CalmStatus::Ok,
        _ => CalmStatus::Error,
    }
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_export_psd(
    engine: *mut CalmEngine,
    out_bytes: *mut *mut u8,
    out_len: *mut usize,
) -> CalmStatus {
    if engine.is_null() || out_bytes.is_null() || out_len.is_null() {
        return CalmStatus::Null;
    }
    unsafe {
        *out_bytes = ptr::null_mut();
        *out_len = 0;
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock();
        let doc = inner.doc.as_ref().context("no project is open")?;
        let bytes = encode_psd(doc);
        let mut boxed = bytes.into_boxed_slice();
        let len = boxed.len();
        let ptr = boxed.as_mut_ptr();
        std::mem::forget(boxed);
        unsafe {
            *out_bytes = ptr;
            *out_len = len;
        }
        Ok::<(), anyhow::Error>(())
    })) {
        Ok(Ok(())) => CalmStatus::Ok,
        _ => CalmStatus::Error,
    }
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_layer_rgba(
    engine: *mut CalmEngine,
    layer_index: u32,
    out_rgba: *mut *mut u8,
    out_w: *mut u32,
    out_h: *mut u32,
) -> CalmStatus {
    if engine.is_null() || out_rgba.is_null() || out_w.is_null() || out_h.is_null() {
        return CalmStatus::Null;
    }
    unsafe {
        *out_rgba = ptr::null_mut();
        *out_w = 0;
        *out_h = 0;
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock();
        let doc = inner.doc.as_ref().context("no project is open")?;
        let (w, h, rgba) = doc
            .layer_rgba(layer_index as usize)
            .context("layer has no raster content")?;
        let mut boxed = rgba.into_boxed_slice();
        let ptr = boxed.as_mut_ptr();
        std::mem::forget(boxed);
        unsafe {
            *out_rgba = ptr;
            *out_w = w;
            *out_h = h;
        }
        Ok::<(), anyhow::Error>(())
    })) {
        Ok(Ok(())) => CalmStatus::Ok,
        _ => CalmStatus::Error,
    }
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_layer_svg(
    engine: *mut CalmEngine,
    layer_index: u32,
) -> *mut c_char {
    if engine.is_null() {
        return ptr::null_mut();
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock();
        let doc = inner.doc.as_ref().context("no project is open")?;
        let svg = doc
            .layer_svg(layer_index as usize)
            .context("layer has no vector content")?;
        CString::new(svg).context("svg contained an interior NUL")
    })) {
        Ok(Ok(svg)) => svg.into_raw(),
        _ => ptr::null_mut(),
    }
}

/// The whole visible stack as one SVG: vector layers as real primitives, everything with
/// pixels as an embedded PNG. Layered, unlike the flattened raster exports.
/// Whole-document PDF. Layered like the SVG export rather than flattened like the raster
/// ones: vector layers stay real PDF paths, opacity and blend mode ride an `/ExtGState`, and
/// only the layers that really are pixels become image XObjects.
///
/// `dpi` decides how many points a document pixel is worth — the page shrinks as it rises
/// while the pixels stay put. Pass `calm_pdf_default_dpi()` for one pixel per point.
#[no_mangle]
pub unsafe extern "C" fn calm_engine_export_pdf(
    engine: *mut CalmEngine,
    dpi: f32,
    out_bytes: *mut *mut u8,
    out_len: *mut usize,
) -> CalmStatus {
    if engine.is_null() || out_bytes.is_null() || out_len.is_null() {
        return CalmStatus::Null;
    }
    unsafe {
        *out_bytes = ptr::null_mut();
        *out_len = 0;
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock();
        let doc = inner.doc.as_ref().context("no project is open")?;
        let bytes = encode_pdf(doc, dpi);
        let mut boxed = bytes.into_boxed_slice();
        let len = boxed.len();
        let ptr = boxed.as_mut_ptr();
        std::mem::forget(boxed);
        unsafe {
            *out_bytes = ptr;
            *out_len = len;
        }
        Ok::<(), anyhow::Error>(())
    })) {
        Ok(Ok(())) => CalmStatus::Ok,
        _ => CalmStatus::Error,
    }
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_export_svg(engine: *mut CalmEngine) -> *mut c_char {
    if engine.is_null() {
        return ptr::null_mut();
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock();
        let doc = inner.doc.as_ref().context("no project is open")?;
        CString::new(encode_svg(doc)).context("svg contained an interior NUL")
    })) {
        Ok(Ok(svg)) => svg.into_raw(),
        _ => ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_selection_rgba(
    engine: *mut CalmEngine,
    out_rgba: *mut *mut u8,
    out_w: *mut u32,
    out_h: *mut u32,
) -> CalmStatus {
    if engine.is_null() || out_rgba.is_null() || out_w.is_null() || out_h.is_null() {
        return CalmStatus::Null;
    }
    unsafe {
        *out_rgba = ptr::null_mut();
        *out_w = 0;
        *out_h = 0;
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock();
        let doc = inner.doc.as_ref().context("no project is open")?;
        let (w, h, rgba) = doc
            .selection_rgba()
            .context("there is no active selection")?;
        let mut boxed = rgba.into_boxed_slice();
        let ptr = boxed.as_mut_ptr();
        std::mem::forget(boxed);
        unsafe {
            *out_rgba = ptr;
            *out_w = w;
            *out_h = h;
        }
        Ok::<(), anyhow::Error>(())
    })) {
        Ok(Ok(())) => CalmStatus::Ok,
        _ => CalmStatus::Error,
    }
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_has_selection(engine: *mut CalmEngine) -> c_int {
    if engine.is_null() {
        return 0;
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock();
        let doc = inner.doc.as_ref().context("no project is open")?;
        Ok::<bool, anyhow::Error>(doc.selection.is_some())
    })) {
        Ok(Ok(true)) => 1,
        _ => 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_deselect(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.deselect();
            if let Some(r) = &mut inner.renderer {
                r.invalidate();
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_select_all(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.select_all();
            if let Some(r) = &mut inner.renderer {
                r.invalidate();
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_invert_selection(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.invert_selection();
            if let Some(r) = &mut inner.renderer {
                r.invalidate();
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_selection_clear_pixels(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.clear_selection_pixels();
            inner.dirty_save = true;
            if let Some(r) = &mut inner.renderer {
                r.invalidate();
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_project_create(
    engine: *mut CalmEngine,
    name: *const c_char,
    width: u32,
    height: u32,
) -> *mut c_char {
    if engine.is_null() || name.is_null() {
        return ptr::null_mut();
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let mut inner = mutex.lock();
        let name = unsafe { CStr::from_ptr(name) }
            .to_str()
            .unwrap_or(calumma_core::UNTITLED);
        inner.close_document();
        let doc = inner
            .store
            .create(name, width, height)
            .with_context(|| format!("creating project {name} at {width}x{height}"))?;
        let id = doc.id.clone();
        inner.install_document(doc);
        Ok::<_, anyhow::Error>(cstring(&id))
    })) {
        Ok(Ok(p)) => p,
        _ => ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn calm_import_max_side() -> u32 {
    IMPORT_MAX_SIDE
}

#[no_mangle]
pub unsafe extern "C" fn calm_project_create_from_image(
    engine: *mut CalmEngine,
    name: *const c_char,
    width: u32,
    height: u32,
    premultiplied_rgba: *const u8,
    len: usize,
) -> *mut c_char {
    if engine.is_null() || name.is_null() || premultiplied_rgba.is_null() {
        return ptr::null_mut();
    }
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4));
    if width == 0
        || height == 0
        || width > IMPORT_MAX_SIDE
        || height > IMPORT_MAX_SIDE
        || expected != Some(len)
    {
        return ptr::null_mut();
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let mut inner = mutex.lock();
        let name = unsafe { CStr::from_ptr(name) }
            .to_str()
            .unwrap_or(calumma_core::UNTITLED);
        let mut rgba = unsafe { std::slice::from_raw_parts(premultiplied_rgba, len) }.to_vec();
        unpremultiply_rgba(&mut rgba);
        inner.close_document();
        let mut doc = inner
            .store
            .create(name, width, height)
            .with_context(|| format!("creating project {name} at {width}x{height}"))?;
        if !doc.place_image(&rgba, width, height) {
            bail!("placing a {width}x{height} image into the new project failed");
        }
        inner.store.save(&mut doc).context("saving project")?;
        let id = doc.id.clone();
        inner.install_document(doc);
        Ok::<_, anyhow::Error>(cstring(&id))
    })) {
        Ok(Ok(p)) => p,
        _ => ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn calm_project_open(
    engine: *mut CalmEngine,
    id: *const c_char,
) -> CalmStatus {
    if id.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let id = unsafe { CStr::from_ptr(id) }
            .to_str()
            .context("project id is not valid UTF-8")?;
        inner.close_document();
        let doc = inner
            .store
            .open_project(id)
            .with_context(|| format!("opening project {id}"))?;
        inner.install_document(doc);
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_project_close(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        inner.close_document();
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_project_list(
    engine: *mut CalmEngine,
    out: *mut CalmProjectInfo,
    cap: usize,
) -> usize {
    if engine.is_null() || out.is_null() || cap == 0 {
        return 0;
    }
    catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock();
        let items = inner.store.list_recent(cap).unwrap_or_default();
        Some(write_projects(out, &items, cap))
    }))
    .ok()
    .flatten()
    .unwrap_or_default()
}

fn write_projects(out: *mut CalmProjectInfo, items: &[ProjectListItem], cap: usize) -> usize {
    let n = items.len().min(cap);
    for (i, item) in items.iter().take(n).enumerate() {
        unsafe {
            *out.add(i) = CalmProjectInfo {
                id: cstring(&item.id),
                name: cstring(&item.name),
                width: item.width,
                height: item.height,
                opened_at: item.opened_at,
                accent: pack_rgb(item.accent),
            };
        }
    }
    n
}

#[no_mangle]
pub unsafe extern "C" fn calm_project_get(
    engine: *mut CalmEngine,
    id: *const c_char,
    out: *mut CalmProjectInfo,
) -> CalmStatus {
    if id.is_null() || out.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let id = unsafe { CStr::from_ptr(id) }
            .to_str()
            .context("project id is not valid UTF-8")?;
        let item = inner.store.project(id).context("loading project")?;
        unsafe {
            *out = CalmProjectInfo {
                id: cstring(&item.id),
                name: cstring(&item.name),
                width: item.width,
                height: item.height,
                opened_at: item.opened_at,
                accent: pack_rgb(item.accent),
            };
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_open_project_tabs(
    engine: *mut CalmEngine,
    out: *mut *mut c_char,
    cap: usize,
) -> usize {
    if engine.is_null() || out.is_null() || cap == 0 {
        return 0;
    }
    catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock();
        let ids = inner.store.open_project_tabs().unwrap_or_default();
        let n = ids.len().min(cap);
        for (i, id) in ids.iter().take(n).enumerate() {
            unsafe {
                *out.add(i) = cstring(id);
            }
        }
        Some(n)
    }))
    .ok()
    .flatten()
    .unwrap_or_default()
}

#[no_mangle]
pub unsafe extern "C" fn calm_set_open_project_tabs(
    engine: *mut CalmEngine,
    ids: *const *const c_char,
    count: usize,
) -> CalmStatus {
    if ids.is_null() && count > 0 {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let mut owned = Vec::with_capacity(count);
        for i in 0..count {
            let ptr = unsafe { *ids.add(i) };
            if ptr.is_null() {
                continue;
            }
            let id = unsafe { CStr::from_ptr(ptr) }
                .to_str()
                .context("project tab id is not valid UTF-8")?;
            owned.push(id.to_string());
        }
        inner
            .store
            .set_open_project_tabs(&owned)
            .context("persisting open project tabs")?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_project_delete(
    engine: *mut CalmEngine,
    id: *const c_char,
) -> CalmStatus {
    if id.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let id = unsafe { CStr::from_ptr(id) }
            .to_str()
            .context("project id is not valid UTF-8")?;
        inner.store.delete(id).context("deleting project")?;
        if inner.doc.as_ref().map(|d| d.id.as_str()) == Some(id) {
            inner.doc = None;
            inner.release_gpu_resources();
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_project_delete_all(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        inner.close_document();
        inner
            .store
            .delete_all_projects()
            .context("deleting all projects")?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_project_save(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        let Inner {
            doc,
            store,
            last_save,
            dirty_save,
            ..
        } = inner;
        if let Some(doc) = doc.as_mut() {
            store.save(doc).context("saving project")?;
            *dirty_save = false;
            *last_save = Instant::now();
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_install_platform_ops(
    engine: *mut CalmEngine,
    ops: *const CalmPlatformOps,
) -> CalmStatus {
    if ops.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let ops = unsafe { *ops };
        inner.platform_ops = Some(ops);
        inner.registry = OpRegistry::new();
        inner
            .registry
            .register_platform(Box::new(PlatformOp::remove_background(ops)));
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_op_available(engine: *mut CalmEngine, kind: u32) -> bool {
    if engine.is_null() {
        return false;
    }
    let Some(op_kind) = parse_op_kind(kind) else {
        return false;
    };
    catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock();
        Some(inner.registry.available(op_kind))
    }))
    .ok()
    .flatten()
    .unwrap_or(false)
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_run_op(
    engine: *mut CalmEngine,
    kind: u32,
    layer_index: u32,
) -> CalmStatus {
    let Some(op_kind) = parse_op_kind(kind) else {
        return CalmStatus::Error;
    };
    if engine.is_null() {
        return CalmStatus::Null;
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let layer_index = layer_index as usize;

        enum Prepared {
            Done,
            Platform {
                input: calumma_ops::OpInput,
                ops: CalmPlatformOps,
            },
        }

        let prepared = {
            let mut inner = mutex.lock();
            match inner.registry.backend_for(op_kind) {
                Some(Backend::Core) => {
                    let Inner {
                        doc,
                        registry,
                        renderer,
                        dirty_save,
                        ..
                    } = &mut *inner;
                    let doc = doc.as_mut().context("no project is open")?;
                    run_op_on_document(registry, doc, layer_index, op_kind, &OpParams::default())
                        .context("running the core op")?;
                    *dirty_save = true;
                    if let Some(r) = renderer {
                        r.invalidate();
                    }
                    Prepared::Done
                }
                Some(Backend::Platform) => {
                    if !inner.registry.available(op_kind) {
                        bail!("op {op_kind:?} has no registered platform backend");
                    }
                    let ops = inner.platform_ops.context("no platform ops registered")?;
                    let doc = inner.doc.as_ref().context("no project is open")?;
                    let input =
                        layer_input(doc, layer_index).context("reading the layer for the op")?;
                    Prepared::Platform { input, ops }
                }
                None => bail!("op {op_kind:?} is not available on this platform"),
            }
        };

        if let Prepared::Platform { input, ops } = prepared {
            let output = PlatformOp::for_kind(op_kind, ops)
                .run(input, &OpParams::default())
                .context("running the platform op")?;
            let mut inner = mutex.lock();
            let Inner {
                doc,
                renderer,
                dirty_save,
                ..
            } = &mut *inner;
            let doc = doc.as_mut().context("no project is open")?;
            apply_output(doc, layer_index, output).context("applying the op result")?;
            *dirty_save = true;
            if let Some(r) = renderer {
                r.invalidate();
            }
        }
        Ok(())
    })) {
        Ok(Ok(())) => CalmStatus::Ok,
        _ => CalmStatus::Error,
    }
}

#[cfg(test)]
mod stub_renderer_tests {
    use super::*;
    use std::ffi::CString;

    fn engine_with_project() -> (tempfile::TempDir, *mut CalmEngine) {
        let dir = tempfile::tempdir().unwrap();
        let path = CString::new(dir.path().join("t.sqlite").to_str().unwrap()).unwrap();
        let ptr = unsafe { calm_engine_new(path.as_ptr()) };
        assert!(!ptr.is_null());
        let name = CString::new("StubGpu").unwrap();
        let id = unsafe { calm_project_create(ptr, name.as_ptr(), 64, 64) };
        assert!(!id.is_null());
        unsafe { calm_string_free(id) };
        (dir, ptr)
    }

    #[test]
    fn stub_attach_drives_resize_render_and_invalidate() {
        let (_dir, ptr) = engine_with_project();
        let layer = 0x1 as *mut c_void;
        unsafe {
            assert_eq!(
                calm_engine_attach_surface(ptr, layer, 128, 96, 2.0),
                CalmStatus::Ok
            );
            assert_eq!(calm_engine_resize(ptr, 160, 120, 1.0), CalmStatus::Ok);
            assert_eq!(calm_engine_render(ptr), CalmStatus::Ok);
            assert_eq!(calm_engine_set_tool(ptr, Tool::Pen as u32), CalmStatus::Ok);
            assert_eq!(calm_engine_pointer_down(ptr, 10.0, 10.0), CalmStatus::Ok);
            assert_eq!(calm_engine_pointer_move(ptr, 20.0, 20.0), CalmStatus::Ok);
            assert_eq!(calm_engine_pointer_up(ptr, 20.0, 20.0), CalmStatus::Ok);
            assert_eq!(calm_engine_pan(ptr, 2.0, 2.0), CalmStatus::Ok);
            assert_eq!(calm_engine_zoom(ptr, 30.0, 30.0, 1.1), CalmStatus::Ok);
            assert_eq!(calm_engine_fit(ptr), CalmStatus::Ok);
            assert_eq!(calm_engine_render(ptr), CalmStatus::Ok);
            assert_eq!(calm_engine_resize_document(ptr, 80, 80), CalmStatus::Ok);
            assert_eq!(calm_engine_render(ptr), CalmStatus::Ok);
            calm_engine_free(ptr);
        }
    }

    /// The regression this guards: a `parking_lot::Mutex` (unlike `std::sync::Mutex`) never
    /// poisons, so a bug that panics while the lock is held degrades that one call — caught by
    /// `catch_unwind` above — instead of wedging every later call behind "engine mutex poisoned
    /// by an earlier panic" for the rest of the process.
    #[test]
    fn a_panic_while_locked_does_not_wedge_later_calls() {
        let (_dir, ptr) = engine_with_project();
        let status = with_inner(ptr, |_inner| {
            panic!("simulated panic while holding the lock")
        });
        assert_eq!(
            status,
            CalmStatus::Error,
            "the panic is caught, not propagated"
        );

        assert_eq!(
            unsafe { calm_engine_render(ptr) },
            CalmStatus::Ok,
            "the lock still works after the earlier panic"
        );
        unsafe { calm_engine_free(ptr) };
    }
}
