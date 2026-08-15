use calumma_core::{Document, Layer, TextAlign, TextRun, Tool};
use calumma_io::*;
use rusqlite::{params, Connection};
use tempfile::tempdir;

fn store() -> (tempfile::TempDir, ProjectStore) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("t.sqlite");
    let store = ProjectStore::open(&path).unwrap();
    (dir, store)
}

fn typed_project(store: &ProjectStore, text: &str) -> Document {
    let mut doc = store.create("text", 512, 512).unwrap();
    doc.resize_viewport(512.0, 512.0, 1.0);
    doc.fit_to_view();
    doc.tool = Tool::Text;
    doc.text_style.size = 64.0;
    let (sx, sy) = doc.camera.to_screen(80.0, 120.0);
    doc.pointer_down(sx, sy);
    doc.text_insert(text);
    doc.set_text_align(TextAlign::Center);
    doc.commit_text();
    store.save(&mut doc).unwrap();
    doc
}

fn text_layer(doc: &Document) -> &Layer {
    doc.layers.iter().find(|l| l.is_text()).expect("text layer")
}

#[test]
fn a_text_layer_round_trips_with_its_run() {
    let (_dir, store) = store();
    let saved = typed_project(&store, "Hello Calumma");
    let before = text_layer(&saved).run().unwrap().clone();

    let reopened = store.open_project(&saved.id).unwrap();
    let after = text_layer(&reopened).run().unwrap();
    assert_eq!(after.text, "Hello Calumma");
    assert_eq!(after.family, before.family);
    assert_eq!(after.size, before.size);
    assert_eq!(after.align, TextAlign::Center);
    assert_eq!(after.origin, before.origin);
    assert_eq!(after.color, before.color);
}

#[test]
fn reopening_re_rasterizes_the_text() {
    let (_dir, store) = store();
    let saved = typed_project(&store, "pixels");
    let reopened = store.open_project(&saved.id).unwrap();
    let grid = text_layer(&reopened).tiles().unwrap();
    assert!(!grid.is_empty(), "text layer should have pixels after open");
    assert_eq!(
        text_layer(&saved).tiles().unwrap().len(),
        grid.len(),
        "the same run must rasterize to the same tiles"
    );
}

