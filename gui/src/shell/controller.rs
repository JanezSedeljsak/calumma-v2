use super::export::{project_basename, save_bytes, save_text};
use super::{format_bytes, Catalog, LayerThumbCache, QuickColors, ShellPrefs, Theme};
use anyhow::Result;
use calumma_app::{pick_tool, Engine, LayerSummary, ProjectSummary};
use calumma_core::{guide::GuideAxis, Tool};
use calumma_io::RasterFormat;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

pub struct AppController {
    pub engine: Rc<RefCell<Engine>>,
    pub prefs: ShellPrefs,
    pub theme: Theme,
    pub l10n: Catalog,
    pub root: PathBuf,
    pub editor_open: bool,
    pub settings_open: bool,
    pub new_project_open: bool,
    pub layer_settings_open: bool,
    pub layer_settings_index: usize,
    pub tools_busy: bool,
    pub layer_hover_index: Option<usize>,
    pub layer_hover_y: f32,
    pub thumb_cache: LayerThumbCache,
    pub quick_colors: QuickColors,
    pub toast_text: String,
    pub toast_visible: bool,
    pub toast_is_error: bool,
}

impl AppController {
    pub fn new(root: PathBuf) -> Result<Self> {
        let prefs = ShellPrefs::load();
        let theme = Theme::load(&root, prefs.is_dark())?;
        let l10n = Catalog::load(&prefs.language, &root)?;
        let engine = Rc::new(RefCell::new(Engine::new(None)?));
        Ok(Self {
            engine,
            prefs,
            theme,
            l10n,
            root,
            editor_open: false,
            settings_open: false,
            new_project_open: false,
            layer_settings_open: false,
            layer_settings_index: 0,
            tools_busy: false,
            layer_hover_index: None,
            layer_hover_y: 0.0,
            thumb_cache: LayerThumbCache::new(),
            quick_colors: QuickColors::new(),
            toast_text: String::new(),
            toast_visible: false,
            toast_is_error: false,
        })
    }

    pub fn any_modal_open(&self) -> bool {
        self.settings_open || self.new_project_open || self.layer_settings_open
    }

    pub fn dismiss_modals(&mut self) {
        self.settings_open = false;
        self.new_project_open = false;
        self.layer_settings_open = false;
    }

    pub fn push_board_colors(&mut self) {
        let colors = self.theme.board_colors();
        self.engine.borrow_mut().set_board_colors(colors);
    }

    pub fn load_quick_colors_from_engine(&mut self) {
        let engine = self.engine.borrow();
        self.quick_colors = QuickColors::load_from_engine(
            engine.ink_color(),
            engine.stroke_color(),
            engine.shape_fill_color(),
            engine.select_color(),
        );
    }

    pub fn push_quick_colors_to_engine(&mut self) {
        let colors = &self.quick_colors;
        self.engine.borrow_mut().push_quick_colors(
            colors.ink_rgba(),
            colors.slots[0],
            colors.slots[1],
            colors.slots[2],
        );
    }

    pub fn select_quick_color(&mut self, index: usize) {
        self.quick_colors.select(index);
        self.push_quick_colors_to_engine();
    }

    pub fn set_color_sb(&mut self, saturation: f32, brightness: f32) {
        self.quick_colors
            .set_saturation_brightness(saturation, brightness);
        self.push_quick_colors_to_engine();
    }

    pub fn set_color_hue(&mut self, hue: f32) {
        self.quick_colors.set_hue(hue);
        self.push_quick_colors_to_engine();
    }

    pub fn commit_color_hex(&mut self, text: &str) -> String {
        let hex = self.quick_colors.commit_hex(text);
        self.push_quick_colors_to_engine();
        hex
    }

    pub fn memory_label(&self) -> String {
        format_bytes(self.engine.borrow().resident_memory_bytes())
    }

    pub fn set_theme_dark(&mut self, dark: bool) -> Result<()> {
        self.prefs.set_theme_dark(dark);
        self.prefs.save()?;
        self.theme = Theme::load(&self.root, dark)?;
        self.push_board_colors();
        Ok(())
    }

    pub fn set_language(&mut self, language: &str) -> Result<()> {
        self.prefs.language = language.to_string();
        self.prefs.save()?;
        self.l10n = Catalog::load(language, &self.root)?;
        Ok(())
    }

