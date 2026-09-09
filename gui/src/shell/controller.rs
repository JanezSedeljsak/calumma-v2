use super::dialogs::confirm;
use super::export::{project_basename, save_bytes, save_text};
use super::{format_bytes, Catalog, LayerThumbCache, QuickColors, ShellPrefs, Theme};
use anyhow::Result;
use calumma_app::{pick_tool, Engine, LayerSummary, ProjectSummary, ToolBlock};
use calumma_core::{guide::GuideAxis, BlendMode, CropOverlayStyle, Tool};
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
    pub open_tabs: Vec<String>,
    pub active_project_id: Option<String>,
    pub load_generation: u64,
    pub settings_open: bool,
    pub new_project_open: bool,
    pub layer_settings_open: bool,
    pub layer_settings_index: usize,
    pub guides_open: bool,
    pub project_settings_open: bool,
    pub project_settings_id: String,
    pub project_settings_anchor_x: f32,
    pub project_settings_anchor_y: f32,
    pub guide_drag_pos: Option<(f32, f32)>,
    pub pinch_zoom: Option<f32>,
    pub tools_busy: bool,
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
            guides_open: false,
            project_settings_open: false,
            project_settings_id: String::new(),
            project_settings_anchor_x: 0.0,
            project_settings_anchor_y: 0.0,
            guide_drag_pos: None,
            pinch_zoom: None,
            tools_busy: false,
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

    pub fn refresh_recents(&self) -> Vec<ProjectSummary> {
        self.engine.borrow().list_recent_projects(32)
    }

    pub fn bump_load_generation(&mut self) -> u64 {
        self.load_generation += 1;
        self.load_generation
    }

    fn persist_tabs(&self) {
        self.engine
            .borrow()
            .persist_open_project_tabs(&self.open_tabs);
    }

    fn add_open_tab(&mut self, id: &str) {
        if !self.open_tabs.iter().any(|tab| tab == id) {
            self.open_tabs.push(id.to_string());
            self.persist_tabs();
        }
    }

    fn project_summary(&self, id: &str) -> Option<ProjectSummary> {
        self.engine.borrow().project_summary(id)
    }

    pub fn restore_open_tabs(&mut self) -> Option<ProjectSummary> {
        let stored = self.engine.borrow().open_project_tab_ids();
        let mut tabs: Vec<String> = stored
            .into_iter()
            .filter(|id| self.project_summary(id).is_some())
            .collect();
        if tabs.is_empty() {
            if let Some(id) = self.prefs.last_active_project_id.clone() {
                if self.project_summary(&id).is_some() {
                    tabs.push(id);
                }
            }
        }
        if tabs.is_empty() {
            return None;
        }
        self.open_tabs = tabs;
        self.persist_tabs();
        let active_id = self
            .prefs
            .last_active_project_id
            .clone()
            .filter(|id| self.open_tabs.iter().any(|tab| tab == id))
            .or_else(|| {
                self.open_tabs
                    .iter()
                    .filter_map(|id| self.project_summary(id))
                    .max_by_key(|summary| summary.opened_at)
                    .map(|summary| summary.id.clone())
            })
            .unwrap_or_else(|| self.open_tabs[0].clone());
        self.active_project_id = Some(active_id.clone());
        self.project_summary(&active_id)
    }

    pub fn prepare_switch_to(&mut self, id: &str) -> Option<ProjectSummary> {
        if self.active_project_id.as_deref() == Some(id)
            && self.editor_open
            && self.engine.borrow().has_project()
        {
            return None;
        }
        let summary = self.project_summary(id)?;
        self.add_open_tab(id);
        self.active_project_id = Some(id.to_string());
        self.prefs.set_last_active_project(Some(id));
        let _ = self.prefs.save();
        self.dismiss_modals();
        Some(summary)
    }

    pub fn load_project(&mut self, id: &str) -> Result<()> {
        self.engine.borrow_mut().open_project(id)?;
        self.engine.borrow_mut().prepare_editor();
        self.push_board_colors();
        self.load_quick_colors_from_engine();
        self.active_project_id = Some(id.to_string());
        self.prefs.set_last_active_project(Some(id));
        let _ = self.prefs.save();
        self.editor_open = true;
        Ok(())
    }

    fn reset_editor_state(&mut self) {
        self.active_project_id = None;
        self.prefs.set_last_active_project(None);
        let _ = self.prefs.save();
        self.editor_open = false;
        self.guides_open = false;
        self.guide_drag_pos = None;
        self.pinch_zoom = None;
    }

    pub fn close_project_tab(&mut self, id: &str) -> TabCloseResult {
        self.bump_load_generation();
        self.open_tabs.retain(|tab| tab != id);
        self.persist_tabs();
        if self.active_project_id.as_deref() != Some(id) {
            return TabCloseResult::Unchanged;
        }
        self.engine.borrow_mut().flush_save();
        self.engine.borrow_mut().close_project();
        if let Some(next) = self.open_tabs.last().cloned() {
            TabCloseResult::SwitchTo(next)
        } else {
            self.reset_editor_state();
            TabCloseResult::ShowLanding
        }
    }

    pub fn create_project(
        &mut self,
        name: &str,
        width: u32,
        height: u32,
        accent: Option<[u8; 3]>,
    ) -> Result<()> {
        let id = self
            .engine
            .borrow_mut()
            .create_project_with_accent(name, width, height, accent)?;
        self.add_open_tab(&id);
        self.active_project_id = Some(id.clone());
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
        self.add_open_tab(&id);
        self.active_project_id = Some(id.clone());
        self.prefs.set_last_active_project(Some(&id));
        self.prefs.save()?;
        self.engine.borrow_mut().prepare_editor();
        self.push_board_colors();
        self.load_quick_colors_from_engine();
        self.editor_open = true;
        Ok(())
    }

    pub fn delete_project(&mut self, id: &str) -> TabCloseResult {
        self.bump_load_generation();
        self.open_tabs.retain(|tab| tab != id);
        self.persist_tabs();
        let was_active = self.active_project_id.as_deref() == Some(id);
        if was_active {
            self.engine.borrow_mut().flush_save();
        }
        let _ = self.engine.borrow_mut().delete_project(id);
        if !was_active {
            return TabCloseResult::Unchanged;
        }
        self.active_project_id = None;
        if let Some(next) = self.open_tabs.last().cloned() {
            TabCloseResult::SwitchTo(next)
        } else {
            self.reset_editor_state();
            TabCloseResult::ShowLanding
        }
    }

    pub fn clear_recents(&mut self) -> Result<()> {
        self.bump_load_generation();
        self.engine.borrow_mut().delete_all_projects()?;
        self.open_tabs.clear();
        self.persist_tabs();
        self.reset_editor_state();
        Ok(())
    }

    pub fn open_settings(&mut self) {
        self.settings_open = true;
    }

    pub fn clear_recents_confirmed(
        &mut self,
        title: &str,
        message: &str,
        ok: &str,
        cancel: &str,
    ) -> Result<bool> {
        if !confirm(title, message, ok, cancel) {
            return Ok(false);
        }
        self.clear_recents()?;
        Ok(true)
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

    pub fn begin_guide_drag(&mut self, horizontal: bool, x: f32, y: f32, shift: bool) {
        let axis = if horizontal {
            GuideAxis::Horizontal
        } else {
            GuideAxis::Vertical
        };
        let mut engine = self.engine.borrow_mut();
        engine.set_shift_held(shift);
        engine.begin_guide_drag_from_ruler(axis, x, y);
        self.guide_drag_pos = Some((x, y));
    }

    pub fn update_guide_drag(&mut self, x: f32, y: f32, shift: bool) {
        let mut engine = self.engine.borrow_mut();
        engine.set_shift_held(shift);
        engine.update_guide_drag(x, y);
        self.guide_drag_pos = Some((x, y));
    }

    pub fn end_guide_drag(&mut self) {
        self.engine.borrow_mut().end_guide_drag();
        self.guide_drag_pos = None;
    }

    pub fn refresh_guide_shift(&mut self, shift: bool) {
        let mut engine = self.engine.borrow_mut();
        engine.set_shift_held(shift);
        if let Some((x, y)) = self.guide_drag_pos {
            engine.update_guide_drag(x, y);
        }
    }

    pub fn open_guides(&mut self) {
        self.guides_open = true;
    }

    pub fn add_guide_from_card(&mut self, horizontal: bool, text: &str) {
        let Ok(position) = text.trim().parse::<f32>() else {
            return;
        };
        self.engine.borrow_mut().add_guide(horizontal, position);
    }

    pub fn remove_guide(&mut self, index: usize) {
        self.engine.borrow_mut().remove_guide(index);
    }

    pub fn clear_guides(&mut self) {
        self.engine.borrow_mut().clear_guides();
    }

    pub fn set_guide_axis(&mut self, index: usize, horizontal: bool) {
        self.engine.borrow_mut().set_guide_axis(index, horizontal);
    }

    pub fn set_guide_offset(&mut self, index: usize, text: &str) {
        let Ok(position) = text.trim().parse::<f32>() else {
            return;
        };
        self.engine.borrow_mut().set_guide_position(index, position);
    }

    pub fn set_guide_color(&mut self, index: usize, palette_index: usize) {
        self.engine
            .borrow_mut()
            .set_guide_color(index, palette_index);
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

    pub fn open_layer_settings(&mut self, index: usize, _anchor_x: f32, _anchor_y: f32) {
        self.layer_settings_index = index;
        self.layer_settings_open = true;
    }

    pub fn open_project_settings(&mut self, id: &str, anchor_x: f32, anchor_y: f32) {
        self.project_settings_id = id.to_string();
        self.project_settings_anchor_x = anchor_x;
        self.project_settings_anchor_y = anchor_y;
        self.project_settings_open = true;
    }

    pub fn rename_open_project_settings(&mut self, name: &str) -> Result<()> {
        let id = self.project_settings_id.clone();
        self.engine.borrow_mut().rename_project(&id, name)
    }

    pub fn recolor_open_project_settings(&mut self, palette_index: usize) -> Result<()> {
        let id = self.project_settings_id.clone();
        let accent = calumma_core::project_color(palette_index);
        self.engine.borrow_mut().set_project_accent(&id, accent)
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

    pub fn set_layer_blend_mode(&mut self, index: usize, mode: i32) {
        if let Some(mode) = BlendMode::from_u32(mode as u32) {
            self.engine.borrow_mut().set_layer_blend_mode(index, mode);
        }
    }

    pub fn set_layer_filter(&mut self, index: usize, kind: i32, value: f32) {
        let mut adjustments = self.engine.borrow().layer_adjustments(index);
        match kind {
            0 => adjustments.brightness = value,
            1 => adjustments.contrast = value,
            2 => adjustments.vibrance = value,
            3 => adjustments.saturation = value,
            4 => adjustments.levels_gamma = value,
            _ => return,
        }
        self.engine
            .borrow_mut()
            .set_layer_adjustments(index, adjustments);
    }

    pub fn reset_layer_filters(&mut self, index: usize) {
        self.engine.borrow_mut().reset_layer_adjustments(index);
    }

    pub fn rename_layer(&mut self, index: usize, name: &str) -> bool {
        self.engine.borrow_mut().set_layer_name(index, name)
    }

    pub fn toggle_layer_clip(&mut self, index: usize) -> bool {
        let mut engine = self.engine.borrow_mut();
        if engine.is_layer_clipped(index) {
            engine.release_clipping_mask(index)
        } else {
            engine.create_clipping_mask(index)
        }
    }

    pub fn flatten_layer_clip(&mut self, index: usize) -> bool {
        self.engine.borrow_mut().flatten_clip(index)
    }

    pub fn merge_layer_down(&mut self, index: usize) -> bool {
        self.engine.borrow_mut().merge_layer_down(index)
    }

    pub fn reset_layer_transform(&mut self, index: usize) {
        self.engine.borrow_mut().reset_layer_transform(index);
    }

    pub fn move_layer_up(&mut self, index: usize) -> bool {
        if !self.engine.borrow_mut().move_layer_up(index) {
            return false;
        }
        self.layer_settings_index = index + 1;
        true
    }

    pub fn move_layer_down(&mut self, index: usize) -> bool {
        if !self.engine.borrow_mut().move_layer_down(index) {
            return false;
        }
        self.layer_settings_index = index.saturating_sub(1);
        true
    }

    pub fn move_layer_row(&mut self, from_row: usize, to_row: usize) -> bool {
        let count = self.engine.borrow().layer_count() as usize;
        if !self.engine.borrow_mut().move_layer_row(from_row, to_row) {
            return false;
        }
        let from = count - 1 - from_row;
        let to = count - 1 - to_row;
        let index = self.layer_settings_index;
        self.layer_settings_index = if index == from {
            to
        } else if from < to && index > from && index <= to {
            index - 1
        } else if from > to && index >= to && index < from {
            index + 1
        } else {
            index
        };
        self.thumb_cache.invalidate();
        true
    }

    pub fn rasterize_layer(&mut self, index: usize) -> bool {
        self.engine.borrow_mut().rasterize_layer(index)
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

    pub fn set_layer_hover(&mut self, index: usize) {
        self.layer_hover_index = Some(index);
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
