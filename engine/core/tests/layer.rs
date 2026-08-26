use calumma_core::layer::*;
use calumma_core::tile::*;
use calumma_core::transform::LayerTransform;
use calumma_core::vector::{VectorItem, VectorPath};
use calumma_core::DocRect;

fn unbound_item() -> VectorItem {
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

fn stroked_path() -> VectorItem {
    VectorItem::Path(VectorPath {
        points: vec![(10.0, 20.0), (30.0, 40.0)],
        closed: false,
        fill: false,
        stroke: true,
        color: [0, 0, 0, 255],
        stroke_color: [0, 0, 0, 255],
        stroke_width: 2.0,
    })
}

#[test]
fn new_layer_is_raster() {
    let layer = Layer::new("L", 64, 64);
    assert!(layer.content.is_raster());
    assert!(layer.tiles().unwrap().is_empty());
}

#[test]
fn vector_layer_reports_path_bounds() {
    let layer = Layer::vector("V", stroked_path());
    assert!(layer.content.is_vector());
    // A stroked path covers half its width on every side, plus a pixel of antialiasing.
    assert_eq!(layer.content_bounds(), Some((8.0, 18.0, 32.0, 42.0)));
}

#[test]
fn vector_layer_tile_access_is_none_not_panic() {
    let mut layer = Layer::vector("V", unbound_item());
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

/// The box is tight to the pixels, not to the tiles that hold them. Two dots 590 pixels apart
/// live in different tiles; the box is the 591-pixel span between them, not the 1024-pixel
/// union of their four 256-pixel cells.
#[test]
fn content_bounds_is_tight_to_the_painted_pixels() {
    let mut layer = Layer::new("c", 1024, 1024);
    assert!(layer.content_bounds().is_none());
    let grid = layer.tiles_mut().unwrap();
    grid.set_pixel(10, 10, [1, 1, 1, 255]);
    grid.set_pixel(600, 700, [1, 1, 1, 255]);
    assert_eq!(layer.content_bounds(), Some((10.0, 10.0, 601.0, 701.0)));
}

/// The case that sent this back: a paste fills a rectangle that does not line up with the tile
/// grid, and the `⌘T` frame used to be drawn around the tiles instead of around the picture.
#[test]
fn a_paste_sized_rectangle_reports_its_own_size_not_its_tiles() {
    let mut layer = Layer::new("p", 1024, 1024);
    let grid = layer.tiles_mut().unwrap();
    grid.fill_uniform(DocRect::new(40, 30, 339, 229), [9, 9, 9, 255]);
    assert_eq!(layer.content_bounds(), Some((40.0, 30.0, 340.0, 230.0)));
}

/// Tiles full of nothing are not content. A layer that was painted and then erased has no box
/// to frame, transform or pick — the tiles it left behind do not keep it alive.
#[test]
fn tiles_holding_only_transparent_pixels_are_not_bounds() {
    let mut layer = Layer::new("e", 512, 512);
    let grid = layer.tiles_mut().unwrap();
    grid.set_pixel(100, 100, [1, 1, 1, 255]);
    assert!(layer.content_bounds().is_some());
    let grid = layer.tiles_mut().unwrap();
    grid.set_pixel(100, 100, [0, 0, 0, 0]);
    assert!(!grid.is_empty(), "the tile is still allocated");
    assert_eq!(layer.content_bounds(), None);
}

fn text_layer() -> Layer {
    let mut layer = Layer::new("T", 64, 64);
    layer.content = LayerContent::Text {
        run: Box::default(),
        tiles: TileGrid::new(64, 64),
    };
    layer
}

/// Every content accessor answers `None` for the two variants it is not about, so a caller
/// that reached for the wrong one gets nothing rather than the wrong thing. This is the
/// contract `is_raster()` being *false* for text depends on — branch on `tiles().is_none()`
/// when you mean "has no pixels".
#[test]
fn each_content_accessor_is_none_for_the_variants_it_is_not_about() {
    let raster = Layer::new("R", 64, 64);
    assert!(raster.content.item().is_none());
    assert!(raster.content.run().is_none());
    assert!(raster.tiles().is_some());

    let mut vector = Layer::vector("V", unbound_item());
    assert!(vector.content.item().is_some());
    assert!(vector.content.item_mut().is_some());
    assert!(vector.content.run().is_none());
    assert!(vector.content.run_mut().is_none());
    assert!(vector.tiles().is_none());

    let mut text = text_layer();
    assert!(text.content.run().is_some());
    assert!(text.content.run_mut().is_some());
    assert!(text.content.item().is_none());
    assert!(text.content.item_mut().is_none());
    assert!(
        text.tiles().is_some(),
        "a text layer has pixels, they are just a cache"
    );
    assert!(
        !text.content.is_raster(),
        "is_raster is false for text, which is why callers ask tiles().is_none()"
    );
}

/// `set_run` belongs to a text layer alone: handing one to a raster or vector layer has to be
/// refused rather than quietly swapping its content out.
#[test]
fn set_run_is_refused_by_a_layer_that_holds_no_run() {
    let mut raster = Layer::new("R", 32, 32);
    assert!(!raster.set_run(Default::default()));
    assert!(raster.content.is_raster(), "it is still a raster layer");

    let mut vector = Layer::vector("V", unbound_item());
    assert!(!vector.set_run(Default::default()));
    assert!(vector.content.is_vector());
}

/// A vector layer has no pixels to measure, so its box is the item's parametric one — already
/// tight, and unchanged by any of this.
#[test]
fn a_vector_layers_bounds_are_its_parametric_box() {
    let layer = Layer::vector("V", stroked_path());
    assert_eq!(layer.content_bounds(), stroked_path().bounds());
}

#[test]
fn an_empty_layer_of_any_kind_has_no_bounds() {
    assert_eq!(Layer::new("R", 64, 64).content_bounds(), None);
    assert_eq!(Layer::vector("V", unbound_item()).content_bounds(), None);
}

/// Blend modes cross the FFI as plain integers, so an unknown value has to come back as
/// `None` rather than silently landing on Normal.
#[test]
fn a_blend_mode_round_trips_through_its_wire_value_and_refuses_anything_else() {
    for mode in [BlendMode::Normal, BlendMode::Multiply, BlendMode::Screen] {
        assert_eq!(BlendMode::from_u32(mode.as_u32()), Some(mode));
    }
    assert_eq!(BlendMode::from_u32(3), None);
    assert_eq!(BlendMode::from_u32(u32::MAX), None);
    assert_eq!(BlendMode::default(), BlendMode::Normal);
}

#[test]
fn doc_rect_to_grid_follows_layer_transform() {
    let mut layer = Layer::new("L", 100, 100);
    let mut tiles = layer.tiles_mut().unwrap();
    tiles.grow_extent(DocRect::new(200, 10, 260, 60));
    tiles.paint_rect(DocRect::new(200, 10, 260, 60), |_, _, _| {
        Some([255, 0, 0, 255])
    });
    layer.transform = Some(LayerTransform {
        offset_x: -150.0,
        offset_y: 0.0,
        ..LayerTransform::default()
    });
    let visible = DocRect::from_size(100, 100);
    let grid_rect = layer.doc_rect_to_grid(visible);
    assert!(
        grid_rect.max_x >= 200,
        "tiles parked off-canvas must map into the viewport once the layer is dragged in"
    );
}

/// Paper is name-matched, and merge-down, click-to-pick and the Filters menu all key off
/// that — so the test is the string, not the position in the stack.
#[test]
fn paper_is_recognised_by_its_name() {
    let mut layer = Layer::new("Paper", 32, 32);
    assert!(layer.is_paper());
    layer.name = "paper".to_string();
    assert!(!layer.is_paper(), "the match is exact, not case-folded");
    layer.name = "Layer 1".to_string();
    assert!(!layer.is_paper());
}
