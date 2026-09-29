use super::dialogs::confirm;
use super::export::{project_basename, save_bytes, save_text};
use super::{format_bytes, Catalog, LayerThumbCache, QuickColors, ShellPrefs, Theme};
use anyhow::Result;
use calumma_app::{pick_tool, Engine, LayerSummary, ProjectSummary, ToolBlock};
use calumma_core::{guide::GuideAxis, BlendMode, CropOverlayStyle, Tool};
use calumma_io::RasterFormat;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

mod exports;
mod guides;
mod layers;
mod paths;
mod projects;

pub use paths::workspace_root;

pub struct AppController {
    pub engine: Rc<RefCell<Engine>>,
    pub prefs: ShellPrefs,
    pub theme: Theme,
    pub l10n: Catalog,
    pub root: PathBuf,
    pub editor_open: bool,
    pub open_tabs: Vec<String>,
    pub active_project_id: Option<String>,
    pub load_generation: u64,
    pub settings_open: bool,
    pub new_project_open: bool,
    pub layer_settings_open: bool,
    pub layer_settings_index: usize,
    pub layer_settings_dragging: bool,
    pub guides_open: bool,
    pub project_settings_open: bool,
    pub project_settings_id: String,
    pub project_settings_anchor_x: f32,
    pub project_settings_anchor_y: f32,
    pub guide_drag_pos: Option<(f32, f32)>,
    pub pinch_zoom: Option<f32>,
    pub layer_hover_index: Option<usize>,
    pub thumb_cache: LayerThumbCache,
    pub quick_colors: QuickColors,
    pub toast_text: String,
    pub toast_visible: bool,
    pub toast_is_error: bool,
}

pub enum TabCloseResult {
    Unchanged,
    SwitchTo(String),
    ShowLanding,
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
            open_tabs: Vec::new(),
            active_project_id: None,
            load_generation: 0,
            settings_open: false,
            new_project_open: false,
            layer_settings_open: false,
            layer_settings_index: 0,
            layer_settings_dragging: false,
            guides_open: false,
            project_settings_open: false,
            project_settings_id: String::new(),
            project_settings_anchor_x: 0.0,
            project_settings_anchor_y: 0.0,
            guide_drag_pos: None,
            pinch_zoom: None,
            layer_hover_index: None,
            thumb_cache: LayerThumbCache::new(),
            quick_colors: QuickColors::new(),
            toast_text: String::new(),
            toast_visible: false,
            toast_is_error: false,
        })
    }

    pub fn any_modal_open(&self) -> bool {
        self.settings_open
            || self.new_project_open
            || self.layer_settings_open
            || self.guides_open
            || self.project_settings_open
    }

    pub fn dismiss_modals(&mut self) {
        self.settings_open = false;
        self.new_project_open = false;
        self.layer_settings_open = false;
        self.guides_open = false;
        self.project_settings_open = false;
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
        format_bytes(self.engine.borrow().resident_memory_bytes(), &self.l10n)
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

    pub fn open_settings(&mut self) {
        self.settings_open = true;
    }

    pub fn pick_tool(&mut self, tool: Tool) {
        if self.engine.borrow().tool_block(tool) != ToolBlock::None {
            return;
        }
        pick_tool(&mut self.engine.borrow_mut(), tool);
    }

    pub fn announce_tool_block_if_any(&mut self) {
        let block = self.engine.borrow_mut().take_tool_block_notice();
        let key = match block {
            Some(ToolBlock::LayerLocked) => Some("toolBlockedLocked"),
            Some(ToolBlock::TextLayer) => Some("toolBlockedText"),
            Some(ToolBlock::VectorLayer) => Some("toolBlockedVector"),
            Some(ToolBlock::NoContent) => Some("toolBlockedEmpty"),
            _ => None,
        };
        if let Some(key) = key {
            self.show_toast_key(key, true);
        }
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

    pub fn pinch_started(&mut self) {
        self.pinch_zoom = Some(self.engine.borrow().zoom_factor());
    }

    pub fn pinch_updated(&mut self, x: f32, y: f32, scale: f32) {
        let Some(start) = self.pinch_zoom else {
            return;
        };
        self.engine
            .borrow_mut()
            .zoom_to(x, y, start * scale.max(0.01));
    }

    pub fn pinch_ended(&mut self) {
        self.pinch_zoom = None;
        self.engine.borrow_mut().end_camera_motion();
    }

    pub fn step_zoom(&mut self, zoom_in: bool) {
        self.engine.borrow_mut().step_zoom(zoom_in);
    }

    pub fn fit_to_view(&mut self) {
        self.engine.borrow_mut().fit_to_view();
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

    pub fn set_crop_aspect(&mut self, index: i32) {
        let ratio = match index {
            1 => Some(1.0),
            2 => Some(4.0 / 3.0),
            3 => Some(3.0 / 2.0),
            4 => Some(16.0 / 9.0),
            5 => Some(5.0 / 4.0),
            _ => None,
        };
        self.engine.borrow_mut().set_crop_aspect_lock(ratio);
    }

    pub fn set_crop_overlay(&mut self, index: i32) {
        if let Some(style) = CropOverlayStyle::from_u32(index as u32) {
            self.engine.borrow_mut().set_crop_overlay_style(style);
        }
    }

    pub fn commit_crop(&mut self) {
        self.engine.borrow_mut().commit_crop();
    }

    pub fn cancel_crop(&mut self) {
        self.engine.borrow_mut().cancel_crop();
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
}

pub type SharedController = Rc<RefCell<AppController>>;

pub fn shared(root: PathBuf) -> Result<SharedController> {
    Ok(Rc::new(RefCell::new(AppController::new(root)?)))
}