    pub fn show_toast(&mut self, text: &str, is_error: bool) {
        self.toast_text = text.to_string();
        self.toast_is_error = is_error;
        self.toast_visible = true;
    }

    pub fn show_toast_key(&mut self, key: &str, is_error: bool) {
        self.show_toast(&self.l10n.get(key), is_error);
    }

    pub fn dismiss_toast(&mut self) {
        self.toast_visible = false;
    }

    pub fn toggle_layers_panel(&mut self) -> Result<()> {
        self.prefs.layers_panel_open = !self.prefs.layers_panel_open;
        self.prefs.save()?;
        Ok(())
    }

    pub fn refresh_recents(&self) -> Vec<ProjectSummary> {
        self.engine.borrow().list_recent_projects(32)
    }

    pub fn try_restore_last(&mut self) -> bool {
        let Some(id) = self.prefs.last_active_project_id.clone() else {
            return false;
        };
        if self.engine.borrow_mut().open_project(&id).is_err() {
            self.prefs.set_last_active_project(None);
            return false;
        }
        self.engine.borrow_mut().prepare_editor();
        self.push_board_colors();
        self.load_quick_colors_from_engine();
        self.editor_open = true;
        true
    }

    pub fn create_project(&mut self, name: &str, width: u32, height: u32) -> Result<()> {
        let id = self
            .engine
            .borrow_mut()
            .create_project(name, width, height)?;
        self.prefs.set_last_active_project(Some(&id));
        self.prefs.save()?;
        self.engine.borrow_mut().prepare_editor();
        self.push_board_colors();
        self.load_quick_colors_from_engine();
        self.editor_open = true;
        Ok(())
    }

    pub fn import_artwork(&mut self, bytes: &[u8]) -> Result<()> {
        let name = self.l10n.get("untitled");
        let id = self
            .engine
            .borrow_mut()
            .create_project_from_encoded(&name, bytes)?;
        self.prefs.set_last_active_project(Some(&id));
        self.prefs.save()?;
        self.engine.borrow_mut().prepare_editor();
        self.push_board_colors();
        self.load_quick_colors_from_engine();
        self.editor_open = true;
        Ok(())
    }

    pub fn open_project(&mut self, id: &str) -> Result<()> {
        self.engine.borrow_mut().open_project(id)?;
        self.prefs.set_last_active_project(Some(id));
        self.prefs.save()?;
        self.engine.borrow_mut().prepare_editor();
        self.push_board_colors();
        self.load_quick_colors_from_engine();
        self.editor_open = true;
        Ok(())
    }

    pub fn close_editor(&mut self) {
        self.engine.borrow_mut().close_project();
        self.prefs.set_last_active_project(None);
        let _ = self.prefs.save();
        self.editor_open = false;
    }

    pub fn delete_project(&mut self, id: &str) -> Result<()> {
        self.engine.borrow_mut().delete_project(id)?;
        if self.prefs.last_active_project_id.as_deref() == Some(id) {
            self.prefs.set_last_active_project(None);
            self.prefs.save()?;
            self.editor_open = false;
        }
        Ok(())
    }

    pub fn clear_recents(&mut self) -> Result<()> {
        self.engine.borrow_mut().delete_all_projects()?;
        self.prefs.set_last_active_project(None);
        self.prefs.save()?;
        self.editor_open = false;
        Ok(())
    }

    pub fn pick_tool(&mut self, tool: Tool) {
        pick_tool(&mut self.engine.borrow_mut(), tool);
    }

    pub fn set_brush_size_unit(&mut self, unit: f32) {
        self.engine.borrow_mut().set_brush_size_unit(unit);
    }

    pub fn commit_brush_size(&mut self, text: &str) {
        if let Ok(size) = text.trim().parse::<f32>() {
            self.engine.borrow_mut().set_brush_size(size);
        }
    }

    pub fn set_ink_opacity(&mut self, opacity: f32) {
        self.engine.borrow_mut().set_ink_opacity(opacity);
    }

    pub fn set_zoom_unit(&mut self, unit: f32) {
        self.engine.borrow_mut().set_zoom_unit(unit);
    }

    pub fn step_zoom(&mut self, zoom_in: bool) {
        self.engine.borrow_mut().step_zoom(zoom_in);
    }

