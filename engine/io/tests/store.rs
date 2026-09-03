use calumma_core::layer::BlendMode;
use calumma_core::tile::TILE_SIZE;
use calumma_core::{DirtyChannel, Document, Guide, GuideAxis, LayerTransform};
use calumma_io::*;
use rusqlite::{params, Connection};
use tempfile::tempdir;

fn paper_tile_count(width: u32, height: u32) -> i64 {
    let tiles_x = width.div_ceil(TILE_SIZE) as i64;
    let tiles_y = height.div_ceil(TILE_SIZE) as i64;
    tiles_x * tiles_y
}

fn store() -> (tempfile::TempDir, ProjectStore) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("t.sqlite");
    let store = ProjectStore::open(&path).unwrap();
    (dir, store)
}

fn tile_rows(store: &ProjectStore, project: &str) -> i64 {
    Connection::open(store.path())
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM tiles WHERE project_id = ?1",
            params![project],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
}

fn paint_index(doc: &Document) -> usize {
    doc.layers
        .iter()
        .position(|l| l.content.is_raster() && !l.is_paper())
        .expect("raster layer")
}

/// A pasted image bigger than the paper stores tiles outside the document rectangle. A fresh
/// grid holds only the document, so unless the loader widens it first, reopening the project is
/// exactly where that overflow disappears — the same silent data loss the crop used to cause,
/// just deferred to the next launch.
#[test]
fn an_overflowing_layer_survives_a_reopen() {
    let (_dir, store) = store();
    let mut doc = store.create("Overflow", 128, 128).unwrap();
    let rgba = [7u8, 8, 9, 255].repeat(400 * 400);
    assert_eq!(
        doc.paste_image_as_layer("Pasted", &rgba, 400, 400),
        calumma_core::paste::PasteOutcome::Overflowing
    );
    let before = doc.layers[doc.active_layer]
        .content_bounds()
        .expect("the pasted layer has a box");
    assert!(before.0 < 0.0, "it hangs off the top-left: {before:?}");
    store.save(&mut doc).unwrap();

    let reopened = store.open_project(&doc.id).unwrap();
    let pasted = reopened
        .layers
        .iter()
        .find(|l| l.name == "Pasted")
        .expect("the pasted layer came back");
    assert_eq!(pasted.content_bounds(), Some(before));
    assert_eq!(
        (reopened.width, reopened.height),
        (128, 128),
        "and the canvas is still the canvas"
    );
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
fn layer_effects_round_trip() {
    let (_dir, store) = store();
    let mut doc = store.create("Effects", 32, 32).unwrap();
    let i = paint_index(&doc);
    doc.set_layer_opacity(i, 0.4);
    doc.set_layer_blend_mode(i, BlendMode::Multiply);
    doc.set_layer_adjustments(
        i,
        calumma_core::Adjustments {
            brightness: 0.3,
            ..Default::default()
        },
    );
    store.save(&mut doc).unwrap();
    let loaded = store.open_project(&doc.id).unwrap();
    let i = paint_index(&loaded);
    assert!((loaded.layers[i].opacity - 0.4).abs() < 1e-4);
    assert_eq!(loaded.layers[i].blend_mode, BlendMode::Multiply);
    let adj = loaded.layers[i].adjustments.expect("adjustments persisted");
    assert!((adj.brightness - 0.3).abs() < 1e-4);
}

#[test]
fn accent_round_trips_and_can_be_recolored() {
    let (_dir, store) = store();
    let mut doc = store.create("Tinted", 32, 32).unwrap();
    assert!(calumma_core::PROJECT_COLORS.contains(&doc.accent));
    doc.accent = [1, 2, 3];
    store.save(&mut doc).unwrap();
    assert_eq!(store.open_project(&doc.id).unwrap().accent, [1, 2, 3]);

    store.set_accent(&doc.id, [9, 8, 7]).unwrap();
    store.rename(&doc.id, "Renamed").unwrap();
    let listed = store.list_recent(8).unwrap();
    let row = listed.iter().find(|p| p.id == doc.id).unwrap();
    assert_eq!(row.accent, [9, 8, 7]);
    assert_eq!(row.name, "Renamed");
}

#[test]
fn rename_and_recolor_reject_unknown_projects() {
    let (_dir, store) = store();
    assert!(store.rename("nope", "x").is_err());
    assert!(store.set_accent("nope", [1, 2, 3]).is_err());
}

#[test]
fn imported_image_round_trips() {
    let (_dir, store) = store();
    let mut doc = store.create("Artwork", 300, 200).unwrap();
    let mut rgba = vec![0u8; 300 * 200 * 4];
    rgba[0..4].copy_from_slice(&[9, 8, 7, 255]);
    let corner = (199 * 300 + 299) * 4;
    rgba[corner..corner + 4].copy_from_slice(&[1, 2, 3, 255]);
    assert!(doc.place_image(&rgba, 300, 200));
    store.save(&mut doc).unwrap();

    let loaded = store.open_project(&doc.id).unwrap();
    let i = paint_index(&loaded);
    let tiles = loaded.layers[i].tiles().unwrap();
    assert_eq!(tiles.get_pixel(0, 0), [9, 8, 7, 255]);
    assert_eq!(tiles.get_pixel(299, 199), [1, 2, 3, 255]);
}

/// A lock is worth nothing if it comes off every time the project is reopened — the whole
/// point is that the layer you protected stays protected. Names have to survive too, or a
/// rename is a session-long illusion.
#[test]
fn layer_lock_and_name_round_trip() {
    let (_dir, store) = store();
    let mut doc = store.create("Locked", 64, 64).unwrap();
    let i = paint_index(&doc);
    assert!(doc.set_layer_name(i, "Line art"));
    assert!(doc.set_layer_locked(i, true));
    store.save(&mut doc).unwrap();

    let loaded = store.open_project(&doc.id).unwrap();
    let i = loaded
        .layers
        .iter()
        .position(|l| l.name == "Line art")
        .expect("the renamed layer came back under its own name");
    assert!(loaded.layers[i].locked, "and still locked");

    let mut unlocked = loaded;
    assert!(unlocked.set_layer_locked(i, false));
    store.save(&mut unlocked).unwrap();
    let reopened = store.open_project(&unlocked.id).unwrap();
    assert!(
        !reopened.layers[i].locked,
        "unlocking persists just as well as locking"
    );
}

/// A layer dragged with the Move tool (or scaled/rotated with `⌘T`) lives entirely in
/// `Layer.transform` — moving it never touches a pixel. Before this, `transform` had no
/// column, so the offset was only ever in memory: reopening the project always drew the
/// layer back at its untransformed, top-left position.
#[test]
fn layer_transform_round_trip() {
    let (_dir, store) = store();
    let mut doc = store.create("Moved", 64, 64).unwrap();
    let i = paint_index(&doc);
    doc.layers[i].transform = Some(LayerTransform {
        offset_x: 12.0,
        offset_y: -6.0,
        scale_x: 1.5,
        scale_y: 0.8,
        rotation: 0.25,
    });
    store.save(&mut doc).unwrap();

    let loaded = store.open_project(&doc.id).unwrap();
    let i = paint_index(&loaded);
    let t = loaded.layers[i]
        .transform
        .expect("the moved layer keeps its transform after reopening");
    assert!((t.offset_x - 12.0).abs() < 1e-4);
    assert!((t.offset_y + 6.0).abs() < 1e-4);
    assert!((t.scale_x - 1.5).abs() < 1e-4);
    assert!((t.scale_y - 0.8).abs() < 1e-4);
    assert!((t.rotation - 0.25).abs() < 1e-4);

    let mut reset = loaded;
    let i = paint_index(&reset);
    reset.layers[i].transform = None;
    store.save(&mut reset).unwrap();
    let reopened = store.open_project(&reset.id).unwrap();
    assert!(reopened.layers[paint_index(&reopened)].transform.is_none());
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

/// Simulates a real project saved before this fix: every other layer column already exists,
/// `transform` does not. The migration must add just that column, not touch the rest.
#[test]
fn adds_transform_column_to_a_database_saved_before_this_fix() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("pre_fix.sqlite");
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE layers (
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
                PRIMARY KEY (project_id, layer_id)
            );",
        )
        .unwrap();
    }
    let store = ProjectStore::open(&path).unwrap();
    let mut doc = store.create("PreFix", 32, 32).unwrap();
    let i = paint_index(&doc);
    doc.layers[i].transform = Some(LayerTransform {
        offset_x: 5.0,
        offset_y: 5.0,
        scale_x: 1.0,
        scale_y: 1.0,
        rotation: 0.0,
    });
    store.save(&mut doc).unwrap();
    let loaded = store.open_project(&doc.id).unwrap();
    let t = loaded.layers[paint_index(&loaded)]
        .transform
        .expect("transform persists once the column exists");
    assert!((t.offset_x - 5.0).abs() < 1e-4);
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
    assert_eq!(tile_rows(&store, &doc.id), paper_tile_count(1024, 1024) + 3);

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
    assert_eq!(tile_rows(&store, &doc.id), paper_tile_count(512, 512) + 1);

    doc.clear_active_layer();
    store.save(&mut doc).unwrap();
    assert_eq!(tile_rows(&store, &doc.id), paper_tile_count(512, 512));
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
    assert_eq!(tile_rows(&store, &doc.id), paper_tile_count(256, 256));
}

