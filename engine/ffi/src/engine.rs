use crate::platform::{parse_op_kind, CalmPlatformOps, PlatformOp};
use calumma_core::limits::{AUTOSAVE_INTERVAL_MS, BRUSH_SIZE_MAX, BRUSH_SIZE_MIN, IMPORT_MAX_SIDE};
use calumma_core::{
    project_color, unpremultiply_rgba, Adjustments, BlendMode, BoardColors, Document, Tool,
    PROJECT_COLORS,
};
use calumma_io::{encode_psd, ProjectListItem, ProjectStore};
use calumma_ops::{
    apply_output, layer_input, run_op_on_document, Backend, Op, OpParams, OpRegistry,
};
use calumma_render::Renderer;
use std::ffi::{c_char, c_void, CStr, CString};
use std::os::raw::c_int;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[repr(C)]
pub struct CalmEngine {
    _private: [u8; 0],
}

#[repr(i32)]
#[derive(Clone, Copy, PartialEq, Eq)]
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
}

#[repr(C)]
pub struct CalmLayerInfo {
    pub index: u32,
    pub visible: u8,
    pub name: *mut c_char,
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

fn pack_rgb(rgb: [u8; 3]) -> u32 {
    ((rgb[0] as u32) << 16) | ((rgb[1] as u32) << 8) | rgb[2] as u32
}

fn unpack_rgb(packed: u32) -> [u8; 3] {
    [
        ((packed >> 16) & 0xFF) as u8,
        ((packed >> 8) & 0xFF) as u8,
        (packed & 0xFF) as u8,
    ]
}

fn unpack_rgba(packed: u32) -> [u8; 4] {
    [
        ((packed >> 24) & 0xFF) as u8,
        ((packed >> 16) & 0xFF) as u8,
        ((packed >> 8) & 0xFF) as u8,
        (packed & 0xFF) as u8,
    ]
}

struct Inner {
    doc: Option<Document>,
    store: ProjectStore,
    renderer: Option<Renderer>,
    instance: wgpu::Instance,
    last_save: Instant,
    dirty_save: bool,
    registry: OpRegistry,
    platform_ops: Option<CalmPlatformOps>,
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
            registry: OpRegistry::new(),
            platform_ops: None,
        })
    }

    fn autosave(&mut self) {
        if !self.dirty_save {
            return;
        }
        if self.doc.as_ref().is_some_and(|d| d.stroke_active) {
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

fn with_inner<F>(engine: *mut CalmEngine, f: F) -> CalmStatus
where
    F: FnOnce(&mut Inner) -> Result<(), ()> + std::panic::UnwindSafe,
{
    if engine.is_null() {
        return CalmStatus::Null;
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let mut inner = mutex.lock().map_err(|_| ())?;
        f(&mut inner)
    })) {
        Ok(Ok(())) => CalmStatus::Ok,
        _ => CalmStatus::Error,
    }
}

