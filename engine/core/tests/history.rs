use calumma_core::history::*;
use calumma_core::layer::*;
use calumma_core::tile::*;

fn undo(history: &mut History, layer: &mut Layer) -> bool {
    let mut active = 0;
    history.undo(std::slice::from_mut(layer), &mut active)
}

fn redo(history: &mut History, layer: &mut Layer) -> bool {
    let mut active = 0;
    history.redo(std::slice::from_mut(layer), &mut active)
}

fn pixel(layer: &Layer, x: i32, y: i32) -> [u8; 4] {
    layer.tiles().unwrap().get_pixel(x, y)
}

#[test]
fn undo_redo_pixel() {
    let mut layer = Layer::new("L1", 256, 256);
    let mut history = History::default();
    let coord = TileCoord { x: 0, y: 0 };
    let before = layer.tiles().unwrap().snapshot_tiles(&[coord]);
    layer.tiles_mut().unwrap().set_pixel(5, 5, [255, 0, 0, 255]);
    history.push_layer_tiles(layer.id.clone(), before, Some(0));
    assert_eq!(pixel(&layer, 5, 5), [255, 0, 0, 255]);
    assert!(undo(&mut history, &mut layer));
    assert_eq!(pixel(&layer, 5, 5), [0, 0, 0, 0]);
    assert!(redo(&mut history, &mut layer));
    assert_eq!(pixel(&layer, 5, 5), [255, 0, 0, 255]);
}

#[test]
fn absent_tiles_cost_nothing() {
    let mut layer = Layer::new("L1", 4096, 4096);
    let mut history = History::default();
    let coords: Vec<TileCoord> = (0..100).map(|i| TileCoord { x: i, y: 0 }).collect();
    let snap = layer.tiles().unwrap().snapshot_tiles(&coords);
    assert!(snap.values().all(|v| v.is_none()));
    layer.tiles_mut().unwrap().set_pixel(0, 0, [1, 2, 3, 4]);
    history.push_layer_tiles(layer.id.clone(), snap, Some(0));
    assert_eq!(history.memory_used(), 0);
}

#[test]
fn present_tiles_are_charged_once_each() {
    let mut layer = Layer::new("L1", 1024, 1024);
    let mut history = History::default();
    layer.tiles_mut().unwrap().set_pixel(10, 10, [1, 1, 1, 255]);
    layer
        .tiles_mut()
        .unwrap()
        .set_pixel(600, 600, [1, 1, 1, 255]);
    let coords: Vec<TileCoord> = layer.tiles().unwrap().coords().collect();
    let snap = layer.tiles().unwrap().snapshot_tiles(&coords);
    history.push_layer_tiles(layer.id.clone(), snap, Some(0));
    assert_eq!(history.memory_used(), 2 * TILE_BYTES);
}

#[test]
fn discarded_redo_releases_budget() {
    let mut layer = Layer::new("L1", 256, 256);
    let mut history = History::default();
    let coord = TileCoord { x: 0, y: 0 };
    for i in 0..3 {
        let before = layer.tiles().unwrap().snapshot_tiles(&[coord]);
        layer.tiles_mut().unwrap().set_pixel(i, i, [255, 0, 0, 255]);
        history.push_layer_tiles(layer.id.clone(), before, Some(0));
    }
    assert!(undo(&mut history, &mut layer));
    assert!(undo(&mut history, &mut layer));
    assert!(history.can_redo());

    let before = layer.tiles().unwrap().snapshot_tiles(&[coord]);
    layer.tiles_mut().unwrap().set_pixel(9, 9, [1, 2, 3, 4]);
    history.push_layer_tiles(layer.id.clone(), before, Some(0));

    assert!(!history.can_redo());
    assert_eq!(history.undo_depth(), 2);
}

#[test]
fn undo_redo_mask() {
    let mut layer = Layer::new("L1", 4, 4);
    let mut history = History::default();
    let before = layer.mask_owned();
    layer.set_mask(Some(vec![255; 16]));
    history.push_layer_mask(layer.id.clone(), before, Some(0));
    assert!(layer.mask().is_some());
    assert!(undo(&mut history, &mut layer));
    assert!(layer.mask().is_none());
    assert!(redo(&mut history, &mut layer));
    assert_eq!(layer.mask().map(|m| m.len()), Some(16));
}

#[test]
fn undo_restores_active_layer() {
    let mut layers = vec![Layer::new("A", 64, 64), Layer::new("B", 64, 64)];
    let mut history = History::default();
    let coord = TileCoord { x: 0, y: 0 };
    let before = layers[1].tiles().unwrap().snapshot_tiles(&[coord]);
    layers[1]
        .tiles_mut()
        .unwrap()
        .set_pixel(1, 1, [9, 9, 9, 255]);
    history.push_layer_tiles(layers[1].id.clone(), before, Some(1));

    let mut active = 0;
    assert!(history.undo(&mut layers, &mut active));
    assert_eq!(active, 1);
    assert!(history.redo(&mut layers, &mut active));
    assert_eq!(active, 0);
}
