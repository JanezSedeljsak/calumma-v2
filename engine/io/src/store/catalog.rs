use super::{accent_or_seed, now_secs, pack_accent, ProjectListItem, ProjectStore, StoreError};
use calumma_core::limits::RECENT_PROJECTS_LIMIT;
use calumma_core::Document;
use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

impl ProjectStore {
    pub fn list_recent(&self, limit: usize) -> Result<Vec<ProjectListItem>, StoreError> {
        let limit = limit.min(RECENT_PROJECTS_LIMIT);
        let mut stmt = self.conn.prepare(
            "SELECT id, name, width, height, opened_at, created_at, accent FROM projects ORDER BY opened_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(ProjectListItem {
                id: row.get(0)?,
                name: row.get(1)?,
                width: row.get::<_, i64>(2)? as u32,
                height: row.get::<_, i64>(3)? as u32,
                opened_at: row.get(4)?,
                created_at: row.get(5)?,
                accent: accent_or_seed(row.get::<_, Option<i64>>(6)?, &row.get::<_, String>(0)?),
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn project(&self, id: &str) -> Result<ProjectListItem, StoreError> {
        self.conn
            .query_row(
                "SELECT id, name, width, height, opened_at, created_at, accent FROM projects WHERE id = ?1",
                params![id],
                |row| {
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
                },
            )
            .optional()?
            .ok_or(StoreError::NotFound)
    }

    pub fn open_project_tabs(&self) -> Result<Vec<String>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT project_id FROM open_project_tabs ORDER BY position ASC")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn set_open_project_tabs(&self, ids: &[String]) -> Result<(), StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM open_project_tabs", [])?;
        {
            let mut insert =
                tx.prepare("INSERT INTO open_project_tabs (position, project_id) VALUES (?1, ?2)")?;
            for (i, id) in ids.iter().enumerate() {
                insert.execute(params![i as i64, id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn create(&self, name: &str, width: u32, height: u32) -> Result<Document, StoreError> {
        self.create_with_accent(name, width, height, None)
    }

    pub fn create_with_accent(
        &self,
        name: &str,
        width: u32,
        height: u32,
        accent: Option<[u8; 3]>,
    ) -> Result<Document, StoreError> {
        let id = Uuid::new_v4().to_string();
        let ts = now_secs();
        let mut doc = Document::new(id.clone(), name, width, height);
        if let Some(accent) = accent {
            doc.accent = accent;
        }
        self.conn.execute(
            "INSERT INTO projects (id, name, width, height, created_at, opened_at, thumb, accent, guides) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, NULL)",
            params![
                id,
                name,
                width as i64,
                height as i64,
                ts,
                ts,
                pack_accent(doc.accent)
            ],
        )?;
        self.save(&mut doc)?;
        Ok(doc)
    }

    pub fn project_thumbnail(&self, id: &str) -> Result<Vec<u8>, StoreError> {
        let thumb: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT thumb FROM projects WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(StoreError::NotFound)?;
        thumb.ok_or(StoreError::NotFound)
    }

    pub fn rename(&self, id: &str, name: &str) -> Result<(), StoreError> {
        let n = self.conn.execute(
            "UPDATE projects SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub fn set_accent(&self, id: &str, accent: [u8; 3]) -> Result<(), StoreError> {
        let n = self.conn.execute(
            "UPDATE projects SET accent = ?1 WHERE id = ?2",
            params![pack_accent(accent), id],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<(), StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM tiles WHERE project_id = ?1", params![id])?;
        tx.execute("DELETE FROM layers WHERE project_id = ?1", params![id])?;
        tx.execute(
            "DELETE FROM open_project_tabs WHERE project_id = ?1",
            params![id],
        )?;
        let n = tx.execute("DELETE FROM projects WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_all_projects(&self) -> Result<(), StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM tiles", [])?;
        tx.execute("DELETE FROM layers", [])?;
        tx.execute("DELETE FROM open_project_tabs", [])?;
        tx.execute("DELETE FROM projects", [])?;
        tx.commit()?;
        let _ = self.conn.execute_batch("VACUUM;");
        Ok(())
    }
}