    pub fn begin_guide_drag(&mut self, horizontal: bool, x: f32, y: f32) {
        let axis = if horizontal {
            GuideAxis::Horizontal
        } else {
            GuideAxis::Vertical
        };
        self.engine
            .borrow_mut()
            .begin_guide_drag_from_ruler(axis, x, y);
    }

    pub fn update_guide_drag(&mut self, x: f32, y: f32) {
        self.engine.borrow_mut().update_guide_drag(x, y);
    }

    pub fn end_guide_drag(&mut self) {
        self.engine.borrow_mut().end_guide_drag();
    }

    pub fn fit_to_view(&mut self) {
        self.engine.borrow_mut().fit_to_view();
    }

    pub fn pick_layer(&mut self, index: usize) {
        self.engine.borrow_mut().set_active_layer(index);
    }

    pub fn add_layer(&mut self) {
        self.engine.borrow_mut().add_layer();
    }

    pub fn undo(&mut self) {
        self.engine.borrow_mut().undo();
    }

    pub fn redo(&mut self) {
        self.engine.borrow_mut().redo();
    }

    pub fn save(&mut self) {
        self.engine.borrow_mut().flush_save();
    }

    pub fn can_undo(&self) -> bool {
        self.engine.borrow().can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.engine.borrow().can_redo()
    }

    pub fn export_basename(&self) -> String {
        let name = self
            .engine
            .borrow()
            .project_name()
            .unwrap_or_else(|| "export".to_string());
        project_basename(&name)
    }

    pub fn save_export_bytes(&self, bytes: &[u8], extension: &str) -> bool {
        let suggested = format!("{}.{}", self.export_basename(), extension);
        save_bytes(bytes, &suggested, extension)
    }

    pub fn save_export_text(&self, text: &str, extension: &str) -> bool {
        let suggested = format!("{}.{}", self.export_basename(), extension);
        save_text(text, &suggested, extension)
    }

    pub fn export_composite(&self, format: RasterFormat) -> Result<Vec<u8>> {
        self.engine.borrow().export_raster(format)
    }

    pub fn export_psd(&self) -> Result<Vec<u8>> {
        self.engine.borrow().export_psd_bytes()
    }

    pub fn export_svg(&self) -> Result<String> {
        self.engine.borrow().export_svg_string()
    }

    pub fn export_pdf(&self) -> Result<Vec<u8>> {
        self.engine.borrow().export_pdf_bytes()
    }

    pub fn export_layer_at(&self, index: usize) -> Result<(bool, Vec<u8>, Option<String>)> {
        let engine = self.engine.borrow();
        if engine.layer_is_vector(index) {
            let svg = engine.export_layer_svg(index)?;
            return Ok((true, Vec::new(), Some(svg)));
        }
        let bytes = engine.export_layer_raster(index, RasterFormat::Png)?;
        Ok((false, bytes, None))
    }

    pub fn layer_settings_summary(&self) -> Option<LayerSummary> {
        let engine = self.engine.borrow();
        engine
            .list_layers()
            .into_iter()
            .find(|layer| layer.index == self.layer_settings_index)
    }

    pub fn open_layer_settings(&mut self, index: usize) {
        self.layer_settings_index = index;
        self.layer_settings_open = true;
    }

    pub fn toggle_layer_visible(&mut self, index: usize) {
        let visible = self
            .engine
            .borrow()
            .list_layers()
            .into_iter()
            .find(|layer| layer.index == index)
            .map(|layer| layer.visible)
            .unwrap_or(true);
        self.engine.borrow_mut().set_layer_visible(index, !visible);
    }

    pub fn set_layer_visible(&mut self, index: usize, visible: bool) {
        self.engine.borrow_mut().set_layer_visible(index, visible);
    }

    pub fn set_layer_locked(&mut self, index: usize, locked: bool) {
        self.engine.borrow_mut().set_layer_locked(index, locked);
    }

    pub fn set_layer_opacity(&mut self, index: usize, opacity: f32) {
        self.engine.borrow_mut().set_layer_opacity(index, opacity);
    }

    pub fn duplicate_layer(&mut self, index: usize) -> bool {
        self.engine.borrow_mut().duplicate_layer(index)
    }

    pub fn remove_layer(&mut self, index: usize) -> bool {
        self.engine.borrow_mut().remove_layer(index)
    }