#[test]
fn paper_layer_round_trip() {
    let (_dir, store) = store();
    let doc = store.create("Paper", 64, 48).unwrap();
    let loaded = store.open_project(&doc.id).unwrap();
    assert!(loaded.layers[0].is_paper());
    assert!(loaded.layers[0].visible);
    let tiles = loaded.layers[0].tiles().unwrap();
    assert_eq!(tiles.get_pixel(0, 0), [255, 255, 255, 255]);
    assert_eq!(tiles.get_pixel(63, 47), [255, 255, 255, 255]);
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

#[test]
fn save_writes_project_thumbnail_png() {
    let (_dir, store) = store();
    let mut doc = store.create("Thumb", 64, 48).unwrap();
    let i = paint_index(&doc);
    doc.layers[i]
        .tiles_mut()
        .unwrap()
        .set_pixel(2, 2, [9, 8, 7, 255]);
    store.save(&mut doc).unwrap();
    let png = store.project_thumbnail(&doc.id).unwrap();
    assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
}

#[test]
fn open_project_tabs_persist_and_cascade_on_delete() {
    let (_dir, store) = store();
    let doc_a = store.create("A", 32, 32).unwrap();
    let doc_b = store.create("B", 32, 32).unwrap();
    store
        .set_open_project_tabs(&[doc_a.id.clone(), doc_b.id.clone()])
        .unwrap();
    assert_eq!(
        store.open_project_tabs().unwrap(),
        vec![doc_a.id.clone(), doc_b.id.clone()]
    );
    store.delete(&doc_a.id).unwrap();
    assert_eq!(store.open_project_tabs().unwrap(), vec![doc_b.id.clone()]);
}

#[test]
fn delete_all_projects_clears_store() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProjectStore::open(dir.path().join("t.sqlite")).unwrap();
    let doc_a = store.create("A", 32, 32).unwrap();
    let doc_b = store.create("B", 64, 64).unwrap();
    store
        .set_open_project_tabs(std::slice::from_ref(&doc_a.id))
        .unwrap();
    assert_eq!(store.list_recent(8).unwrap().len(), 2);

    store.delete_all_projects().unwrap();

    assert!(store.list_recent(8).unwrap().is_empty());
    assert!(store.open_project_tabs().unwrap().is_empty());
    assert!(store.open_project(&doc_b.id).is_err());
}

