use calumma_core::document::Document;
use calumma_core::history::*;
use calumma_core::layer::*;
use calumma_core::tile::*;
use calumma_core::vector::{VectorItem, VectorPath};

fn empty_path() -> VectorItem {
    VectorItem::Path(VectorPath {
        points: vec![],
        closed: false,
        fill: false,
        stroke: true,
        color: [0, 0, 0, 255],
        stroke_color: [0, 0, 0, 255],
        stroke_width: 1.0,
    })
}

fn undo(doc: &mut Document) -> bool {
    doc.undo()
}

#[test]
fn a_step_whose_layer_is_gone_is_skipped_not_misapplied() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    let painted = doc.active_layer;
    let coord = TileCoord { x: 0, y: 0 };
    let before = doc.layers[painted]
        .tiles()
        .unwrap()
        .snapshot_tiles(&[coord]);
    doc.layers[painted]
        .tiles_mut()
        .unwrap()
        .set_pixel(2, 2, [255, 0, 0, 255]);
    doc.history
        .push_layer_tiles(doc.layers[painted].id.clone(), before, Some(painted));

    doc.layers.remove(painted);
    doc.active_layer = 0;
    let untouched = doc.layers[0].tiles().unwrap().get_pixel(2, 2);
    doc.undo();
    assert_eq!(
        doc.layers[0].tiles().unwrap().get_pixel(2, 2),
        untouched,
        "the surviving layer was not painted with the dead one's undo"
    );
}

fn pixel(layer: &Layer, x: i32, y: i32) -> [u8; 4] {
    layer.tiles().unwrap().get_pixel(x, y)
}

#[test]
fn undo_redo_pixel() {
    let mut doc = Document::new("p".into(), "t", 256, 256);
    let layer_index = doc.active_layer;
    let mut history = std::mem::take(&mut doc.history);
    let layer = &mut doc.layers[layer_index];
    let coord = TileCoord { x: 0, y: 0 };
    let before = layer.tiles().unwrap().snapshot_tiles(&[coord]);
    layer.tiles_mut().unwrap().set_pixel(5, 5, [255, 0, 0, 255]);
    history.push_layer_tiles(layer.id.clone(), before, Some(layer_index));
    doc.history = history;
    assert_eq!(pixel(&doc.layers[layer_index], 5, 5), [255, 0, 0, 255]);
    assert!(doc.undo());
    assert_eq!(pixel(&doc.layers[layer_index], 5, 5), [0, 0, 0, 0]);
    assert!(doc.redo());
    assert_eq!(pixel(&doc.layers[layer_index], 5, 5), [255, 0, 0, 255]);
}

#[test]
fn absent_tiles_cost_nothing() {
    let mut doc = Document::new("p".into(), "t", 4096, 4096);
    let layer_index = doc.active_layer;
    let mut history = std::mem::take(&mut doc.history);
    let layer = &mut doc.layers[layer_index];
    let coords: Vec<TileCoord> = (0..100).map(|i| TileCoord { x: i, y: 0 }).collect();
    let snap = layer.tiles().unwrap().snapshot_tiles(&coords);
    assert!(snap.values().all(|v| v.is_none()));
    layer.tiles_mut().unwrap().set_pixel(0, 0, [1, 2, 3, 4]);
    history.push_layer_tiles(layer.id.clone(), snap, Some(layer_index));
    doc.history = history;
    assert_eq!(doc.history.memory_used(), 0);
}

#[test]
fn present_tiles_are_charged_once_each() {
    let mut doc = Document::new("p".into(), "t", 1024, 1024);
    let layer_index = doc.active_layer;
    let mut history = std::mem::take(&mut doc.history);
    let layer = &mut doc.layers[layer_index];
    layer.tiles_mut().unwrap().set_pixel(10, 10, [1, 1, 1, 255]);
    layer
        .tiles_mut()
        .unwrap()
        .set_pixel(600, 600, [1, 1, 1, 255]);
    let coords: Vec<TileCoord> = layer.tiles().unwrap().coords().collect();
    let snap = layer.tiles().unwrap().snapshot_tiles(&coords);
    history.push_layer_tiles(layer.id.clone(), snap, Some(layer_index));
    doc.history = history;
    assert_eq!(doc.history.memory_used(), 2 * TILE_BYTES);
}

