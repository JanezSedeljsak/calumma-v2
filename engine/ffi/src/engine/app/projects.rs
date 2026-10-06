use super::Engine;
use anyhow::{Context, Result};
use calumma_core::{pack_rgb, unpack_rgb, BoardColors, Tool};
use calumma_io::ProjectListItem;

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

impl Engine {
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
}