    pub fn smart_matte_label(&self) -> String {
        if self.engine.borrow().has_selection() {
            self.l10n.get("smartMatteDrawn")
        } else {
            self.l10n.get("smartMatte")
        }
    }

    pub fn can_run_smart_tools(&self) -> bool {
        !self.tools_busy && self.engine.borrow().active_layer_is_raster()
    }

    pub fn run_upscale(&mut self) -> Result<()> {
        self.tools_busy = true;
        let result = self.engine.borrow_mut().run_upscale();
        self.tools_busy = false;
        result
    }

    pub fn run_smart_matte(&mut self) -> Result<()> {
        self.tools_busy = true;
        let result = self.engine.borrow_mut().run_smart_matte();
        self.tools_busy = false;
        result
    }

    pub fn run_seam_carve(&mut self) -> Result<()> {
        self.tools_busy = true;
        let result = self.engine.borrow_mut().run_seam_carve_narrow();
        self.tools_busy = false;
        result
    }

    pub fn set_layer_hover(&mut self, index: usize, y: f32) {
        self.layer_hover_index = Some(index);
        self.layer_hover_y = y;
        self.engine.borrow_mut().set_hover_layer(Some(index));
    }

    pub fn clear_layer_hover(&mut self) {
        self.layer_hover_index = None;
        self.engine.borrow_mut().set_hover_layer(None);
    }

    pub fn commit_layer_bounds(&mut self, x: &str, y: &str, w: &str, h: &str) {
        let parse = |text: &str| text.trim().parse::<f32>().ok();
        let (Some(x), Some(y), Some(w), Some(h)) = (parse(x), parse(y), parse(w), parse(h)) else {
            return;
        };
        let Some(index) = self.engine.borrow().active_layer_index() else {
            return;
        };
        self.engine.borrow_mut().set_layer_bounds(index, x, y, w, h);
    }

    pub fn commit_canvas_size(&mut self, width: &str, height: &str) {
        let parse = |text: &str| text.trim().parse::<u32>().ok();
        let (Some(width), Some(height)) = (parse(width), parse(height)) else {
            return;
        };
        if width == 0 || height == 0 {
            return;
        }
        self.engine.borrow_mut().resize_document(width, height);
    }

    pub fn try_save_composite(&self, format: RasterFormat, ext: &str) -> Result<bool> {
        let bytes = self.export_composite(format)?;
        Ok(self.save_export_bytes(&bytes, ext))
    }

    pub fn try_save_psd(&self) -> Result<bool> {
        let bytes = self.export_psd()?;
        Ok(self.save_export_bytes(&bytes, "psd"))
    }

    pub fn try_save_svg(&self) -> Result<bool> {
        let text = self.export_svg()?;
        Ok(self.save_export_text(&text, "svg"))
    }

    pub fn try_save_pdf(&self) -> Result<bool> {
        let bytes = self.export_pdf()?;
        Ok(self.save_export_bytes(&bytes, "pdf"))
    }

    pub fn try_save_layer_export(&self, index: usize) -> Result<bool> {
        let (is_svg, bytes, svg) = self.export_layer_at(index)?;
        if is_svg {
            Ok(self.save_export_text(&svg.unwrap_or_default(), "svg"))
        } else {
            Ok(self.save_export_bytes(&bytes, "png"))
        }
    }

    pub fn complete_smart_op(&mut self, success_key: &str, failed_key: &str, result: Result<()>) {
        self.tools_busy = false;
        match result {
            Ok(()) => self.show_toast_key(success_key, false),
            Err(_) => self.show_toast_key(failed_key, true),
        }
    }

    pub fn notify_export(&mut self, result: Result<bool>) {
        if result.is_err() {
            self.show_toast_key("exportFailed", true);
        }
    }

    pub fn notify_layer_export(&mut self, result: Result<bool>) {
        if result.is_err() {
            self.show_toast_key("exportLayerFailed", true);
        }
    }
}

pub fn workspace_root() -> PathBuf {
    let mut dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for _ in 0..8 {
        if dir.join("design").join("icon.png").is_file() && dir.join("translations").is_dir() {
            return dir;
        }
        if !dir.pop() {
            break;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub type SharedController = Rc<RefCell<AppController>>;

pub fn shared(root: PathBuf) -> Result<SharedController> {
    Ok(Rc::new(RefCell::new(AppController::new(root)?)))
}