#[test]
fn discarded_redo_releases_budget() {
    let mut doc = Document::new("p".into(), "t", 256, 256);
    let layer_index = doc.active_layer;
    let mut history = std::mem::take(&mut doc.history);
    let layer = &mut doc.layers[layer_index];
    let coord = TileCoord { x: 0, y: 0 };
    for i in 0..3 {
        let before = layer.tiles().unwrap().snapshot_tiles(&[coord]);
        layer.tiles_mut().unwrap().set_pixel(i, i, [255, 0, 0, 255]);
        history.push_layer_tiles(layer.id.clone(), before, Some(layer_index));
    }
    doc.history = history;
    assert!(doc.undo());
    assert!(doc.undo());
    assert!(doc.history.can_redo());

    let layer = &mut doc.layers[layer_index];
    let before = layer.tiles().unwrap().snapshot_tiles(&[coord]);
    layer.tiles_mut().unwrap().set_pixel(9, 9, [1, 2, 3, 4]);
    doc.history
        .push_layer_tiles(layer.id.clone(), before, Some(layer_index));

    assert!(!doc.history.can_redo());
    assert_eq!(doc.history.undo_depth(), 2);
}

#[test]
fn undo_redo_mask() {
    let mut doc = Document::new("p".into(), "t", 4, 4);
    let layer_index = doc.active_layer;
    let mut history = std::mem::take(&mut doc.history);
    let layer = &mut doc.layers[layer_index];
    let before = layer.mask_owned();
    layer.set_mask(Some(vec![255; 16]));
    history.push_layer_mask(layer.id.clone(), before, Some(layer_index));
    doc.history = history;
    assert!(doc.layers[layer_index].mask().is_some());
    assert!(doc.undo());
    assert!(doc.layers[layer_index].mask().is_none());
    assert!(doc.redo());
    assert_eq!(doc.layers[layer_index].mask().map(|m| m.len()), Some(16));
}

#[test]
fn undo_restores_active_layer() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    doc.add_layer("B");
    let mut history = std::mem::take(&mut doc.history);
    let coord = TileCoord { x: 0, y: 0 };
    let layer_index = doc.active_layer;
    let before = doc.layers[layer_index]
        .tiles()
        .unwrap()
        .snapshot_tiles(&[coord]);
    doc.layers[layer_index]
        .tiles_mut()
        .unwrap()
        .set_pixel(1, 1, [9, 9, 9, 255]);
    history.push_layer_tiles(
        doc.layers[layer_index].id.clone(),
        before,
        Some(layer_index),
    );
    doc.history = history;
    doc.active_layer = 0;
    assert!(doc.undo());
    assert_eq!(doc.active_layer, layer_index);
    assert!(doc.redo());
    assert_eq!(doc.active_layer, 0);
}

#[test]
fn a_command_with_no_diffs_at_all_is_not_pushed() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    let mut history = History::default();
    history.push(HistoryCommand {
        diffs: Vec::new(),
        masks: Vec::new(),
        runs: Vec::new(),
        transforms: Vec::new(),
        props: Vec::new(),
        vectors: Vec::new(),
        stack: None,
        active_layer_index: Some(0),
        bytes: 0,
    });
    assert!(!history.can_undo());
    assert_eq!(history.undo_depth(), 0);
    assert_eq!(history.memory_used(), 0);
    assert!(!undo(&mut doc));
}

#[test]
fn a_snapshot_of_absent_tiles_is_still_an_undoable_step() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    let layer_index = doc.active_layer;
    let mut history = std::mem::take(&mut doc.history);
    let layer = &mut doc.layers[layer_index];
    let coord = TileCoord { x: 0, y: 0 };
    let before = layer.tiles().unwrap().snapshot_tiles(&[coord]);
    assert!(before.values().all(Option::is_none), "nothing painted yet");
    layer.tiles_mut().unwrap().set_pixel(4, 4, [255, 0, 0, 255]);
    history.push_layer_tiles(layer.id.clone(), before, Some(layer_index));
    doc.history = history;
    assert!(doc.history.can_undo());
    assert_eq!(doc.history.memory_used(), 0, "an absent tile costs nothing");
    assert!(doc.undo());
    assert_eq!(pixel(&doc.layers[layer_index], 4, 4), [0, 0, 0, 0]);
}