/// Paper is written to disk as one identical white blob per tile. Reading them back into
/// separate allocations would undo the sharing a freshly created project has, so the loader
/// hands every solid tile of a color the same buffer.
#[test]
fn solid_tiles_come_back_sharing_one_allocation() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProjectStore::open(dir.path().join("t.sqlite")).unwrap();
    let mut doc = store.create("Paper", 1024, 1024).unwrap();
    store.save(&mut doc).unwrap();

    let reopened = store.open_project(&doc.id).unwrap();
    let paper = reopened.layers.iter().find(|l| l.is_paper()).unwrap();
    let grid = paper.tiles().unwrap();
    let mut coords: Vec<_> = grid.coords().collect();
    coords.sort_by_key(|c| (c.y, c.x));
    assert!(coords.len() >= 16, "a 1024px paper covers 4x4 tiles");

    let first = grid.get(coords[0]).unwrap();
    for coord in &coords[1..] {
        assert!(
            std::sync::Arc::ptr_eq(first, grid.get(*coord).unwrap()),
            "every white tile shares one buffer after a reload"
        );
    }
    assert_eq!(grid.get_pixel(900, 900), [255, 255, 255, 255]);
}

#[test]
fn a_painted_tile_is_not_shared_with_the_solid_ones() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProjectStore::open(dir.path().join("t.sqlite")).unwrap();
    let mut doc = store.create("Mixed", 512, 512).unwrap();
    doc.layers[0]
        .tiles_mut()
        .unwrap()
        .set_pixel(5, 5, [1, 2, 3, 255]);
    store.save(&mut doc).unwrap();

    let reopened = store.open_project(&doc.id).unwrap();
    let grid = reopened.layers[0].tiles().unwrap();
    assert_eq!(grid.get_pixel(5, 5), [1, 2, 3, 255]);
    assert_eq!(grid.get_pixel(300, 300), [255, 255, 255, 255]);
}

