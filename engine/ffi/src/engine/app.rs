use super::Inner;
use crate::surface::{CalmNativeSurface, CalmSurfaceKind};
use anyhow::{Context, Result};
use calumma_core::{
    brush_size_from_unit, brush_size_unit, guide::GuideAxis, pack_rgb, ruler::RulerTick,
    unpack_rgb, BoardColors, Tool, ToolBlock,
};
use calumma_io::ProjectListItem;
use parking_lot::Mutex;
use std::ffi::c_void;
use std::path::Path;
use std::sync::Arc;

pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub accent_rgb: u32,
    pub opened_at: i64,
}

impl From<&ProjectListItem> for ProjectSummary {
    fn from(item: &ProjectListItem) -> Self {
        Self {
            id: item.id.clone(),
            name: item.name.clone(),
            width: item.width,
            height: item.height,
            accent_rgb: pack_rgb(item.accent),
            opened_at: item.opened_at,
        }
    }
}

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
            let _ = inner.store.save(&mut doc);
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
        if let Some(doc) = &mut inner.doc {
            doc.resize_viewport(width as f32, height as f32, scale);
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

    pub fn create_project(&mut self, name: &str, width: u32, height: u32) -> Result<String> {
        self.create_project_with_accent(name, width, height, None)
    }

    pub fn create_project_with_accent(
        &mut self,
        name: &str,
        width: u32,
        height: u32,
        accent: Option<[u8; 3]>,
    ) -> Result<String> {
        let mut inner = self.inner.lock();
        inner.close_document();
        let doc = inner
            .store
            .create_with_accent(name, width, height, accent)
            .with_context(|| format!("creating project {name} at {width}x{height}"))?;
        let id = doc.id.clone();
        inner.install_document(doc);
        Ok(id)
    }

    pub fn create_project_from_encoded(&mut self, name: &str, bytes: &[u8]) -> Result<String> {
        let (width, height, rgba) =
            calumma_io::decode_encoded(bytes).with_context(|| "decoding artwork bytes")?;
        let mut inner = self.inner.lock();
        inner.close_document();
        let mut doc = inner
            .store
            .create(name, width, height)
            .with_context(|| format!("creating project {name} at {width}x{height}"))?;
        if !doc.place_image(&rgba, width, height) {
            anyhow::bail!("placing imported image into the first paint layer");
        }
        inner
            .store
            .save(&mut doc)
            .context("saving imported project")?;
        let id = doc.id.clone();
        inner.install_document(doc);
        Ok(id)
    }

    pub fn project_thumbnail_rgba(&self, id: &str) -> Option<(u32, u32, Vec<u8>)> {
        let inner = self.inner.lock();
        let png = inner.store.project_thumbnail(id).ok()?;
        calumma_io::decode_png_rgba(&png)
    }

    pub fn document_size(&self) -> Option<(u32, u32)> {
        let inner = self.inner.lock();
        inner.doc.as_ref().map(|doc| (doc.width, doc.height))
    }

    pub fn open_project(&mut self, id: &str) -> Result<()> {
        let mut inner = self.inner.lock();
        inner.close_document();
        let doc = inner
            .store
            .open_project(id)
            .with_context(|| format!("opening project {id}"))?;
        inner.install_document(doc);
        Ok(())
    }

    pub fn project_summary(&self, id: &str) -> Option<ProjectSummary> {
        self.inner
            .lock()
            .store
            .project(id)
            .ok()
            .map(|item| ProjectSummary::from(&item))
    }

    pub fn open_project_tab_ids(&self) -> Vec<String> {
        self.inner
            .lock()
            .store
            .open_project_tabs()
            .unwrap_or_default()
    }

    pub fn persist_open_project_tabs(&self, ids: &[String]) {
        let _ = self.inner.lock().store.set_open_project_tabs(ids);
    }

    pub fn close_project(&mut self) {
        self.inner.lock().close_document();
    }

    pub fn has_project(&self) -> bool {
        self.inner.lock().doc.is_some()
    }

    pub fn project_name(&self) -> Option<String> {
        self.inner.lock().doc.as_ref().map(|doc| doc.name.clone())
    }

    pub fn list_recent_projects(&self, limit: usize) -> Vec<ProjectSummary> {
        let inner = self.inner.lock();
        inner
            .store
            .list_recent(limit)
            .unwrap_or_default()
            .iter()
            .map(ProjectSummary::from)
            .collect()
    }

    pub fn delete_project(&mut self, id: &str) -> Result<()> {
        let mut inner = self.inner.lock();
        if inner.doc.as_ref().is_some_and(|doc| doc.id == id) {
            inner.close_document();
        }
        inner.store.delete(id).context("deleting project")?;
        Ok(())
    }

    pub fn rename_project(&mut self, id: &str, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            anyhow::bail!("project name is empty");
        }
        let mut inner = self.inner.lock();
        inner
            .store
            .rename(id, name)
            .with_context(|| format!("renaming project {id}"))?;
        if let Some(doc) = inner.doc.as_mut() {
            if doc.id == id {
                doc.name = name.to_string();
            }
        }
        Ok(())
    }

    pub fn set_project_accent(&mut self, id: &str, accent: [u8; 3]) -> Result<()> {
        let mut inner = self.inner.lock();
        inner
            .store
            .set_accent(id, accent)
            .with_context(|| format!("recoloring project {id}"))?;
        if let Some(doc) = inner.doc.as_mut() {
            if doc.id == id {
                doc.accent = accent;
            }
        }
        Ok(())
    }

    pub fn delete_all_projects(&mut self) -> Result<()> {
        let mut inner = self.inner.lock();
        inner.close_document();
        inner
            .store
            .delete_all_projects()
            .context("deleting all projects")?;
        Ok(())
    }

    pub fn prepare_editor(&mut self) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_tool(Tool::Pen);
            doc.fit_to_view();
            doc.board_colors = BoardColors::fallback(true);
        }
        inner.invalidate_renderer();
    }

    pub fn accent_hex(summary: &ProjectSummary) -> String {
        let rgb = unpack_rgb(summary.accent_rgb);
        format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
    }

    pub fn set_tool(&mut self, tool: Tool) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_tool(tool);
            let last_shape = doc.last_shape_tool;
            let last_select = doc.last_select_tool;
            inner.last_shape_tool = last_shape;
            inner.last_select_tool = last_select;
        } else if tool.is_shape() {
            inner.last_shape_tool = tool;
        } else if tool.is_selection() {
            inner.last_select_tool = tool;
        }
        inner.invalidate_renderer();
    }

    pub fn set_board_colors(&mut self, colors: BoardColors) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            if doc.board_colors != colors {
                doc.board_colors = colors;
                inner.invalidate_renderer();
            }
        }
    }

    pub fn pointer_down(&mut self, x: f32, y: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.pointer_down(x, y);
            inner.dirty_save = true;
            inner.invalidate_renderer();
        }
    }

    pub fn pointer_move(&mut self, x: f32, y: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            let changed_content = doc.pointer_move(x, y);
            if changed_content {
                inner.invalidate_renderer();
            } else {
                inner.invalidate_overlay();
            }
        }
    }

    pub fn pointer_up(&mut self, x: f32, y: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.pointer_up(x, y);
            inner.dirty_save = true;
            inner.invalidate_renderer();
        }
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        let mut inner = self.inner.lock();
        inner.pending_pan_dx += dx;
        inner.pending_pan_dy += dy;
        inner.invalidate_camera();
    }

    pub fn pan_scroll(&mut self, dx: f32, dy: f32, precise: bool) {
        let mut inner = self.inner.lock();
        inner.pending_scroll_dx += dx;
        inner.pending_scroll_dy += dy;
        if precise {
            inner.pending_scroll_precise = true;
        }
        inner.invalidate_camera();
    }

    pub fn zoom_scroll(&mut self, x: f32, y: f32, delta: f32, precise: bool) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            let (w, h) = (doc.width as f32, doc.height as f32);
            doc.camera.zoom_by_scroll(x, y, delta, precise, w, h);
            inner.invalidate_camera();
        }
    }

    pub fn zoom(&mut self, x: f32, y: f32, factor: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            let next = doc.camera.zoom * factor;
            let (w, h) = (doc.width as f32, doc.height as f32);
            doc.camera.zoom_at(x, y, next, w, h);
            inner.invalidate_camera();
        }
    }

    pub fn zoom_to(&mut self, x: f32, y: f32, zoom: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            let (w, h) = (doc.width as f32, doc.height as f32);
            doc.camera.zoom_at(x, y, zoom, w, h);
            inner.invalidate_camera();
        }
    }

    pub fn end_camera_motion(&mut self) {
        let mut inner = self.inner.lock();
        if let Some(renderer) = &mut inner.renderer {
            renderer.end_camera_motion();
        }
    }

    pub fn set_pointer_hover(&mut self, x: f32, y: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_pointer_hover(x, y);
            inner.invalidate_overlay();
        }
    }

    pub fn clear_pointer_hover(&mut self) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.clear_pointer_hover();
            inner.invalidate_overlay();
        }
    }

    pub fn brush_ring_visible(&self) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .is_some_and(|doc| doc.brush_ring().is_some())
    }

    pub fn ruler_ticks_x(&self) -> Vec<RulerTick> {
        let inner = self.inner.lock();
        inner
            .doc
            .as_ref()
            .map(|doc| doc.camera.ruler_ticks_x())
            .unwrap_or_default()
    }

    pub fn ruler_ticks_y(&self) -> Vec<RulerTick> {
        let inner = self.inner.lock();
        inner
            .doc
            .as_ref()
            .map(|doc| doc.camera.ruler_ticks_y())
            .unwrap_or_default()
    }

    pub fn camera_pan(&self) -> (f32, f32) {
        let inner = self.inner.lock();
        inner
            .doc
            .as_ref()
            .map(|doc| (doc.camera.pan_x, doc.camera.pan_y))
            .unwrap_or((0.0, 0.0))
    }

    pub fn begin_guide_drag_from_ruler(&mut self, axis: GuideAxis, x: f32, y: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            if doc.begin_guide_drag_from_ruler(axis, x, y) {
                inner.invalidate_overlay();
            }
        }
    }

    pub fn update_guide_drag(&mut self, x: f32, y: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            if doc.update_guide_drag(x, y) {
                inner.invalidate_overlay();
            }
        }
    }

    pub fn end_guide_drag(&mut self) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            if doc.end_guide_drag() {
                inner.dirty_save = true;
                inner.invalidate_overlay();
            }
        }
    }

    pub fn guide_axis_at(&self, x: f32, y: f32) -> Option<GuideAxis> {
        self.inner.lock().doc.as_ref().and_then(|doc| {
            doc.guide_at(x, y)
                .and_then(|index| doc.guides().get(index))
                .map(|guide| guide.axis)
        })
    }

    pub fn set_shift_held(&mut self, held: bool) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_shift_held(held);
        }
    }

    pub fn set_alt_held(&mut self, held: bool) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_alt_held(held);
        }
    }

    pub fn undo(&mut self) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.undo();
            inner.dirty_save = true;
            inner.invalidate_renderer();
        }
    }

    pub fn redo(&mut self) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.redo();
            inner.dirty_save = true;
            inner.invalidate_renderer();
        }
    }

    pub fn can_undo(&self) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .is_some_and(|doc| doc.history.can_undo())
    }

    pub fn can_redo(&self) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .is_some_and(|doc| doc.history.can_redo())
    }

    pub fn active_tool(&self) -> Option<Tool> {
        self.inner.lock().doc.as_ref().map(|doc| doc.tool)
    }

    pub fn tool_block(&self, tool: Tool) -> ToolBlock {
        self.inner
            .lock()
            .doc
            .as_ref()
            .map(|doc| doc.tool_block(tool))
            .unwrap_or(ToolBlock::None)
    }

    pub fn take_tool_block_notice(&mut self) -> Option<ToolBlock> {
        self.inner
            .lock()
            .doc
            .as_mut()
            .and_then(|doc| doc.take_tool_block_notice())
    }

    pub fn last_select_tool(&self) -> Tool {
        let inner = self.inner.lock();
        inner
            .doc
            .as_ref()
            .map(|doc| doc.last_select_tool)
            .unwrap_or(inner.last_select_tool)
    }

    pub fn last_shape_tool(&self) -> Tool {
        let inner = self.inner.lock();
        inner
            .doc
            .as_ref()
            .map(|doc| doc.last_shape_tool)
            .unwrap_or(inner.last_shape_tool)
    }

    pub fn zoom_unit(&self) -> f32 {
        let inner = self.inner.lock();
        inner
            .doc
            .as_ref()
            .map(|doc| {
                let (w, h) = (doc.width as f32, doc.height as f32);
                doc.camera.zoom_unit(w, h)
            })
            .unwrap_or(0.0)
    }

    pub fn zoom_factor(&self) -> f32 {
        let inner = self.inner.lock();
        inner.doc.as_ref().map(|doc| doc.camera.zoom).unwrap_or(1.0)
    }

    pub fn is_fit(&self) -> bool {
        let inner = self.inner.lock();
        inner
            .doc
            .as_ref()
            .map(|doc| doc.camera.is_fit(doc.width as f32, doc.height as f32))
            .unwrap_or(false)
    }

    pub fn set_zoom_unit(&mut self, unit: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            let (w, h) = (doc.width as f32, doc.height as f32);
            let zoom = doc.camera.zoom_from_unit(unit, w, h);
            doc.camera.zoom_to_center(zoom, w, h);
            inner.invalidate_camera();
        }
    }

    pub fn step_zoom(&mut self, zoom_in: bool) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            let (w, h) = (doc.width as f32, doc.height as f32);
            doc.camera.step_zoom(zoom_in, w, h);
            inner.invalidate_camera();
        }
    }

    pub fn fit_to_view(&mut self) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.fit_to_view();
            inner.invalidate_camera();
        }
    }

    pub fn brush_size(&self) -> f32 {
        self.inner
            .lock()
            .doc
            .as_ref()
            .map(|doc| doc.brush_size)
            .unwrap_or(0.0)
    }

    pub fn brush_size_unit(&self) -> f32 {
        brush_size_unit(self.brush_size())
    }

    pub fn set_brush_size(&mut self, size: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.brush_size = size.clamp(
                calumma_core::limits::BRUSH_SIZE_MIN,
                calumma_core::limits::BRUSH_SIZE_MAX,
            );
            inner.invalidate_overlay();
        }
    }

    pub fn set_brush_size_unit(&mut self, unit: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.brush_size = brush_size_from_unit(unit);
            inner.invalidate_overlay();
        }
    }

    pub fn ink_opacity(&self) -> f32 {
        self.inner
            .lock()
            .doc
            .as_ref()
            .map(|doc| doc.ink_opacity)
            .unwrap_or(1.0)
    }

    pub fn set_ink_opacity(&mut self, opacity: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_ink_opacity(opacity);
            inner.invalidate_overlay();
        }
    }

    pub fn flush_save(&mut self) {
        let mut inner = self.inner.lock();
        if let Some(mut doc) = inner.doc.take() {
            let _ = inner.store.save(&mut doc);
            inner.doc = Some(doc);
            inner.dirty_save = false;
        }
    }

    pub fn resident_memory_bytes(&self) -> u64 {
        self.inner.lock().resident_memory_bytes()
    }

    pub fn layer_count(&self) -> u32 {
        self.inner
            .lock()
            .doc
            .as_ref()
            .map(|doc| doc.layers.len() as u32)
            .unwrap_or(0)
    }

    pub fn list_layers(&self) -> Vec<LayerSummary> {
        let inner = self.inner.lock();
        let doc = match inner.doc.as_ref() {
            Some(doc) => doc,
            None => return Vec::new(),
        };
        let active = doc.active_layer;
        doc.layers
            .iter()
            .enumerate()
            .rev()
            .map(|(index, layer)| LayerSummary {
                index,
                name: layer.name.clone(),
                visible: layer.visible,
                locked: layer.locked,
                active: index == active,
                is_paper: layer.is_paper(),
                clipped: layer.clips_to.is_some(),
                clip_base: doc.is_layer_clip_base(index),
            })
            .collect()
    }

    pub fn set_active_layer(&mut self, index: usize) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_active_layer(index);
            inner.invalidate_renderer();
        }
    }

    pub fn add_layer(&mut self) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            let n = doc.layers.len();
            doc.add_layer(format!("Layer {}", n));
            inner.dirty_save = true;
            inner.invalidate_renderer();
        }
    }
}

pub struct LayerSummary {
    pub index: usize,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub active: bool,
    pub is_paper: bool,
    pub clipped: bool,
    pub clip_base: bool,
}

mod colors;
mod guides;
mod knobs;
mod layers;
mod ops;
mod shell;

pub use guides::GuideInfo;
