use calumma_core::layer::*;
use calumma_core::tile::*;
use calumma_core::vector::{VectorItem, VectorPath};

#[test]
fn new_layer_is_raster() {
    let layer = Layer::new("L", 64, 64);
    assert!(layer.content.is_raster());
    assert!(layer.tiles().unwrap().is_empty());
}

#[test]
fn vector_layer_reports_path_bounds() {
    let layer = Layer::vector(
        "V",
        vec![VectorItem::Path(VectorPath {
            points: vec![(10.0, 20.0), (30.0, 40.0)],
            closed: false,
            fill: false,
            color: [0, 0, 0, 255],
            stroke_width: 2.0,
        })],
    );
    assert!(layer.content.is_vector());
    // A stroked path covers half its width on every side, plus a pixel of antialiasing.
    assert_eq!(layer.content_bounds(), Some((8.0, 18.0, 32.0, 42.0)));
}

#[test]
fn vector_layer_tile_access_is_none_not_panic() {
    let mut layer = Layer::vector("V", Vec::new());
    assert!(layer.tiles().is_none());
    assert!(layer.tiles_mut().is_none());
    assert!(layer.dirty_tiles(DirtyChannel::Render).is_none());
    assert!(layer.clear().is_empty());
    layer.mark_all_dirty();
}

#[test]
fn setting_mask_dirties_every_tile() {
    let mut layer = Layer::new("L", 1024, 1024);
    layer.tiles_mut().unwrap().set_pixel(10, 10, [1, 2, 3, 255]);
    layer
        .tiles_mut()
        .unwrap()
        .set_pixel(600, 600, [1, 2, 3, 255]);
    layer.clear_dirty(DirtyChannel::Render);
    assert!(layer.dirty_tiles(DirtyChannel::Render).unwrap().is_empty());
    layer.set_mask(Some(vec![255; 1024 * 1024]));
    assert_eq!(layer.dirty_tiles(DirtyChannel::Render).unwrap().len(), 2);
}

#[test]
fn resize_mask_keeps_the_overlapping_region_and_pads_opaque() {
    let mut layer = Layer::new("m", 4, 4);
    layer.set_mask(Some((0..16).map(|i| i as u8).collect()));
    layer.resize_mask(4, 4, 6, 6);
    let mask = layer.mask().expect("mask survives the resize");
    assert_eq!(mask.len(), 36);
    for y in 0..4usize {
        for x in 0..4usize {
            assert_eq!(mask[y * 6 + x], (y * 4 + x) as u8);
        }
    }
    assert_eq!(mask[4], 255);
    assert_eq!(mask[30], 255);
}

#[test]
fn resize_mask_crops_when_shrinking_and_is_a_no_op_without_a_mask() {
    let mut layer = Layer::new("m", 4, 4);
    layer.set_mask(Some((0..16).map(|i| i as u8).collect()));
    layer.resize_mask(4, 4, 2, 2);
    let mask = layer.mask().unwrap();
    assert_eq!(mask, &[0, 1, 4, 5]);

    let mut bare = Layer::new("n", 4, 4);
    bare.resize_mask(4, 4, 8, 8);
    assert!(bare.mask().is_none());
}

#[test]
fn content_bounds_spans_every_painted_tile() {
    let mut layer = Layer::new("c", 1024, 1024);
    assert!(layer.content_bounds().is_none());
    let grid = layer.tiles_mut().unwrap();
    grid.set_pixel(10, 10, [1, 1, 1, 255]);
    grid.set_pixel(600, 700, [1, 1, 1, 255]);
    let (min_x, min_y, max_x, max_y) = layer.content_bounds().unwrap();
    assert!(min_x <= 10.0 && min_y <= 10.0);
    assert!(max_x >= 600.0 && max_y >= 700.0);
}
