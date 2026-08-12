use calumma_core::limits::WORKSPACES_LIMIT;
use calumma_core::palette;
use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use crate::store::{
    accent_or_seed, now_secs, pack_accent, ProjectListItem, ProjectStore, StoreError,
};

#[derive(Clone, Debug)]
pub struct WorkspaceListItem {
    pub id: String,
    pub name: String,
    pub accent: [u8; 3],
    pub active_project_id: Option<String>,
    pub opened_at: i64,
}

impl ProjectStore {
    pub(crate) fn migrate_workspaces(&self) -> Result<(), StoreError> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS workspaces (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                accent INTEGER,
                active_project_id TEXT,
                opened_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS workspace_projects (
                workspace_id TEXT NOT NULL,
                project_id TEXT NOT NULL,
                position INTEGER NOT NULL,
                PRIMARY KEY (workspace_id, project_id),
                FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE,
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS open_workspace_tabs (
                position INTEGER PRIMARY KEY,
                workspace_id TEXT NOT NULL UNIQUE,
                FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
            );
            ",
        )?;
        Ok(())
    }

    pub fn list_workspaces(&self, limit: usize) -> Result<Vec<WorkspaceListItem>, StoreError> {
        let limit = limit.min(WORKSPACES_LIMIT);
        let mut stmt = self.conn.prepare(
            "SELECT id, name, accent, active_project_id, opened_at FROM workspaces ORDER BY opened_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            let id: String = row.get(0)?;
            Ok(WorkspaceListItem {
                id: id.clone(),
                name: row.get(1)?,
                accent: accent_or_seed(row.get::<_, Option<i64>>(2)?, &id),
                active_project_id: row.get(3)?,
                opened_at: row.get(4)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn create_workspace(
        &self,
        name: &str,
        accent: Option<[u8; 3]>,
    ) -> Result<WorkspaceListItem, StoreError> {
        let id = Uuid::new_v4().to_string();
        let ts = now_secs();
        let accent = accent.unwrap_or_else(|| palette::color_for_seed(&id));
        self.conn.execute(
            "INSERT INTO workspaces (id, name, accent, active_project_id, opened_at) VALUES (?1, ?2, ?3, NULL, ?4)",
            params![id, name, pack_accent(accent), ts],
        )?;
        Ok(WorkspaceListItem {
            id,
            name: name.to_string(),
            accent,
            active_project_id: None,
            opened_at: ts,
        })
    }

    pub fn rename_workspace(&self, id: &str, name: &str) -> Result<(), StoreError> {
        let n = self.conn.execute(
            "UPDATE workspaces SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub fn set_workspace_accent(&self, id: &str, accent: [u8; 3]) -> Result<(), StoreError> {
        let n = self.conn.execute(
            "UPDATE workspaces SET accent = ?1 WHERE id = ?2",
            params![pack_accent(accent), id],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub fn delete_workspace(&self, id: &str) -> Result<(), StoreError> {
        let n = self
            .conn
            .execute("DELETE FROM workspaces WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub fn workspace(&self, id: &str) -> Result<WorkspaceListItem, StoreError> {
        self.conn
            .query_row(
                "SELECT id, name, accent, active_project_id, opened_at FROM workspaces WHERE id = ?1",
                params![id],
                |row| {
                    let wid: String = row.get(0)?;
                    Ok(WorkspaceListItem {
                        id: wid.clone(),
                        name: row.get(1)?,
                        accent: accent_or_seed(row.get::<_, Option<i64>>(2)?, &wid),
                        active_project_id: row.get(3)?,
                        opened_at: row.get(4)?,
                    })
                },
            )
            .optional()?
            .ok_or(StoreError::NotFound)
    }

    pub fn touch_workspace(&self, id: &str) -> Result<(), StoreError> {
        let n = self.conn.execute(
            "UPDATE workspaces SET opened_at = ?1 WHERE id = ?2",
            params![now_secs(), id],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub fn set_workspace_active_project(
        &self,
        workspace_id: &str,
        project_id: Option<&str>,
    ) -> Result<(), StoreError> {
        let n = self.conn.execute(
            "UPDATE workspaces SET active_project_id = ?1 WHERE id = ?2",
            params![project_id, workspace_id],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub fn add_project_to_workspace(
        &self,
        workspace_id: &str,
        project_id: &str,
    ) -> Result<(), StoreError> {
        self.workspace(workspace_id)?;
        let exists: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM projects WHERE id = ?1",
            params![project_id],
            |r| r.get(0),
        )?;
        if exists == 0 {
            return Err(StoreError::NotFound);
        }
        let next_pos: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(position), -1) + 1 FROM workspace_projects WHERE workspace_id = ?1",
                params![workspace_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        self.conn.execute(
            "INSERT OR IGNORE INTO workspace_projects (workspace_id, project_id, position) VALUES (?1, ?2, ?3)",
            params![workspace_id, project_id, next_pos],
        )?;
        let active: Option<String> = self.conn.query_row(
            "SELECT active_project_id FROM workspaces WHERE id = ?1",
            params![workspace_id],
            |r| r.get(0),
        )?;
        if active.is_none() {
            self.set_workspace_active_project(workspace_id, Some(project_id))?;
        }
        Ok(())
    }

    pub fn remove_project_from_workspace(
        &self,
        workspace_id: &str,
        project_id: &str,
    ) -> Result<(), StoreError> {
        let n = self.conn.execute(
            "DELETE FROM workspace_projects WHERE workspace_id = ?1 AND project_id = ?2",
            params![workspace_id, project_id],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        let active: Option<String> = self
            .conn
            .query_row(
                "SELECT active_project_id FROM workspaces WHERE id = ?1",
                params![workspace_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        if active.as_deref() == Some(project_id) {
            let next: Option<String> = self
                .conn
                .query_row(
                    "SELECT project_id FROM workspace_projects WHERE workspace_id = ?1 ORDER BY position ASC LIMIT 1",
                    params![workspace_id],
                    |r| r.get(0),
                )
                .optional()?;
            self.set_workspace_active_project(workspace_id, next.as_deref())?;
        }
        Ok(())
    }

    pub fn workspace_projects(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<ProjectListItem>, StoreError> {
        self.workspace(workspace_id)?;
        let mut stmt = self.conn.prepare(
            "SELECT p.id, p.name, p.width, p.height, p.opened_at, p.created_at, p.accent
             FROM workspace_projects wp
             JOIN projects p ON p.id = wp.project_id
             WHERE wp.workspace_id = ?1
             ORDER BY wp.position ASC",
        )?;
        let rows = stmt.query_map(params![workspace_id], |row| {
            let id: String = row.get(0)?;
            Ok(ProjectListItem {
                id: id.clone(),
                name: row.get(1)?,
                width: row.get::<_, i64>(2)? as u32,
                height: row.get::<_, i64>(3)? as u32,
                opened_at: row.get(4)?,
                created_at: row.get(5)?,
                accent: accent_or_seed(row.get::<_, Option<i64>>(6)?, &id),
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn open_workspace_tabs(&self) -> Result<Vec<String>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT workspace_id FROM open_workspace_tabs ORDER BY position ASC")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn set_open_workspace_tabs(&self, ids: &[String]) -> Result<(), StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM open_workspace_tabs", [])?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO open_workspace_tabs (position, workspace_id) VALUES (?1, ?2)",
            )?;
            for (i, id) in ids.iter().enumerate() {
                insert.execute(params![i as i64, id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn workspace_containing_project(
        &self,
        project_id: &str,
    ) -> Result<Option<WorkspaceListItem>, StoreError> {
        let id: Option<String> = self
            .conn
            .query_row(
                "SELECT workspace_id FROM workspace_projects WHERE project_id = ?1
                 ORDER BY workspace_id ASC LIMIT 1",
                params![project_id],
                |r| r.get(0),
            )
            .optional()?;
        match id {
            Some(wid) => Ok(Some(self.workspace(&wid)?)),
            None => Ok(None),
        }
    }

    pub fn create_workspace_for_project(
        &self,
        project_id: &str,
        name: &str,
        accent: [u8; 3],
    ) -> Result<WorkspaceListItem, StoreError> {
        let ws = self.create_workspace(name, Some(accent))?;
        self.add_project_to_workspace(&ws.id, project_id)?;
        self.set_workspace_active_project(&ws.id, Some(project_id))?;
        self.workspace(&ws.id)
    }
}
