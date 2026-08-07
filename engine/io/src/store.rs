use calumma_core::limits::RECENT_PROJECTS_LIMIT;
use calumma_core::tile::{DirtyChannel, TileCoord, TILE_BYTES};
use calumma_core::{Document, Layer, LayerContent};
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use uuid::Uuid;

use crate::vector_blob;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("not found")]
    NotFound,
}

#[derive(Clone, Debug)]
pub struct ProjectListItem {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub opened_at: i64,
    pub created_at: i64,
}

pub struct ProjectStore {
    conn: Connection,
    path: PathBuf,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn placeholders(count: usize) -> String {
    (0..count)
        .map(|i| format!("?{}", i + 2))
        .collect::<Vec<_>>()
        .join(",")
}

impl ProjectStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path.as_ref())?;
        conn.execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                width INTEGER NOT NULL,
                height INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                opened_at INTEGER NOT NULL,
                thumb BLOB
            );
            CREATE TABLE IF NOT EXISTS layers (
                project_id TEXT NOT NULL,
                layer_id TEXT NOT NULL,
                name TEXT NOT NULL,
                visible INTEGER NOT NULL,
                z_index INTEGER NOT NULL,
                mask BLOB,
                content_kind INTEGER NOT NULL DEFAULT 0,
                vector_data BLOB,
                PRIMARY KEY (project_id, layer_id),
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS tiles (
                project_id TEXT NOT NULL,
                layer_id TEXT NOT NULL,
                tx INTEGER NOT NULL,
                ty INTEGER NOT NULL,
                pixels BLOB NOT NULL,
                PRIMARY KEY (project_id, layer_id, tx, ty),
                FOREIGN KEY (project_id, layer_id) REFERENCES layers(project_id, layer_id) ON DELETE CASCADE
            );
            ",
        )?;
        let store = Self {
            conn,
            path: path.as_ref().to_path_buf(),
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), StoreError> {
        let mut stmt = self.conn.prepare("PRAGMA table_info(layers)")?;
        let mut has_mask = false;
        let mut has_kind = false;
        let mut has_vector = false;
        let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
        for column in columns {
            match column?.as_str() {
                "mask" => has_mask = true,
                "content_kind" => has_kind = true,
                "vector_data" => has_vector = true,
                _ => {}
            }
        }
        if !has_mask {
            self.conn
                .execute("ALTER TABLE layers ADD COLUMN mask BLOB", [])?;
        }
        if !has_kind {
            self.conn.execute(
                "ALTER TABLE layers ADD COLUMN content_kind INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        if !has_vector {
            self.conn
                .execute("ALTER TABLE layers ADD COLUMN vector_data BLOB", [])?;
        }
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn list_recent(&self, limit: usize) -> Result<Vec<ProjectListItem>, StoreError> {
        let limit = limit.min(RECENT_PROJECTS_LIMIT);
        let mut stmt = self.conn.prepare(
            "SELECT id, name, width, height, opened_at, created_at FROM projects ORDER BY opened_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(ProjectListItem {
                id: row.get(0)?,
                name: row.get(1)?,
                width: row.get::<_, i64>(2)? as u32,
                height: row.get::<_, i64>(3)? as u32,
                opened_at: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn create(&self, name: &str, width: u32, height: u32) -> Result<Document, StoreError> {
        let id = Uuid::new_v4().to_string();
        let ts = now_secs();
        let mut doc = Document::new(id.clone(), name, width, height);
        self.conn.execute(
            "INSERT INTO projects (id, name, width, height, created_at, opened_at, thumb) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)",
            params![id, name, width as i64, height as i64, ts, ts],
        )?;
        self.save(&mut doc)?;
        Ok(doc)
    }

    pub fn open_project(&self, id: &str) -> Result<Document, StoreError> {
        let ts = now_secs();
        self.conn.execute(
            "UPDATE projects SET opened_at = ?1 WHERE id = ?2",
            params![ts, id],
        )?;
        let (name, width, height): (String, u32, u32) = self
            .conn
            .query_row(
                "SELECT name, width, height FROM projects WHERE id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get::<_, i64>(1)? as u32,
                        row.get::<_, i64>(2)? as u32,
                    ))
                },
            )
            .optional()?
            .ok_or(StoreError::NotFound)?;

        let mut doc = Document::new(id.to_string(), name, width, height);
        doc.layers.clear();

        let mut layer_stmt = self.conn.prepare(
            "SELECT layer_id, name, visible, mask, content_kind, vector_data FROM layers WHERE project_id = ?1 ORDER BY z_index ASC",
        )?;
        let layer_rows = layer_stmt.query_map(params![id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? != 0,
                row.get::<_, Option<Vec<u8>>>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Option<Vec<u8>>>(5)?,
            ))
        })?;

        let mask_len = (width as usize) * (height as usize);
        let mut tile_stmt = self
            .conn
            .prepare("SELECT tx, ty, pixels FROM tiles WHERE project_id = ?1 AND layer_id = ?2")?;

        for layer_row in layer_rows {
            let (layer_id, name, visible, mask, content_kind, vector_data) = layer_row?;
            let mut layer = if content_kind == 1 {
                let paths = vector_data
                    .as_deref()
                    .and_then(vector_blob::decode)
                    .unwrap_or_default();
                let mut layer = Layer::vector(name, paths);
                layer.id = layer_id.clone();
                layer
            } else {
                Layer::with_id(layer_id.clone(), name, width, height)
            };
            layer.visible = visible;
            if content_kind != 1 {
                layer.set_mask(mask.filter(|m| m.len() == mask_len));

                let tile_rows = tile_stmt.query_map(params![id, layer_id], |row| {
                    Ok((
                        row.get::<_, i64>(0)? as i32,
                        row.get::<_, i64>(1)? as i32,
                        row.get::<_, Vec<u8>>(2)?,
                    ))
                })?;
                for tile in tile_rows {
                    let (tx, ty, pixels) = tile?;
                    if pixels.len() != TILE_BYTES {
                        continue;
                    }
                    let coord = TileCoord { x: tx, y: ty };
                    if let Some(grid) = layer.tiles_mut() {
                        if let Some(dst) = grid.ensure_mut(coord) {
                            dst.copy_from_slice(&pixels);
                        }
                    }
                }
                layer.clear_dirty(DirtyChannel::Store);
            }
            doc.layers.push(layer);
        }

        if doc.layers.is_empty() {
            doc.layers
                .push(Layer::new(calumma_core::LAYER_ONE, width, height));
        }
        let missing_paper = !doc.layers.iter().any(Layer::is_paper);
        doc.ensure_paper_layer();
        doc.active_layer = doc
            .layers
            .iter()
            .position(|l| l.content.is_raster())
            .unwrap_or(0);
        if missing_paper {
            self.save(&mut doc)?;
        }
        Ok(doc)
    }

    pub fn save(&self, doc: &mut Document) -> Result<(), StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE projects SET name = ?1, width = ?2, height = ?3 WHERE id = ?4",
            params![doc.name, doc.width as i64, doc.height as i64, doc.id],
        )?;

        let live_ids: Vec<String> = doc.layers.iter().map(|l| l.id.clone()).collect();
        let prune = format!(
            "DELETE FROM layers WHERE project_id = ?1 AND layer_id NOT IN ({})",
            placeholders(live_ids.len())
        );
        let mut prune_args: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(live_ids.len() + 1);
        prune_args.push(&doc.id);
        for id in &live_ids {
            prune_args.push(id);
        }
        tx.execute(&prune, params_from_iter(prune_args))?;

        {
            let mut upsert_layer = tx.prepare(
                "INSERT INTO layers (project_id, layer_id, name, visible, z_index, mask, content_kind, vector_data) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(project_id, layer_id) DO UPDATE SET
                    name = excluded.name,
                    visible = excluded.visible,
                    z_index = excluded.z_index,
                    mask = excluded.mask,
                    content_kind = excluded.content_kind,
                    vector_data = excluded.vector_data",
            )?;
            let mut upsert_tile = tx.prepare(
                "INSERT INTO tiles (project_id, layer_id, tx, ty, pixels) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(project_id, layer_id, tx, ty) DO UPDATE SET pixels = excluded.pixels",
            )?;
            let mut delete_tile = tx.prepare(
                "DELETE FROM tiles WHERE project_id = ?1 AND layer_id = ?2 AND tx = ?3 AND ty = ?4",
            )?;

            for (z, layer) in doc.layers.iter().enumerate() {
                let (content_kind, vector_data, mask): (i64, Option<Vec<u8>>, Option<&[u8]>) =
                    match &layer.content {
                        LayerContent::Raster(_) => (0, None, layer.mask()),
                        LayerContent::Vector(paths) => {
                            (1, Some(vector_blob::encode(paths)), None)
                        }
                    };
                upsert_layer.execute(params![
                    doc.id,
                    layer.id,
                    layer.name,
                    if layer.visible { 1 } else { 0 },
                    z as i64,
                    mask,
                    content_kind,
                    vector_data
                ])?;

                let Some(grid) = layer.tiles() else {
                    continue;
                };
                for coord in grid.dirty_tiles(DirtyChannel::Store) {
                    match grid.pixels_ref(*coord) {
                        Some(pixels) => {
                            upsert_tile
                                .execute(params![doc.id, layer.id, coord.x, coord.y, pixels])?;
                        }
                        None => {
                            delete_tile.execute(params![doc.id, layer.id, coord.x, coord.y])?;
                        }
                    }
                }
            }
        }

        tx.commit()?;
        doc.clear_layer_dirty(DirtyChannel::Store);
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<(), StoreError> {
        let n = self
            .conn
            .execute("DELETE FROM projects WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub fn default_path() -> PathBuf {
        let home = std::env::var_os("HOME").unwrap_or_default();
        PathBuf::from(home).join("Library/Application Support/Calumma/calumma.sqlite")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn store() -> (tempfile::TempDir, ProjectStore) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.sqlite");
        let store = ProjectStore::open(&path).unwrap();
        (dir, store)
    }

    fn tile_rows(store: &ProjectStore, project: &str) -> i64 {
        store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM tiles WHERE project_id = ?1",
                params![project],
                |r| r.get(0),
            )
            .unwrap()
    }

    fn paint_index(doc: &Document) -> usize {
        doc.layers
            .iter()
            .position(|l| l.content.is_raster())
            .expect("raster layer")
    }

    #[test]
    fn create_open_round_trip() {
        let (_dir, store) = store();
        let mut doc = store.create("Demo", 128, 128).unwrap();
        assert!(doc.layers[0].is_paper());
        let i = paint_index(&doc);
        doc.layers[i]
            .tiles_mut()
            .unwrap()
            .set_pixel(3, 3, [1, 2, 3, 4]);
        store.save(&mut doc).unwrap();
        let loaded = store.open_project(&doc.id).unwrap();
        assert_eq!(loaded.name, "Demo");
        assert!(loaded.layers[0].is_paper());
        let i = paint_index(&loaded);
        assert_eq!(
            loaded.layers[i].tiles().unwrap().get_pixel(3, 3),
            [1, 2, 3, 4]
        );
    }

    #[test]
    fn layer_mask_round_trip() {
        let (_dir, store) = store();
        let mut doc = store.create("Masked", 32, 16).unwrap();
        let mut mask = vec![0u8; 32 * 16];
        mask[7] = 200;
        let i = paint_index(&doc);
        doc.layers[i].set_mask(Some(mask));
        store.save(&mut doc).unwrap();

        let loaded = store.open_project(&doc.id).unwrap();
        let i = paint_index(&loaded);
        let loaded_mask = loaded.layers[i].mask().unwrap();
        assert_eq!(loaded_mask.len(), 32 * 16);
        assert_eq!(loaded_mask[7], 200);

        let mut cleared = loaded;
        let i = paint_index(&cleared);
        cleared.layers[i].set_mask(None);
        store.save(&mut cleared).unwrap();
        let reopened = store.open_project(&cleared.id).unwrap();
        assert!(reopened.layers[paint_index(&reopened)].mask().is_none());
    }

    #[test]
    fn adds_mask_column_to_existing_database() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("legacy.sqlite");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE layers (
                    project_id TEXT NOT NULL,
                    layer_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    visible INTEGER NOT NULL,
                    z_index INTEGER NOT NULL,
                    PRIMARY KEY (project_id, layer_id)
                );",
            )
            .unwrap();
        }
        let store = ProjectStore::open(&path).unwrap();
        let mut doc = store.create("Legacy", 8, 8).unwrap();
        let i = paint_index(&doc);
        doc.layers[i].set_mask(Some(vec![9u8; 64]));
        store.save(&mut doc).unwrap();
        let loaded = store.open_project(&doc.id).unwrap();
        assert_eq!(
            loaded.layers[paint_index(&loaded)].mask(),
            Some([9u8; 64].as_slice())
        );
    }

    #[test]
    fn save_writes_only_dirty_tiles() {
        let (_dir, store) = store();
        let mut doc = store.create("Incremental", 1024, 1024).unwrap();
        let i = paint_index(&doc);
        let grid = doc.layers[i].tiles_mut().unwrap();
        grid.set_pixel(10, 10, [1, 1, 1, 255]);
        grid.set_pixel(600, 10, [2, 2, 2, 255]);
        grid.set_pixel(10, 600, [3, 3, 3, 255]);
        store.save(&mut doc).unwrap();
        assert_eq!(tile_rows(&store, &doc.id), 3);

        assert!(doc.layers[i]
            .dirty_tiles(DirtyChannel::Store)
            .unwrap()
            .is_empty());

        doc.layers[i]
            .tiles_mut()
            .unwrap()
            .set_pixel(11, 11, [9, 9, 9, 255]);
        assert_eq!(
            doc.layers[i]
                .dirty_tiles(DirtyChannel::Store)
                .unwrap()
                .len(),
            1
        );
        store.save(&mut doc).unwrap();

        let loaded = store.open_project(&doc.id).unwrap();
        let g = loaded.layers[paint_index(&loaded)].tiles().unwrap();
        assert_eq!(g.get_pixel(11, 11), [9, 9, 9, 255]);
        assert_eq!(g.get_pixel(600, 10), [2, 2, 2, 255]);
        assert_eq!(g.get_pixel(10, 600), [3, 3, 3, 255]);
    }

    #[test]
    fn cleared_tiles_are_deleted_from_disk() {
        let (_dir, store) = store();
        let mut doc = store.create("Cleared", 512, 512).unwrap();
        let i = paint_index(&doc);
        doc.layers[i]
            .tiles_mut()
            .unwrap()
            .set_pixel(5, 5, [1, 2, 3, 255]);
        store.save(&mut doc).unwrap();
        assert_eq!(tile_rows(&store, &doc.id), 1);

        doc.clear_active_layer();
        store.save(&mut doc).unwrap();
        assert_eq!(tile_rows(&store, &doc.id), 0);
        let loaded = store.open_project(&doc.id).unwrap();
        assert!(loaded.layers[paint_index(&loaded)]
            .tiles()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn removed_layers_are_pruned() {
        let (_dir, store) = store();
        let mut doc = store.create("Layers", 256, 256).unwrap();
        assert_eq!(doc.layers.len(), 2);
        doc.add_layer("Layer 2");
        let i = doc.layers.len() - 1;
        doc.layers[i]
            .tiles_mut()
            .unwrap()
            .set_pixel(1, 1, [4, 5, 6, 255]);
        store.save(&mut doc).unwrap();
        assert_eq!(store.open_project(&doc.id).unwrap().layers.len(), 3);

        assert!(doc.remove_layer(i));
        store.save(&mut doc).unwrap();
        let loaded = store.open_project(&doc.id).unwrap();
        assert_eq!(loaded.layers.len(), 2);
        assert_eq!(tile_rows(&store, &doc.id), 0);
    }

    #[test]
    fn paper_layer_round_trip() {
        let (_dir, store) = store();
        let doc = store.create("Paper", 64, 48).unwrap();
        let loaded = store.open_project(&doc.id).unwrap();
        assert!(loaded.layers[0].is_paper());
        assert!(loaded.layers[0].visible);
        let paths = loaded.layers[0].content.paths().unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].fill);
        assert_eq!(paths[0].color, [255, 255, 255, 255]);
    }

    #[test]
    fn layer_reorder_persists_z_index() {
        let (_dir, store) = store();
        let mut doc = store.create("Order", 128, 128).unwrap();
        doc.add_layer("Second");
        let second = doc.layers[2].id.clone();
        doc.layers.swap(1, 2);
        store.save(&mut doc).unwrap();
        let loaded = store.open_project(&doc.id).unwrap();
        assert_eq!(loaded.layers[1].id, second);
    }
}
