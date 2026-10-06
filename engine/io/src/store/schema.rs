use super::StoreError;
use rusqlite::Connection;

pub(super) fn prepare(conn: &Connection) -> Result<(), StoreError> {
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
            content_kind INTEGER NOT NULL DEFAULT 0,
            vector_data BLOB,
            opacity REAL NOT NULL DEFAULT 1.0,
            blend_mode INTEGER NOT NULL DEFAULT 0,
            adjustments BLOB,
            text_data BLOB,
            transform BLOB,
            locked INTEGER NOT NULL DEFAULT 0,
            clips_to TEXT,
            clip_invert INTEGER NOT NULL DEFAULT 0,
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
        CREATE TABLE IF NOT EXISTS open_project_tabs (
            position INTEGER PRIMARY KEY,
            project_id TEXT NOT NULL UNIQUE,
            FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        );
        DROP TABLE IF EXISTS workspace_projects;
        DROP TABLE IF EXISTS open_workspace_tabs;
        DROP TABLE IF EXISTS workspaces;
        ",
    )?;
    let _ = conn.execute(
        "ALTER TABLE layers ADD COLUMN clip_invert INTEGER NOT NULL DEFAULT 0",
        [],
    );
    cleanup_orphans(conn)
}

fn cleanup_orphans(conn: &Connection) -> Result<(), StoreError> {
    conn.execute_batch(
        "
        DELETE FROM tiles WHERE NOT EXISTS (
            SELECT 1 FROM layers
            WHERE layers.project_id = tiles.project_id
              AND layers.layer_id = tiles.layer_id
        );
        DELETE FROM layers WHERE NOT EXISTS (
            SELECT 1 FROM projects WHERE projects.id = layers.project_id
        );
        DELETE FROM open_project_tabs WHERE NOT EXISTS (
            SELECT 1 FROM projects WHERE projects.id = open_project_tabs.project_id
        );
        ",
    )?;
    Ok(())
}