#[test]
fn guides_round_trip() {
    let (_dir, store) = store();
    let mut doc = store.create("Guided", 64, 64).unwrap();
    doc.add_guide(GuideAxis::Horizontal, 12.5);
    doc.add_guide(GuideAxis::Vertical, -3.25);
    store.save(&mut doc).unwrap();

    let loaded = store.open_project(&doc.id).unwrap();
    assert_eq!(
        loaded.guides(),
        [
            Guide::new(GuideAxis::Horizontal, 12.5),
            Guide::new(GuideAxis::Vertical, -3.25),
        ]
    );
}

/// A recolored guide is still a guide the store has to hand back the way it was left — the
/// color rides the same blob as the position.
#[test]
fn a_guides_color_round_trips() {
    let (_dir, store) = store();
    let mut doc = store.create("Guided", 64, 64).unwrap();
    doc.add_guide(GuideAxis::Vertical, 24.0);
    assert!(doc.set_guide_color(0, [12, 200, 90]));
    store.save(&mut doc).unwrap();

    let loaded = store.open_project(&doc.id).unwrap();

    assert_eq!(loaded.guides()[0].color, [12, 200, 90]);
}

#[test]
fn clearing_guides_clears_them_on_disk_too() {
    let (_dir, store) = store();
    let mut doc = store.create("Guided", 64, 64).unwrap();
    doc.add_guide(GuideAxis::Horizontal, 8.0);
    store.save(&mut doc).unwrap();
    doc.clear_guides();
    store.save(&mut doc).unwrap();
    assert!(store.open_project(&doc.id).unwrap().guides().is_empty());
}

#[test]
fn adds_guides_column_to_a_database_saved_before_guides_existed() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("pre_guides.sqlite");
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                width INTEGER NOT NULL,
                height INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                opened_at INTEGER NOT NULL,
                thumb BLOB,
                accent INTEGER
            );",
        )
        .unwrap();
    }
    let store = ProjectStore::open(&path).unwrap();
    let mut doc = store.create("PreGuides", 32, 32).unwrap();
    doc.add_guide(GuideAxis::Vertical, 7.0);
    store.save(&mut doc).unwrap();
    assert_eq!(store.open_project(&doc.id).unwrap().guides().len(), 1);
}

/// Opening an install from before workspaces were removed clears the three tables they left
/// behind, so no database keeps orphan tables nothing in the codebase explains.
#[test]
fn opening_an_old_install_drops_the_workspace_tables() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("t.sqlite");
    ProjectStore::open(&path).unwrap();

    // The shape an older build left behind.
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE workspaces (id TEXT PRIMARY KEY, name TEXT NOT NULL);
             CREATE TABLE workspace_projects (workspace_id TEXT, project_id TEXT);
             CREATE TABLE open_workspace_tabs (position INTEGER PRIMARY KEY, workspace_id TEXT);
             INSERT INTO workspaces (id, name) VALUES ('w1', 'Desk');
             INSERT INTO open_workspace_tabs (position, workspace_id) VALUES (0, 'w1');",
        )
        .unwrap();
    }

    let store = ProjectStore::open(&path).unwrap();
    let conn = Connection::open(store.path()).unwrap();
    for table in ["workspaces", "workspace_projects", "open_workspace_tabs"] {
        let found: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                params![table],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(found, 0, "{table} should have been dropped");
    }
}
