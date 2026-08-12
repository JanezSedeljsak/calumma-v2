use calumma_core::layer::BlendMode;
use calumma_core::tile::TILE_SIZE;
use calumma_core::{DirtyChannel, Document};
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
fn accent_round_trips_and_can_be_recoloured() {
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
fn rename_and_recolour_reject_unknown_projects() {
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
fn workspace_crud_membership_and_open_tabs() {
    let (_dir, store) = store();
    let doc_a = store.create("A", 32, 32).unwrap();
    let doc_b = store.create("B", 32, 32).unwrap();
    let ws = store.create_workspace("Desk", None).unwrap();
    store.add_project_to_workspace(&ws.id, &doc_a.id).unwrap();
    store.add_project_to_workspace(&ws.id, &doc_b.id).unwrap();
    let projects = store.workspace_projects(&ws.id).unwrap();
    assert_eq!(projects.len(), 2);
    assert_eq!(
        store
            .workspace_containing_project(&doc_a.id)
            .unwrap()
            .unwrap()
            .id,
        ws.id
    );

    store
        .set_workspace_active_project(&ws.id, Some(&doc_b.id))
        .unwrap();
    assert_eq!(
        store
            .workspace(&ws.id)
            .unwrap()
            .active_project_id
            .as_deref(),
        Some(doc_b.id.as_str())
    );

    store
        .set_open_workspace_tabs(std::slice::from_ref(&ws.id))
        .unwrap();
    assert_eq!(store.open_workspace_tabs().unwrap(), vec![ws.id.clone()]);

    store
        .remove_project_from_workspace(&ws.id, &doc_b.id)
        .unwrap();
    assert_eq!(store.workspace_projects(&ws.id).unwrap().len(), 1);
    assert_eq!(
        store
            .workspace(&ws.id)
            .unwrap()
            .active_project_id
            .as_deref(),
        Some(doc_a.id.as_str())
    );

    store.delete(&doc_a.id).unwrap();
    assert!(store.workspace_projects(&ws.id).unwrap().is_empty());

    store.delete_workspace(&ws.id).unwrap();
    assert!(store.list_workspaces(8).unwrap().is_empty());
    assert!(store.open_workspace_tabs().unwrap().is_empty());
}