#[test]
fn undo_and_redo_on_an_empty_stack_report_that_nothing_happened() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    assert!(!doc.history.can_undo());
    assert!(!doc.history.can_redo());
    assert!(!doc.undo());
    assert!(!doc.redo());
}

#[test]
fn undoing_past_the_bottom_stops_rather_than_wrapping() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    let layer_index = doc.active_layer;
    let mut history = std::mem::take(&mut doc.history);
    let layer = &mut doc.layers[layer_index];
    let coord = TileCoord { x: 0, y: 0 };
    let before = layer.tiles().unwrap().snapshot_tiles(&[coord]);
    layer.tiles_mut().unwrap().set_pixel(1, 1, [255, 0, 0, 255]);
    history.push_layer_tiles(layer.id.clone(), before, Some(layer_index));
    doc.history = history;

    assert!(doc.undo());
    assert!(!doc.undo(), "nothing left to undo");
    assert_eq!(pixel(&doc.layers[layer_index], 1, 1), [0, 0, 0, 0], "and nothing moved");

    assert!(doc.redo());
    assert!(!doc.redo(), "nothing left to redo");
    assert_eq!(pixel(&doc.layers[layer_index], 1, 1), [255, 0, 0, 255]);
}

#[test]
fn a_tile_diff_against_a_layer_with_no_pixels_is_skipped() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    let layer_index = doc.active_layer;
    let mut history = std::mem::take(&mut doc.history);
    let raster = &mut doc.layers[layer_index];
    let coord = TileCoord { x: 0, y: 0 };
    let before = raster.tiles().unwrap().snapshot_tiles(&[coord]);
    raster
        .tiles_mut()
        .unwrap()
        .set_pixel(3, 3, [255, 0, 0, 255]);
    let layer_id = raster.id.clone();
    history.push_layer_tiles(layer_id.clone(), before, Some(layer_index));
    doc.history = history;

    let mut vector = Layer::vector("V", empty_path());
    vector.id = layer_id;
    doc.layers = vec![vector];
    doc.active_layer = 0;
    assert!(doc.undo());
    assert!(doc.layers[0].tiles().is_none(), "still a vector layer");
}

#[test]
fn a_new_edit_after_an_undo_drops_the_redo_branch_and_its_budget() {
    let mut doc = Document::new("p".into(), "t", 512, 512);
    let layer_index = doc.active_layer;
    let mut history = std::mem::take(&mut doc.history);
    let layer = &mut doc.layers[layer_index];
    let coord = TileCoord { x: 0, y: 0 };
    for i in 1..4 {
        let before = layer.tiles().unwrap().snapshot_tiles(&[coord]);
        layer.tiles_mut().unwrap().set_pixel(i, i, [255, 0, 0, 255]);
        history.push_layer_tiles(layer.id.clone(), before, Some(layer_index));
    }
    doc.history = history;
    assert!(doc.undo());
    let with_redo = doc.history.memory_used();
    assert!(doc.history.can_redo());

    let layer = &mut doc.layers[layer_index];
    let before = layer.tiles().unwrap().snapshot_tiles(&[coord]);
    layer.tiles_mut().unwrap().set_pixel(20, 20, [1, 2, 3, 4]);
    doc.history
        .push_layer_tiles(layer.id.clone(), before, Some(layer_index));

    assert!(!doc.history.can_redo());
    assert!(
        doc.history.memory_used() <= with_redo,
        "the dropped branch gave its bytes back"
    );
}

#[test]
fn a_mask_undo_leaves_the_pixels_alone() {
    let mut doc = Document::new("p".into(), "t", 4, 4);
    let layer_index = doc.active_layer;
    let mut history = std::mem::take(&mut doc.history);
    doc.layers[layer_index]
        .tiles_mut()
        .unwrap()
        .set_pixel(1, 1, [9, 9, 9, 255]);

    let before = doc.layers[layer_index].mask_owned();
    doc.layers[layer_index].set_mask(Some(vec![128; 16]));
    history.push_layer_mask(
        doc.layers[layer_index].id.clone(),
        before,
        Some(layer_index),
    );
    doc.history = history;

    assert!(doc.undo());
    assert!(doc.layers[layer_index].mask().is_none());
    assert_eq!(
        pixel(&doc.layers[layer_index], 1, 1),
        [9, 9, 9, 255],
        "the mask step did not touch the tiles"
    );
}
