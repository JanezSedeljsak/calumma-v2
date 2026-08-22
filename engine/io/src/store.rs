use calumma_core::limits::{PROJECT_THUMB_MAX_SIDE, RECENT_PROJECTS_LIMIT};
use calumma_core::tile::{self, DirtyChannel, TileCoord, TILE_BYTES};
use calumma_core::{BlendMode, Document, Layer, LayerContent};
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use uuid::Uuid;

use crate::adjustments_blob;
use crate::guides_blob;
use crate::text_blob;
use crate::transform_blob;
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
    pub accent: [u8; 3],
}

pub(crate) fn pack_accent(accent: [u8; 3]) -> i64 {
    ((accent[0] as i64) << 16) | ((accent[1] as i64) << 8) | accent[2] as i64
}

pub(crate) fn unpack_accent(packed: i64) -> [u8; 3] {
    [
        ((packed >> 16) & 0xFF) as u8,
        ((packed >> 8) & 0xFF) as u8,
        (packed & 0xFF) as u8,
    ]
}

pub(crate) fn accent_or_seed(packed: Option<i64>, id: &str) -> [u8; 3] {
    packed
        .map(unpack_accent)
        .unwrap_or_else(|| calumma_core::palette::color_for_seed(id))
}

pub struct ProjectStore {
    pub(crate) conn: Connection,
    path: PathBuf,
}

/// What `layers.content_kind` means on disk. Text is 2; a row claiming to be text whose blob
/// will not decode falls back to raster so a damaged or newer file loses the run, not the
/// whole project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LayerKind {
    Raster,
    Vector,
    Text,
}

/// The content-shaped columns of one `layers` row. Which blob a layer writes and whether it
/// carries a mask both follow from its content, so the three answers are decided together.
struct LayerColumns<'a> {
    content_kind: i64,
    vector_data: Option<Vec<u8>>,
    text_data: Option<Vec<u8>>,
    mask: Option<&'a [u8]>,
}

impl<'a> LayerColumns<'a> {
    fn of(layer: &'a Layer) -> Self {
        match &layer.content {
            LayerContent::Raster(_) => Self {
                content_kind: 0,
                vector_data: None,
                text_data: None,
                mask: layer.mask(),
            },
            LayerContent::Vector(paths) => Self {
                content_kind: 1,
                vector_data: Some(vector_blob::encode(paths)),
                text_data: None,
                mask: None,
            },
            LayerContent::Text { run, .. } => Self {
                content_kind: 2,
                vector_data: None,
                text_data: Some(text_blob::encode(run)),
                mask: layer.mask(),
            },
        }
    }
}

