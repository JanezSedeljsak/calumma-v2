use calumma_core::memory::document_memory;
use calumma_core::tile::{DocRect, TileGrid, TILE_BYTES, TILE_SIZE};
use calumma_core::*;

const SIDE: u32 = TILE_SIZE * 8;

fn doc() -> Document {
    Document::new("p".into(), "t", SIDE, SIDE)
}

#[test]
fn a_uniform_fill_shares_one_allocation_across_every_whole_tile() {
    let mut grid = TileGrid::new(SIDE, SIDE);
    let touched = grid.fill_uniform(DocRect::from_size(SIDE, SIDE), [255, 255, 255, 255]);
    assert_eq!(touched, 64, "8x8 tiles covered");

    let first = grid.get(TileCoord { x: 0, y: 0 }).unwrap();
    let other = grid.get(TileCoord { x: 3, y: 5 }).unwrap();
    assert!(
        std::sync::Arc::ptr_eq(first, other),
        "whole tiles of one colour share their pixels"
    );
    assert_eq!(grid.get_pixel(900, 900), [255, 255, 255, 255]);
}

#[test]
fn painting_a_shared_tile_forks_only_that_tile() {
    let mut grid = TileGrid::new(SIDE, SIDE);
    grid.fill_uniform(DocRect::from_size(SIDE, SIDE), [255, 255, 255, 255]);
    grid.set_pixel(10, 10, [0, 0, 0, 255]);

    let painted = grid.get(TileCoord { x: 0, y: 0 }).unwrap();
    let untouched = grid.get(TileCoord { x: 1, y: 0 }).unwrap();
    let other = grid.get(TileCoord { x: 2, y: 0 }).unwrap();
    assert!(
        !std::sync::Arc::ptr_eq(painted, untouched),
        "the edited tile forked"
    );
    assert!(
        std::sync::Arc::ptr_eq(untouched, other),
        "the rest still share"
    );
    assert_eq!(grid.get_pixel(10, 10), [0, 0, 0, 255]);
    assert_eq!(grid.get_pixel(300, 10), [255, 255, 255, 255]);
}

#[test]
fn a_partially_covered_tile_keeps_its_own_pixels() {
    let mut grid = TileGrid::new(TILE_SIZE * 2, TILE_SIZE * 2);
    grid.fill_uniform(DocRect::new(0, 0, TILE_SIZE as i32 + 9, 9), [1, 2, 3, 255]);
    assert_eq!(grid.get_pixel(TILE_SIZE as i32 + 5, 5), [1, 2, 3, 255]);
    assert_eq!(
        grid.get_pixel(TILE_SIZE as i32 + 5, 40),
        [0, 0, 0, 0],
        "outside the filled rect stays transparent"
    );
}

#[test]
fn paper_costs_one_tile_not_one_per_tile() {
    let doc = doc();
    let report = document_memory(&doc);
    assert_eq!(
        report.tile_bytes, TILE_BYTES,
        "a full white Paper layer is one allocation"
    );
    assert_eq!(report.tile_count, 64);
    assert_eq!(report.shared_tile_count, 63);
}

#[test]
fn painting_grows_the_report_by_the_tiles_it_forks() {
    let mut doc = doc();
    let before = document_memory(&doc).tile_bytes;
    doc.layers[0]
        .tiles_mut()
        .unwrap()
        .set_pixel(10, 10, [0, 0, 0, 255]);
    let after = document_memory(&doc).tile_bytes;
    assert_eq!(after, before + TILE_BYTES, "exactly one tile forked");
}

#[test]
fn history_only_charges_for_what_it_alone_holds() {
    let mut doc = doc();
    doc.resize_viewport(SIDE as f32, SIDE as f32, 1.0);
    doc.fit_to_view();
    doc.tool = Tool::Pen;
    doc.pointer_down(20.0, 20.0);
    doc.pointer_move(40.0, 40.0);
    doc.pointer_up(40.0, 40.0);

    let report = document_memory(&doc);
    assert!(doc.history.can_undo(), "the stroke landed in history");
    assert_eq!(
        report.history_bytes, 0,
        "the snapshot is the empty pre-stroke state, which owns nothing"
    );
    assert!(report.total() >= report.tile_bytes);
}

#[test]
fn masks_and_vectors_are_counted_where_they_live() {
    let mut doc = doc();
    let index = doc.add_vector_layer("V");
    doc.layers[index]
        .content
        .items_mut()
        .unwrap()
        .push(VectorItem::Path(VectorPath {
            points: vec![(0.0, 0.0); 100],
            closed: false,
            fill: false,
            color: [0, 0, 0, 255],
            stroke_width: 2.0,
        }));
    doc.layers[0].set_mask(Some(vec![255; (SIDE * SIDE) as usize]));

    let report = document_memory(&doc);
    assert_eq!(report.mask_bytes, (SIDE * SIDE) as usize);
    assert!(report.vector_bytes >= 100 * std::mem::size_of::<(f32, f32)>());
}
