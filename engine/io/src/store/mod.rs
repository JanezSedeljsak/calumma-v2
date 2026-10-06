mod catalog;
mod load;
mod save;
mod schema;

use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

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

fn pack_accent(accent: [u8; 3]) -> i64 {
    ((accent[0] as i64) << 16) | ((accent[1] as i64) << 8) | accent[2] as i64
}

fn unpack_accent(packed: i64) -> [u8; 3] {
    [
        ((packed >> 16) & 0xFF) as u8,
        ((packed >> 8) & 0xFF) as u8,
        (packed & 0xFF) as u8,
    ]
}

fn accent_or_seed(packed: Option<i64>, id: &str) -> [u8; 3] {
    packed
        .map(unpack_accent)
        .unwrap_or_else(|| calumma_core::palette::color_for_seed(id))
}

pub struct ProjectStore {
    pub(crate) conn: Connection,
    path: PathBuf,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl ProjectStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path.as_ref())?;
        schema::prepare(&conn)?;
        Ok(Self {
            conn,
            path: path.as_ref().to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn default_path() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("Miw")
            .join("miw.sqlite")
    }
}
