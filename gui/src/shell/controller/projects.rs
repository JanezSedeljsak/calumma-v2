//! Projects and their tabs: opening, switching, creating, importing artwork, deleting, and the
//! project settings card.

use super::*;

pub(super) fn encoded_images(images: &[crate::shell::NamedImage]) -> Vec<(&str, &[u8])> {
    images
        .iter()
        .map(|image| (image.name.as_str(), image.bytes.as_slice()))
        .collect()
}

impl AppController {
    pub fn refresh_recents(&self) -> Vec<ProjectSummary> {
        self.engine.borrow().list_recent_projects(32)
    }

    pub fn bump_load_generation(&mut self) -> u64 {
        self.load_generation += 1;
        self.load_generation
    }

    pub(super) fn persist_tabs(&self) {
        self.engine
            .borrow()
            .persist_open_project_tabs(&self.open_tabs);
    }

    pub(super) fn add_open_tab(&mut self, id: &str) {
        if !self.open_tabs.iter().any(|tab| tab == id) {
            self.open_tabs.push(id.to_string());
            self.persist_tabs();
        }
    }

    pub(super) fn project_summary(&self, id: &str) -> Option<ProjectSummary> {
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

    pub(super) fn reset_editor_state(&mut self) {
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

    pub fn paste_images(&mut self, images: &[crate::shell::NamedImage]) {
        let encoded = encoded_images(images);
        let (_, outcome) = self.engine.borrow_mut().paste_encoded_images(&encoded);
        match outcome {
            calumma_core::paste::PasteOutcome::Failed => self.show_toast_key("pasteFailed", true),
            calumma_core::paste::PasteOutcome::Overflowing => {
                self.show_toast_key("pasteOverflows", false)
            }
            calumma_core::paste::PasteOutcome::Native => {}
        }
    }

    pub fn import_artworks(&mut self, images: &[crate::shell::NamedImage]) -> Result<()> {
        let name = self.l10n.get("untitled");
        let encoded = encoded_images(images);
        let id = self
            .engine
            .borrow_mut()
            .create_project_from_encoded_images(&name, &encoded)?;
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

    pub fn delete_project_confirmed(
        &mut self,
        id: &str,
        title: &str,
        message: &str,
        ok: &str,
        cancel: &str,
    ) -> Option<TabCloseResult> {
        if !confirm(title, message, ok, cancel) {
            return None;
        }
        Some(self.delete_project(id))
    }

    pub fn delete_project(&mut self, id: &str) -> TabCloseResult {
        self.bump_load_generation();
        self.open_tabs.retain(|tab| tab != id);
        self.persist_tabs();
        let was_active = self.active_project_id.as_deref() == Some(id);
        if was_active {
            self.engine.borrow_mut().flush_save();
        }
        let delete_err = self.engine.borrow_mut().delete_project(id).err();
        if let Some(err) = delete_err {
            eprintln!("miw: deleting project {id} failed: {err}");
            self.show_toast_key("deleteProjectFailed", true);
        }
        if self.prefs.last_active_project_id.as_deref() == Some(id) {
            self.prefs.set_last_active_project(None);
            let _ = self.prefs.save();
        }
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
}