fn cstring(s: &str) -> *mut c_char {
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
        Ok(Ok(inner)) => Box::into_raw(Box::new(Mutex::new(inner))) as *mut CalmEngine,
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
        if let Ok(mut inner) = mutex.lock() {
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
            return Err(());
        }
        let surface = unsafe {
            inner
                .instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(layer))
                .map_err(|_| ())?
        };
        let (pw, ph) = {
            let dpr = scale.max(1.0);
            (
                ((w as f32 * dpr).round() as u32).max(1),
                ((h as f32 * dpr).round() as u32).max(1),
            )
        };
        let renderer = Renderer::from_surface(surface, &inner.instance, pw, ph).map_err(|_| ())?;
        inner.renderer = Some(renderer);
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
        if let Some(doc) = &mut inner.doc {
            doc.resize_viewport(w as f32, h as f32, scale);
        }
        if let Some(renderer) = &mut inner.renderer {
            let dpr = scale.max(1.0);
            renderer.resize(
                ((w as f32 * dpr).round() as u32).max(1),
                ((h as f32 * dpr).round() as u32).max(1),
            );
            renderer.invalidate();
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
        let doc = inner.doc.as_mut().ok_or(())?;
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
        inner.autosave();
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
            doc.pointer_move(x, y);
            if let Some(r) = &mut inner.renderer {
                r.invalidate();
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
pub unsafe extern "C" fn calm_engine_pan(engine: *mut CalmEngine, dx: f32, dy: f32) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            let (w, h) = (doc.width as f32, doc.height as f32);
            doc.camera.pan_by(dx, dy, w, h);
            if let Some(r) = &mut inner.renderer {
                r.invalidate();
            }
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
            if let Some(r) = &mut inner.renderer {
                r.invalidate();
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_fit(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.fit_to_view();
            if let Some(r) = &mut inner.renderer {
                r.invalidate();
            }
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
            if let Some(r) = &mut inner.renderer {
                r.invalidate();
            }
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
            if let Some(r) = &mut inner.renderer {
                r.invalidate();
            }
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
            if let Some(r) = &mut inner.renderer {
                r.invalidate();
            }
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
        let id = unsafe { CStr::from_ptr(id) }.to_str().map_err(|_| ())?;
        let name = unsafe { CStr::from_ptr(name) }.to_str().map_err(|_| ())?;
        let name = name.trim();
        if name.is_empty() {
            return Err(());
        }
        inner.store.rename(id, name).map_err(|_| ())?;
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
        let id = unsafe { CStr::from_ptr(id) }.to_str().map_err(|_| ())?;
        let rgb = unpack_rgb(accent);
        inner.store.set_accent(id, rgb).map_err(|_| ())?;
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
        if let Some(doc) = &mut inner.doc {
            doc.tool = Tool::from_u32(tool).ok_or(())?;
        }
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
            doc.color = [r, g, b, a];
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
pub unsafe extern "C" fn calm_engine_set_fill(engine: *mut CalmEngine, fill: u8) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.fill = fill != 0;
        }
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
pub unsafe extern "C" fn calm_engine_set_shift(engine: *mut CalmEngine, held: u8) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(doc) = &mut inner.doc {
            doc.set_shift_held(held != 0);
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_reset_layer_transform(
    engine: *mut CalmEngine,
    index: u32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().ok_or(())?;
        doc.reset_layer_transform(index as usize);
        inner.dirty_save = true;
        if let Some(r) = &mut inner.renderer {
            r.invalidate();
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
                return Err(());
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
        let inner = mutex.lock().map_err(|_| ())?;
        let doc = inner.doc.as_ref().ok_or(())?;
        let layer = doc.layers.get(index as usize).ok_or(())?;
        Ok::<c_int, ()>(if layer.visible { 1 } else { 0 })
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
        let doc = inner.doc.as_mut().ok_or(())?;
        if !doc.duplicate_layer(index as usize) {
            return Err(());
        }
        inner.dirty_save = true;
        if let Some(r) = &mut inner.renderer {
            r.invalidate();
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_merge_layer_down(
    engine: *mut CalmEngine,
    index: u32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().ok_or(())?;
        if !doc.merge_layer_down(index as usize) {
            return Err(());
        }
        inner.dirty_save = true;
        if let Some(r) = &mut inner.renderer {
            r.invalidate();
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_engine_set_layer_opacity(
    engine: *mut CalmEngine,
    index: u32,
    opacity: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().ok_or(())?;
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
        let doc = inner.doc.as_mut().ok_or(())?;
        let mode = BlendMode::from_u32(mode).ok_or(())?;
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
        let inner = mutex.lock().map_err(|_| ())?;
        let doc = inner.doc.as_ref().ok_or(())?;
        let layer = doc.layers.get(index as usize).ok_or(())?;
        Ok::<f32, ()>(layer.opacity)
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
        let inner = mutex.lock().map_err(|_| ())?;
        let doc = inner.doc.as_ref().ok_or(())?;
        let layer = doc.layers.get(index as usize).ok_or(())?;
        Ok::<u32, ()>(layer.blend_mode.as_u32())
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
    pub levels_black: f32,
    pub levels_white: f32,
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
    levels_black: f32,
    levels_white: f32,
    levels_gamma: f32,
) -> CalmStatus {
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().ok_or(())?;
        doc.set_layer_adjustments(
            index as usize,
            Adjustments {
                brightness,
                contrast,
                vibrance,
                saturation,
                levels_black,
                levels_white,
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
pub unsafe extern "C" fn calm_engine_layer_adjustments(
    engine: *mut CalmEngine,
    index: u32,
    out: *mut CalmAdjustments,
) -> CalmStatus {
    if out.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let doc = inner.doc.as_ref().ok_or(())?;
        let layer = doc.layers.get(index as usize).ok_or(())?;
        let adj = layer.adjustments.unwrap_or_default();
        unsafe {
            *out = CalmAdjustments {
                brightness: adj.brightness,
                contrast: adj.contrast,
                vibrance: adj.vibrance,
                saturation: adj.saturation,
                levels_black: adj.levels_black,
                levels_white: adj.levels_white,
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
            doc.hover_layer = if index < 0 {
                None
            } else {
                Some(index as usize)
            };
            if let Some(r) = &mut inner.renderer {
                r.invalidate();
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
        let inner = mutex.lock().ok()?;
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

#[no_mangle]
pub unsafe extern "C" fn calm_buffer_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)));
    }));
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
        let inner = mutex.lock().map_err(|_| ())?;
        let doc = inner.doc.as_ref().ok_or(())?;
        let layer = doc.layers.get(layer_index as usize).ok_or(())?;
        let (w, h, rgba) = if let Some(tiles) = layer.tiles() {
            tiles.thumbnail(max_side.max(1))
        } else if let Some(paths) = layer.content.paths() {
            let side = max_side.clamp(1, 64);
            let color = paths
                .first()
                .map(|p| p.color)
                .unwrap_or([255, 255, 255, 255]);
            let mut rgba = vec![0u8; (side * side * 4) as usize];
            for px in rgba.chunks_exact_mut(4) {
                px.copy_from_slice(&color);
            }
            (side, side, rgba)
        } else {
            return Err(());
        };
        let mut boxed = rgba.into_boxed_slice();
        let ptr = boxed.as_mut_ptr();
        std::mem::forget(boxed);
        unsafe {
            *out_rgba = ptr;
            *out_w = w;
            *out_h = h;
        }
        Ok::<(), ()>(())
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
        let inner = mutex.lock().map_err(|_| ())?;
        let doc = inner.doc.as_ref().ok_or(())?;
        let (w, h, rgba) = doc.composite_rgba();
        let mut boxed = rgba.into_boxed_slice();
        let ptr = boxed.as_mut_ptr();
        std::mem::forget(boxed);
        unsafe {
            *out_rgba = ptr;
            *out_w = w;
            *out_h = h;
        }
        Ok::<(), ()>(())
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
        let inner = mutex.lock().map_err(|_| ())?;
        let doc = inner.doc.as_ref().ok_or(())?;
        let bytes = encode_psd(doc);
        let mut boxed = bytes.into_boxed_slice();
        let len = boxed.len();
        let ptr = boxed.as_mut_ptr();
        std::mem::forget(boxed);
        unsafe {
            *out_bytes = ptr;
            *out_len = len;
        }
        Ok::<(), ()>(())
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
        let inner = mutex.lock().map_err(|_| ())?;
        let doc = inner.doc.as_ref().ok_or(())?;
        let (w, h, rgba) = doc.layer_rgba(layer_index as usize).ok_or(())?;
        let mut boxed = rgba.into_boxed_slice();
        let ptr = boxed.as_mut_ptr();
        std::mem::forget(boxed);
        unsafe {
            *out_rgba = ptr;
            *out_w = w;
            *out_h = h;
        }
        Ok::<(), ()>(())
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
        let inner = mutex.lock().map_err(|_| ())?;
        let doc = inner.doc.as_ref().ok_or(())?;
        let svg = doc.layer_svg(layer_index as usize).ok_or(())?;
        CString::new(svg).map_err(|_| ())
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
        let inner = mutex.lock().map_err(|_| ())?;
        let doc = inner.doc.as_ref().ok_or(())?;
        let (w, h, rgba) = doc.selection_rgba().ok_or(())?;
        let mut boxed = rgba.into_boxed_slice();
        let ptr = boxed.as_mut_ptr();
        std::mem::forget(boxed);
        unsafe {
            *out_rgba = ptr;
            *out_w = w;
            *out_h = h;
        }
        Ok::<(), ()>(())
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
        let inner = mutex.lock().map_err(|_| ())?;
        let doc = inner.doc.as_ref().ok_or(())?;
        Ok::<bool, ()>(doc.selection.is_some())
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
pub unsafe extern "C" fn calm_engine_paste_image(
    engine: *mut CalmEngine,
    premultiplied_rgba: *const u8,
    len: usize,
    width: u32,
    height: u32,
) -> CalmStatus {
    if engine.is_null() || premultiplied_rgba.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let doc = inner.doc.as_mut().ok_or(())?;
        let mut rgba = unsafe { std::slice::from_raw_parts(premultiplied_rgba, len) }.to_vec();
        unpremultiply_rgba(&mut rgba);
        let n = doc.layers.len() + 1;
        let name = calumma_core::names::numbered_pasted_layer(n);
        if !doc.paste_image_as_layer(name, &rgba, width, height) {
            return Err(());
        }
        inner.dirty_save = true;
        if let Some(r) = &mut inner.renderer {
            r.invalidate();
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
        let mut inner = mutex.lock().map_err(|_| ())?;
        let name = unsafe { CStr::from_ptr(name) }
            .to_str()
            .unwrap_or(calumma_core::UNTITLED);
        let doc = inner.store.create(name, width, height).map_err(|_| ())?;
        let id = doc.id.clone();
        if let Some(mut old) = inner.doc.take() {
            let _ = inner.store.save(&mut old);
        }
        inner.doc = Some(doc);
        inner.dirty_save = false;
        if let Some(r) = &mut inner.renderer {
            r.invalidate();
        }
        Ok::<_, ()>(cstring(&id))
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
        let mut inner = mutex.lock().map_err(|_| ())?;
        let name = unsafe { CStr::from_ptr(name) }
            .to_str()
            .unwrap_or(calumma_core::UNTITLED);
        let mut rgba = unsafe { std::slice::from_raw_parts(premultiplied_rgba, len) }.to_vec();
        unpremultiply_rgba(&mut rgba);
        let mut doc = inner.store.create(name, width, height).map_err(|_| ())?;
        if !doc.place_image(&rgba, width, height) {
            return Err(());
        }
        inner.store.save(&mut doc).map_err(|_| ())?;
        let id = doc.id.clone();
        if let Some(mut old) = inner.doc.take() {
            let _ = inner.store.save(&mut old);
        }
        inner.doc = Some(doc);
        inner.dirty_save = false;
        if let Some(r) = &mut inner.renderer {
            r.invalidate();
        }
        Ok::<_, ()>(cstring(&id))
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
        let id = unsafe { CStr::from_ptr(id) }.to_str().map_err(|_| ())?;
        if let Some(mut old) = inner.doc.take() {
            let _ = inner.store.save(&mut old);
        }
        let mut doc = inner.store.open_project(id).map_err(|_| ())?;
        if inner.renderer.is_some() {
            doc.fit_to_view();
        }
        inner.doc = Some(doc);
        inner.dirty_save = false;
        if let Some(r) = &mut inner.renderer {
            r.invalidate();
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_project_close(engine: *mut CalmEngine) -> CalmStatus {
    with_inner(engine, |inner| {
        if let Some(mut doc) = inner.doc.take() {
            let _ = inner.store.save(&mut doc);
        }
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
        let inner = mutex.lock().ok()?;
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
pub unsafe extern "C" fn calm_project_delete(
    engine: *mut CalmEngine,
    id: *const c_char,
) -> CalmStatus {
    if id.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let id = unsafe { CStr::from_ptr(id) }.to_str().map_err(|_| ())?;
        inner.store.delete(id).map_err(|_| ())?;
        if inner.doc.as_ref().map(|d| d.id.as_str()) == Some(id) {
            inner.doc = None;
        }
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
            store.save(doc).map_err(|_| ())?;
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
        let inner = mutex.lock().ok()?;
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
            let mut inner = mutex.lock().map_err(|_| ())?;
            match inner.registry.backend_for(op_kind) {
                Some(Backend::Core) => {
                    let Inner {
                        doc,
                        registry,
                        renderer,
                        dirty_save,
                        ..
                    } = &mut *inner;
                    let doc = doc.as_mut().ok_or(())?;
                    run_op_on_document(registry, doc, layer_index, op_kind, &OpParams::default())
                        .map_err(|_| ())?;
                    *dirty_save = true;
                    if let Some(r) = renderer {
                        r.invalidate();
                    }
                    Prepared::Done
                }
                Some(Backend::Platform) => {
                    if !inner.registry.available(op_kind) {
                        return Err(());
                    }
                    let ops = inner.platform_ops.ok_or(())?;
                    let doc = inner.doc.as_ref().ok_or(())?;
                    let input = layer_input(doc, layer_index).map_err(|_| ())?;
                    Prepared::Platform { input, ops }
                }
                None => return Err(()),
            }
        };

        if let Prepared::Platform { input, ops } = prepared {
            let output = PlatformOp::for_kind(op_kind, ops)
                .run(input, &OpParams::default())
                .map_err(|_| ())?;
            let mut inner = mutex.lock().map_err(|_| ())?;
            let Inner {
                doc,
                renderer,
                dirty_save,
                ..
            } = &mut *inner;
            let doc = doc.as_mut().ok_or(())?;
            apply_output(doc, layer_index, output).map_err(|_| ())?;
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