pub(crate) fn now_secs() -> i64 {
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
                thumb BLOB,
                accent INTEGER,
                guides BLOB
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
                opacity REAL NOT NULL DEFAULT 1.0,
                blend_mode INTEGER NOT NULL DEFAULT 0,
                adjustments BLOB,
                text_data BLOB,
                transform BLOB,
                locked INTEGER NOT NULL DEFAULT 0,
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
        self.migrate_projects()?;
        self.migrate_layers()?;
        self.migrate_workspaces()
    }

    fn migrate_projects(&self) -> Result<(), StoreError> {
        let mut stmt = self.conn.prepare("PRAGMA table_info(projects)")?;
        let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
        let mut has_accent = false;
        let mut has_guides = false;
        for column in columns {
            match column?.as_str() {
                "accent" => has_accent = true,
                "guides" => has_guides = true,
                _ => {}
            }
        }
        if !has_accent {
            self.conn
                .execute("ALTER TABLE projects ADD COLUMN accent INTEGER", [])?;
        }
        if !has_guides {
            self.conn
                .execute("ALTER TABLE projects ADD COLUMN guides BLOB", [])?;
        }
        Ok(())
    }

    fn migrate_layers(&self) -> Result<(), StoreError> {
        let mut stmt = self.conn.prepare("PRAGMA table_info(layers)")?;
        let mut has_mask = false;
        let mut has_kind = false;
        let mut has_vector = false;
        let mut has_opacity = false;
        let mut has_blend_mode = false;
        let mut has_adjustments = false;
        let mut has_text = false;
        let mut has_transform = false;
        let mut has_locked = false;
        let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
        for column in columns {
            match column?.as_str() {
                "mask" => has_mask = true,
                "content_kind" => has_kind = true,
                "vector_data" => has_vector = true,
                "opacity" => has_opacity = true,
                "blend_mode" => has_blend_mode = true,
                "adjustments" => has_adjustments = true,
                "text_data" => has_text = true,
                "transform" => has_transform = true,
                "locked" => has_locked = true,
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
        if !has_opacity {
            self.conn.execute(
                "ALTER TABLE layers ADD COLUMN opacity REAL NOT NULL DEFAULT 1.0",
                [],
            )?;
        }
        if !has_blend_mode {
            self.conn.execute(
                "ALTER TABLE layers ADD COLUMN blend_mode INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        if !has_adjustments {
            self.conn
                .execute("ALTER TABLE layers ADD COLUMN adjustments BLOB", [])?;
        }
        if !has_text {
            self.conn
                .execute("ALTER TABLE layers ADD COLUMN text_data BLOB", [])?;
        }
        if !has_transform {
            self.conn
                .execute("ALTER TABLE layers ADD COLUMN transform BLOB", [])?;
        }
        if !has_locked {
            self.conn.execute(
                "ALTER TABLE layers ADD COLUMN locked INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

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

    pub fn create(&self, name: &str, width: u32, height: u32) -> Result<Document, StoreError> {
        let id = Uuid::new_v4().to_string();
        let ts = now_secs();
        let mut doc = Document::new(id.clone(), name, width, height);
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

    pub fn open_project(&self, id: &str) -> Result<Document, StoreError> {
        let ts = now_secs();
        self.conn.execute(
            "UPDATE projects SET opened_at = ?1 WHERE id = ?2",
            params![ts, id],
        )?;
        let (name, width, height, accent, guides): (String, u32, u32, [u8; 3], Option<Vec<u8>>) =
            self.conn
                .query_row(
                    "SELECT name, width, height, accent, guides FROM projects WHERE id = ?1",
                    params![id],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get::<_, i64>(1)? as u32,
                            row.get::<_, i64>(2)? as u32,
                            accent_or_seed(row.get::<_, Option<i64>>(3)?, id),
                            row.get::<_, Option<Vec<u8>>>(4)?,
                        ))
                    },
                )
                .optional()?
                .ok_or(StoreError::NotFound)?;

        let mut doc = Document::new(id.to_string(), name, width, height);
        doc.accent = accent;
        doc.set_guides(
            guides
                .as_deref()
                .and_then(guides_blob::decode)
                .unwrap_or_default(),
        );
        doc.layers.clear();

        let mut layer_stmt = self.conn.prepare(
            "SELECT layer_id, name, visible, mask, content_kind, vector_data, opacity, blend_mode, adjustments, text_data, transform, locked FROM layers WHERE project_id = ?1 ORDER BY z_index ASC",
        )?;
        let layer_rows = layer_stmt.query_map(params![id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? != 0,
                row.get::<_, Option<Vec<u8>>>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Option<Vec<u8>>>(5)?,
                row.get::<_, f64>(6)? as f32,
                row.get::<_, i64>(7)? as u32,
                row.get::<_, Option<Vec<u8>>>(8)?,
                row.get::<_, Option<Vec<u8>>>(9)?,
                row.get::<_, Option<Vec<u8>>>(10)?,
                row.get::<_, i64>(11)? != 0,
            ))
        })?;

        let mask_len = (width as usize) * (height as usize);
        let mut solid_tiles: FxHashMap<[u8; 4], Arc<Vec<u8>>> = FxHashMap::default();
        let mut tile_stmt = self
            .conn
            .prepare("SELECT tx, ty, pixels FROM tiles WHERE project_id = ?1 AND layer_id = ?2")?;

        for layer_row in layer_rows {
            let (
                layer_id,
                name,
                visible,
                mask,
                content_kind,
                vector_data,
                opacity,
                blend_mode,
                adjustments,
                text_data,
                transform,
                locked,
            ) = layer_row?;
            let decoded_run = text_data.as_deref().and_then(text_blob::decode);
            let kind = match (content_kind, &decoded_run) {
                (1, _) => LayerKind::Vector,
                (2, Some(_)) => LayerKind::Text,
                _ => LayerKind::Raster,
            };
            let mut layer = match kind {
                LayerKind::Vector => {
                    let paths = vector_data
                        .as_deref()
                        .and_then(vector_blob::decode)
                        .unwrap_or_default();
                    let mut layer = Layer::vector(name, paths);
                    layer.id = layer_id.clone();
                    layer
                }
                LayerKind::Text => {
                    let run = decoded_run.unwrap_or_default();
                    let mut layer = Layer::text(name, run, width, height);
                    layer.id = layer_id.clone();
                    layer
                }
                LayerKind::Raster => Layer::with_id(layer_id.clone(), name, width, height),
            };
            layer.visible = visible;
            layer.opacity = opacity.clamp(0.0, 1.0);
            layer.blend_mode = BlendMode::from_u32(blend_mode).unwrap_or_default();
            layer.adjustments = adjustments.as_deref().and_then(adjustments_blob::decode);
            layer.transform = transform.as_deref().and_then(transform_blob::decode);
            layer.locked = locked;
            if kind != LayerKind::Vector {
                layer.set_mask(mask.filter(|m| m.len() == mask_len));
            }
            if kind == LayerKind::Text {
                layer.clear_dirty(DirtyChannel::Store);
            }
            if kind == LayerKind::Raster {
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
                    let Some(grid) = layer.tiles_mut() else {
                        continue;
                    };
                    // Paper comes back as hundreds of identical white blobs; giving them one
                    // shared allocation costs a scan that mixed tiles abandon immediately.
                    match tile::uniform_color(&pixels) {
                        Some(color) => {
                            let shared = solid_tiles
                                .entry(color)
                                .or_insert_with(|| Arc::new(pixels))
                                .clone();
                            grid.insert_shared(coord, shared);
                        }
                        None => {
                            if let Some(dst) = grid.ensure_mut(coord) {
                                dst.copy_from_slice(&pixels);
                            }
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
            .position(|l| l.content.is_raster() && !l.is_paper())
            .or_else(|| doc.layers.iter().position(|l| l.content.is_raster()))
            .unwrap_or(0);
        if missing_paper {
            self.save(&mut doc)?;
        }
        Ok(doc)
    }

    pub fn save(&self, doc: &mut Document) -> Result<(), StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE projects SET name = ?1, width = ?2, height = ?3, accent = ?4, guides = ?5 WHERE id = ?6",
            params![
                doc.name,
                doc.width as i64,
                doc.height as i64,
                pack_accent(doc.accent),
                guides_blob::encode(doc.guides()),
                doc.id
            ],
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
                "INSERT INTO layers (project_id, layer_id, name, visible, z_index, mask, content_kind, vector_data, opacity, blend_mode, adjustments, text_data, transform, locked) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                 ON CONFLICT(project_id, layer_id) DO UPDATE SET
                    name = excluded.name,
                    visible = excluded.visible,
                    z_index = excluded.z_index,
                    mask = excluded.mask,
                    content_kind = excluded.content_kind,
                    vector_data = excluded.vector_data,
                    opacity = excluded.opacity,
                    blend_mode = excluded.blend_mode,
                    adjustments = excluded.adjustments,
                    text_data = excluded.text_data,
                    transform = excluded.transform,
                    locked = excluded.locked",
            )?;
            let mut upsert_tile = tx.prepare(
                "INSERT INTO tiles (project_id, layer_id, tx, ty, pixels) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(project_id, layer_id, tx, ty) DO UPDATE SET pixels = excluded.pixels",
            )?;
            let mut delete_tile = tx.prepare(
                "DELETE FROM tiles WHERE project_id = ?1 AND layer_id = ?2 AND tx = ?3 AND ty = ?4",
            )?;

            for (z, layer) in doc.layers.iter().enumerate() {
                let LayerColumns {
                    content_kind,
                    vector_data,
                    text_data,
                    mask,
                } = LayerColumns::of(layer);
                let adjustments = layer.adjustments.as_ref().map(adjustments_blob::encode);
                let transform = layer.transform.as_ref().map(transform_blob::encode);
                upsert_layer.execute(params![
                    doc.id,
                    layer.id,
                    layer.name,
                    if layer.visible { 1 } else { 0 },
                    z as i64,
                    mask,
                    content_kind,
                    vector_data,
                    layer.opacity as f64,
                    layer.blend_mode.as_u32() as i64,
                    adjustments,
                    text_data,
                    transform,
                    layer.locked as i64
                ])?;

                // A text layer's tiles are a cache of its run, so the run is all that is
                // written — re-rasterizing on open costs a millisecond and saves storing a
                // bitmap of every headline in the project.
                if layer.is_text() {
                    continue;
                }
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
        self.write_project_thumbnail(doc)?;
        Ok(())
    }

    fn write_project_thumbnail(&self, doc: &Document) -> Result<(), StoreError> {
        let (w, h, rgba) = doc.composite_thumbnail(PROJECT_THUMB_MAX_SIDE);
        let png = crate::encode_png_rgba(&rgba, w, h).map_err(|e| StoreError::Io(e.into()))?;
        self.conn.execute(
            "UPDATE projects SET thumb = ?1 WHERE id = ?2",
            params![png, doc.id],
        )?;
        Ok(())
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
        let n = self
            .conn
            .execute("DELETE FROM projects WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub fn delete_all_projects(&self) -> Result<(), StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM open_workspace_tabs", [])?;
        tx.execute("DELETE FROM workspaces", [])?;
        tx.execute("DELETE FROM projects", [])?;
        tx.commit()?;
        Ok(())
    }

    pub fn default_path() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("Calumma")
            .join("calumma.sqlite")
    }
}