/// The run is the source of truth, so its tiles are never written — that is what keeps a
/// project with a hundred headlines from carrying a hundred bitmaps.
#[test]
fn text_pixels_are_not_stored() {
    let (_dir, store) = store();
    let saved = typed_project(&store, "no bitmaps here");
    let layer_id = text_layer(&saved).id.clone();
    let conn = Connection::open(store.path()).unwrap();
    let tiles: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tiles WHERE project_id = ?1 AND layer_id = ?2",
            params![saved.id, layer_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tiles, 0);
    let kind: i64 = conn
        .query_row(
            "SELECT content_kind FROM layers WHERE project_id = ?1 AND layer_id = ?2",
            params![saved.id, layer_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(kind, 2);
}

#[test]
fn text_survives_a_further_edit_and_save() {
    let (_dir, store) = store();
    let saved = typed_project(&store, "draft");
    let mut doc = store.open_project(&saved.id).unwrap();
    let index = doc.layers.iter().position(Layer::is_text).unwrap();
    assert!(doc.edit_text_layer(index));
    doc.text_insert(" final");
    doc.commit_text();
    store.save(&mut doc).unwrap();

    let reopened = store.open_project(&saved.id).unwrap();
    assert_eq!(text_layer(&reopened).run().unwrap().text, "draft final");
}

/// A row that claims to be text but whose blob will not decode must not take the project
/// down with it — the layer comes back as an ordinary empty raster layer.
#[test]
fn a_corrupt_run_degrades_to_a_raster_layer() {
    let (_dir, store) = store();
    let saved = typed_project(&store, "damaged");
    let layer_id = text_layer(&saved).id.clone();
    Connection::open(store.path())
        .unwrap()
        .execute(
            "UPDATE layers SET text_data = ?1 WHERE project_id = ?2 AND layer_id = ?3",
            params![vec![0xFFu8, 0x00, 0x01], saved.id, layer_id],
        )
        .unwrap();

    let reopened = store.open_project(&saved.id).unwrap();
    let layer = reopened.layers.iter().find(|l| l.id == layer_id).unwrap();
    assert!(!layer.is_text());
    assert!(layer.tiles().is_some());
}

#[test]
fn a_text_layer_is_exported_into_the_psd() {
    let (_dir, store) = store();
    let mut doc = typed_project(&store, "PSD");
    let with_text = encode_psd(&doc);
    let index = doc.layers.iter().position(Layer::is_text).unwrap();
    doc.remove_layer(index);
    let without_text = encode_psd(&doc);
    assert!(
        with_text.len() > without_text.len(),
        "the text layer should reach the PSD as its own layer"
    );
}

#[test]
fn a_blank_run_is_still_readable() {
    let (_dir, store) = store();
    let mut doc = store.create("blank", 256, 256).unwrap();
    doc.layers.push(Layer::text(
        "Text 1",
        TextRun::default().clamped(),
        256,
        256,
    ));
    store.save(&mut doc).unwrap();
    let reopened = store.open_project(&doc.id).unwrap();
    assert_eq!(text_layer(&reopened).run().unwrap().text, "");
}

/// A run styled bold and italic must come back styled — the blob grew a version for exactly
/// this, and a project written by an older build must still open without it.
#[test]
fn bold_italic_and_line_height_survive_a_round_trip() {
    let (_dir, store) = store();
    let mut doc = store.create("styled", 512, 512).unwrap();
    doc.resize_viewport(512.0, 512.0, 1.0);
    doc.fit_to_view();
    doc.tool = Tool::Text;
    let (sx, sy) = doc.camera.to_screen(80.0, 120.0);
    doc.pointer_down(sx, sy);
    doc.text_insert("Styled");
    doc.set_text_bold(true);
    doc.set_text_italic(true);
    doc.set_text_line_height(1.75);
    doc.commit_text();
    store.save(&mut doc).unwrap();

    let reopened = store.open_project(&doc.id).unwrap();
    let run = text_layer(&reopened).run().unwrap();
    assert!(run.bold);
    assert!(run.italic);
    assert_eq!(run.line_height, 1.75);
    assert!(
        !text_layer(&reopened).tiles().unwrap().is_empty(),
        "the styled run rasterizes on open"
    );
}

/// The blob a build before styled text wrote: same fields, version 1, no weight or slant.
fn version_one_blob(text: &str, family: &str, size: f32) -> Vec<u8> {
    let mut out = vec![1u8];
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(text.as_bytes());
    out.extend_from_slice(&(family.len() as u32).to_le_bytes());
    out.extend_from_slice(family.as_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&1.25f32.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 255]);
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&40.0f32.to_le_bytes());
    out.extend_from_slice(&60.0f32.to_le_bytes());
    out.push(0);
    out
}

#[test]
fn a_project_saved_before_styled_text_still_opens() {
    let (_dir, store) = store();
    let saved = typed_project(&store, "legacy");
    let layer = text_layer(&saved);
    let layer_id = layer.id.clone();
    let family = layer.run().unwrap().family.clone();
    Connection::open(store.path())
        .unwrap()
        .execute(
            "UPDATE layers SET text_data = ?1 WHERE project_id = ?2 AND layer_id = ?3",
            params![
                version_one_blob("legacy", &family, 48.0),
                saved.id,
                layer_id
            ],
        )
        .unwrap();

    let reopened = store.open_project(&saved.id).unwrap();
    let run = text_layer(&reopened).run().unwrap();
    assert_eq!(run.text, "legacy");
    assert_eq!(run.family, family);
    assert_eq!(run.size, 48.0);
    assert_eq!(run.align, TextAlign::Center);
    assert_eq!(run.origin, (40.0, 60.0));
    assert!(!run.bold && !run.italic, "an older run has no styles");
    assert!(text_layer(&reopened).is_text(), "still a text layer");
}
