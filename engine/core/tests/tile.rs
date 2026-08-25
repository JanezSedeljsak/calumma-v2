use calumma_core::layer::*;
use calumma_core::limits::*;
use calumma_core::tile::*;
use std::sync::Arc;

#[test]
fn multiply_blend_darkens_toward_black() {
    let dst = [200, 200, 200, 255];
    let src = [100, 50, 25, 255];
    let out = blend_with_mode(dst, src, BlendMode::Multiply);
    assert_eq!(
        out,
        [
            (200u32 * 100 / 255) as u8,
            (200u32 * 50 / 255) as u8,
            (200u32 * 25 / 255) as u8,
            255
        ]
    );
}

#[test]
fn multiply_by_white_is_a_no_op() {
    let dst = [10, 20, 30, 255];
    let src = [255, 255, 255, 255];
    assert_eq!(blend_with_mode(dst, src, BlendMode::Multiply), dst);
}

#[test]
fn screen_blend_lightens_toward_white() {
    let dst = [50, 50, 50, 255];
    let src = [255, 255, 255, 255];
    assert_eq!(
        blend_with_mode(dst, src, BlendMode::Screen),
        [255, 255, 255, 255]
    );
}

#[test]
fn blend_with_mode_normal_matches_blend_over() {
    let dst = [10, 20, 30, 200];
    let src = [200, 100, 50, 128];
    assert_eq!(
        blend_with_mode(dst, src, BlendMode::Normal),
        blend_over(dst, src)
    );
}

#[test]
fn sparse_until_painted() {
    let mut g = TileGrid::new(8000, 8000);
    assert!(g.is_empty());
    g.set_pixel(10, 10, [255, 0, 0, 255]);
    assert_eq!(g.len(), 1);
    assert_eq!(g.get_pixel(10, 10), [255, 0, 0, 255]);
    assert_eq!(g.get_pixel(300, 300), [0, 0, 0, 0]);
}

#[test]
fn cow_sharing() {
    let mut a = TileGrid::new(512, 512);
    a.set_pixel(1, 1, [1, 2, 3, 4]);
    let coord = TileCoord { x: 0, y: 0 };
    let shared = Arc::clone(a.get(coord).unwrap());
    let mut b = a.clone();
    assert!(Arc::ptr_eq(b.get(coord).unwrap(), &shared));
    b.set_pixel(2, 2, [9, 9, 9, 9]);
    assert!(!Arc::ptr_eq(b.get(coord).unwrap(), a.get(coord).unwrap()));
}

#[test]
fn edge_pixels() {
    let mut g = TileGrid::new(256, 256);
    g.set_pixel(0, 0, [1, 0, 0, 255]);
    g.set_pixel(255, 255, [0, 1, 0, 255]);
    assert_eq!(g.get_pixel(0, 0), [1, 0, 0, 255]);
    assert_eq!(g.get_pixel(255, 255), [0, 1, 0, 255]);
    g.set_pixel(-1, 0, [9, 9, 9, 9]);
    g.set_pixel(256, 0, [9, 9, 9, 9]);
    assert_eq!(g.get_pixel(-1, 0), [0, 0, 0, 0]);
    assert_eq!(g.len(), 1);
}

#[test]
fn stamp_disc_fills_center() {
    let mut g = TileGrid::new(64, 64);
    g.stamp_disc(32.0, 32.0, 3.0, [10, 20, 30, 255]);
    assert_eq!(g.get_pixel(32, 32), [10, 20, 30, 255]);
    assert_eq!(g.get_pixel(0, 0), [0, 0, 0, 0]);
}

#[test]
fn stamp_disc_glazes_translucent_ink() {
    let mut g = TileGrid::new(64, 64);
    g.stamp_disc(32.0, 32.0, 3.0, [10, 20, 30, 128]);
    assert_eq!(g.get_pixel(32, 32), [10, 20, 30, 128]);
    g.stamp_disc(32.0, 32.0, 3.0, [10, 20, 30, 128]);
    let p = g.get_pixel(32, 32);
    assert_eq!(p[0], 10);
    assert!(p[3] > 128);
}

#[test]
fn blend_respects_alpha() {
    let mut g = TileGrid::new(16, 16);
    g.set_pixel(1, 1, [255, 0, 0, 255]);
    g.blend_pixel(1, 1, [0, 0, 255, 128]);
    let p = g.get_pixel(1, 1);
    assert!(p[2] > 100);
    assert!(p[0] > 100);
}

#[test]
fn blend_weights_destination_by_its_alpha() {
    let mut g = TileGrid::new(16, 16);
    g.set_pixel(1, 1, [255, 0, 0, 128]);
    g.blend_pixel(1, 1, [0, 0, 255, 128]);
    assert_eq!(g.get_pixel(1, 1), [85, 0, 170, 192]);
}

#[test]
fn blend_onto_empty_keeps_source() {
    let mut g = TileGrid::new(16, 16);
    g.blend_pixel(2, 2, [10, 20, 30, ALPHA_OPAQUE]);
    assert_eq!(g.get_pixel(2, 2), [10, 20, 30, ALPHA_OPAQUE]);
}

#[test]
fn opaque_blend_is_lossless_when_repeated() {
    let mut g = TileGrid::new(16, 16);
    for _ in 0..64 {
        g.blend_pixel(3, 3, [200, 100, 50, 200]);
    }
    let p = g.get_pixel(3, 3);
    assert_eq!([p[0], p[1], p[2]], [200, 100, 50]);
}

#[test]
fn paint_rect_does_not_allocate_untouched_tiles() {
    let mut g = TileGrid::new(2048, 2048);
    g.paint_rect(DocRect::new(0, 0, 2047, 2047), |x, y, _| {
        if x == 5 && y == 5 {
            Some([1, 2, 3, 255])
        } else {
            None
        }
    });
    assert_eq!(g.len(), 1);
}

#[test]
fn dirty_tracks_only_touched_tiles() {
    let mut g = TileGrid::new(1024, 1024);
    g.set_pixel(10, 10, [1, 2, 3, 255]);
    g.set_pixel(600, 600, [1, 2, 3, 255]);
    assert_eq!(g.dirty_tiles(DirtyChannel::Render).len(), 2);
    g.clear_dirty(DirtyChannel::Render);
    assert!(g.dirty_tiles(DirtyChannel::Render).is_empty());
    g.set_pixel(11, 11, [4, 5, 6, 255]);
    assert_eq!(
        g.dirty_tiles(DirtyChannel::Render)
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        vec![TileCoord { x: 0, y: 0 }]
    );
}

#[test]
fn redundant_write_does_not_dirty() {
    let mut g = TileGrid::new(64, 64);
    g.set_pixel(1, 1, [7, 7, 7, 255]);
    g.clear_dirty(DirtyChannel::Render);
    g.set_pixel(1, 1, [7, 7, 7, 255]);
    assert!(g.dirty_tiles(DirtyChannel::Render).is_empty());
}

#[test]
fn round_trip_through_rgba_buffer() {
    let mut g = TileGrid::new(300, 300);
    g.set_pixel(0, 0, [1, 2, 3, 255]);
    g.set_pixel(299, 299, [4, 5, 6, 255]);
    g.set_pixel(260, 10, [7, 8, 9, 255]);
    let mut buf = vec![0u8; 300 * 300 * 4];
    g.copy_into_rgba(&mut buf, 300, 300);

    let mut back = TileGrid::new(300, 300);
    back.blit_rgba(&buf, 300, 300);
    assert_eq!(back.get_pixel(0, 0), [1, 2, 3, 255]);
    assert_eq!(back.get_pixel(299, 299), [4, 5, 6, 255]);
    assert_eq!(back.get_pixel(260, 10), [7, 8, 9, 255]);
}

#[test]
fn unpremultiply_restores_straight_alpha() {
    let mut rgba = vec![
        128, 64, 32, 128, // half-transparent grey
        10, 20, 30, 255, // opaque stays untouched
        9, 9, 9, 0, // fully transparent clears color
    ];
    unpremultiply_rgba(&mut rgba);
    assert_eq!(&rgba[0..4], &[255, 128, 64, 128]);
    assert_eq!(&rgba[4..8], &[10, 20, 30, 255]);
    assert_eq!(&rgba[8..12], &[0, 0, 0, 0]);
}

#[test]
fn unpremultiply_never_overflows_a_channel() {
    let mut rgba = vec![200, 200, 200, 4];
    unpremultiply_rgba(&mut rgba);
    assert_eq!(rgba, vec![255, 255, 255, 4]);
}

#[test]
fn thumbnail_downsamples_without_full_copy() {
    let mut g = TileGrid::new(400, 200);
    g.set_pixel(0, 0, [10, 20, 30, 255]);
    g.set_pixel(399, 199, [40, 50, 60, 255]);
    let (w, h, rgba) = g.thumbnail(100);
    assert_eq!(w, 100);
    assert_eq!(h, 50);
    assert_eq!(rgba.len(), 100 * 50 * 4);
    assert_eq!(&rgba[0..4], &[10, 20, 30, 255]);
    let last = (50 - 1) * 100 * 4 + (100 - 1) * 4;
    assert_eq!(&rgba[last..last + 4], &[40, 50, 60, 255]);
}

#[test]
fn expanded_by_tiles_grows_the_rect_by_whole_tiles() {
    let rect = DocRect::new(100, 200, 300, 400);
    let grown = rect.expanded_by_tiles(2);
    let m = 2 * TILE_SIZE as i32;
    assert_eq!(grown.min_x, 100 - m);
    assert_eq!(grown.min_y, 200 - m);
    assert_eq!(grown.max_x, 300 + m);
    assert_eq!(grown.max_y, 400 + m);
    assert_eq!(rect.expanded_by_tiles(0), rect);
}

#[test]
fn intersects_agrees_with_intersect() {
    let a = DocRect::new(0, 0, 100, 100);
    let touching = DocRect::new(100, 100, 200, 200);
    let apart = DocRect::new(101, 101, 200, 200);
    assert!(a.intersects(touching));
    assert!(a.intersect(touching).is_some());
    assert!(!a.intersects(apart));
    assert!(a.intersect(apart).is_none());
}

#[test]
fn coords_intersecting_skips_empty_cells_and_out_of_map_tiles() {
    let mut grid = TileGrid::new(1024, 1024);
    let a = TileCoord { x: 1, y: 1 };
    let b = TileCoord { x: 3, y: 1 };
    grid.set_pixel(300, 300, [1, 2, 3, 255]);
    grid.set_pixel(900, 300, [4, 5, 6, 255]);
    let rect = DocRect::new(256, 256, 1023, 511);
    let found: Vec<_> = grid.coords_intersecting(rect).collect();
    assert_eq!(found, vec![a, b]);
    assert!(grid
        .coords_intersecting(DocRect::new(0, 0, 255, 255))
        .next()
        .is_none());
}

#[test]
fn tile_rect_covers_exactly_one_tile() {
    let rect = TileGrid::tile_rect(TileCoord { x: 2, y: 3 });
    let ts = TILE_SIZE as i32;
    assert_eq!(rect.min_x, 2 * ts);
    assert_eq!(rect.min_y, 3 * ts);
    assert_eq!(rect.max_x, 3 * ts - 1);
    assert_eq!(rect.max_y, 4 * ts - 1);
}

#[test]
fn clear_dirty_tile_only_clears_that_tile_on_that_channel() {
    let mut grid = TileGrid::new(1024, 1024);
    let a = TileCoord { x: 0, y: 0 };
    let b = TileCoord { x: 1, y: 0 };
    grid.mark_dirty(a);
    grid.mark_dirty(b);
    grid.clear_dirty_tile(DirtyChannel::Render, a);
    assert!(!grid.dirty_tiles(DirtyChannel::Render).contains(&a));
    assert!(grid.dirty_tiles(DirtyChannel::Render).contains(&b));
    assert!(grid.dirty_tiles(DirtyChannel::Store).contains(&a));
}

#[test]
fn mark_channel_dirty_leaves_the_other_channel_alone() {
    let mut grid = TileGrid::new(1024, 1024);
    grid.set_pixel(10, 10, [1, 2, 3, 255]);
    grid.set_pixel(600, 600, [4, 5, 6, 255]);
    grid.clear_dirty(DirtyChannel::Render);
    grid.clear_dirty(DirtyChannel::Store);
    grid.mark_channel_dirty(DirtyChannel::Render);
    assert_eq!(grid.dirty_tiles(DirtyChannel::Render).len(), 2);
    assert!(grid.dirty_tiles(DirtyChannel::Store).is_empty());
}

#[test]
fn memory_bytes_tracks_allocated_tiles() {
    let mut grid = TileGrid::new(1024, 1024);
    assert_eq!(grid.memory_bytes(), 0);
    grid.set_pixel(5, 5, [1, 2, 3, 255]);
    let one = grid.memory_bytes();
    assert_eq!(one, TILE_BYTES);
    grid.set_pixel(5 + TILE_SIZE as i32, 5, [1, 2, 3, 255]);
    assert_eq!(grid.memory_bytes(), one * 2);
}

#[test]
fn stamp_disc_erase_clears_inside_the_radius_only() {
    let mut grid = TileGrid::new(256, 256);
    for y in 0..40 {
        for x in 0..40 {
            grid.set_pixel(x, y, [9, 9, 9, 255]);
        }
    }
    let touched = grid.stamp_disc_erase(20.0, 20.0, 8.0);
    assert!(touched > 0);
    assert_eq!(grid.get_pixel(20, 20), [0, 0, 0, 0]);
    assert_eq!(grid.get_pixel(39, 39), [9, 9, 9, 255]);
    assert_eq!(grid.stamp_disc_erase(20.0, 20.0, 0.0), 0);
}

#[test]
fn whole_tiles_share_one_arc_detects_unpainted_fill() {
    let side = TILE_SIZE * 2;
    let mut grid = TileGrid::new(side + 9, side + 9);
    grid.fill_uniform(DocRect::from_size(side + 9, side + 9), [255, 255, 255, 255]);
    assert!(grid.whole_tiles_share_one_arc());
    grid.set_pixel(1, 1, [200, 200, 200, 255]);
    assert!(!grid.whole_tiles_share_one_arc());
}

#[test]
fn opaque_bounds_spans_every_tile_that_shares_one_buffer() {
    let side = TILE_SIZE * 2;
    let extent = side + 9;
    let mut grid = TileGrid::new(extent, extent);
    grid.fill_uniform(DocRect::from_size(extent, extent), [255, 255, 255, 255]);
    assert!(grid.whole_tiles_share_one_arc());
    let b = grid
        .opaque_bounds()
        .expect("a filled grid is opaque somewhere");
    let last = extent as i32 - 1;
    assert_eq!((b.min_x, b.min_y, b.max_x, b.max_y), (0, 0, last, last));
}

#[test]
fn opaque_bounds_ignores_pixels_past_the_document_edge() {
    let extent = TILE_SIZE + 4;
    let mut grid = TileGrid::new(extent, extent);
    grid.set_pixel(3, 5, [1, 2, 3, 255]);
    let far = extent as i32 - 1;
    grid.set_pixel(far, far, [4, 5, 6, 255]);
    grid.set_pixel(far + 20, far + 20, [7, 8, 9, 255]);
    let b = grid.opaque_bounds().expect("painted pixels have bounds");
    assert_eq!((b.min_x, b.min_y, b.max_x, b.max_y), (3, 5, far, far));
}

/// The box is a cache, so every one of these is really asking whether the cache noticed. A
/// stale answer here is a transform frame drawn in the wrong place and a pivot that scales
/// about the wrong point, so the invalidation has to cover every way pixels move.
#[test]
fn opaque_bounds_follows_paint_erase_and_undo() {
    let mut grid = TileGrid::new(1024, 1024);
    assert_eq!(grid.opaque_bounds(), None);

    grid.set_pixel(100, 100, [1, 2, 3, 255]);
    assert_eq!(grid.opaque_bounds(), Some(DocRect::new(100, 100, 100, 100)));

    grid.set_pixel(700, 400, [1, 2, 3, 255]);
    assert_eq!(
        grid.opaque_bounds(),
        Some(DocRect::new(100, 100, 700, 400)),
        "painting into a second tile grows the box"
    );

    let snapshot = grid.snapshot_tiles(&[TileCoord::from_doc_i32(700, 400)]);
    grid.set_pixel(700, 400, [0, 0, 0, 0]);
    assert_eq!(
        grid.opaque_bounds(),
        Some(DocRect::new(100, 100, 100, 100)),
        "erasing shrinks it back"
    );

    grid.restore_tiles(&snapshot);
    assert_eq!(
        grid.opaque_bounds(),
        Some(DocRect::new(100, 100, 700, 400)),
        "and undo puts it back"
    );

    grid.clear();
    assert_eq!(grid.opaque_bounds(), None);
}

/// A tile nothing touched keeps its cached scan, which is the whole reason an edit on a large
/// layer is cheap — but it must not keep a *stale* one when its own pixels change.
#[test]
fn a_second_edit_in_one_tile_still_moves_the_box() {
    let mut grid = TileGrid::new(1024, 1024);
    grid.set_pixel(300, 300, [1, 2, 3, 255]);
    grid.set_pixel(900, 900, [1, 2, 3, 255]);
    assert_eq!(grid.opaque_bounds(), Some(DocRect::new(300, 300, 900, 900)));
    grid.set_pixel(310, 290, [1, 2, 3, 255]);
    assert_eq!(
        grid.opaque_bounds(),
        Some(DocRect::new(300, 290, 900, 900)),
        "the edited tile was rescanned"
    );
}

/// The scan is clipped to the document, so shrinking one changes the answer without a pixel
/// being touched — the case a dirty-tile invalidation would miss entirely.
#[test]
fn resizing_the_grid_reclips_the_box() {
    let mut grid = TileGrid::new(1024, 1024);
    grid.set_pixel(10, 10, [1, 2, 3, 255]);
    grid.set_pixel(900, 900, [1, 2, 3, 255]);
    assert_eq!(grid.opaque_bounds(), Some(DocRect::new(10, 10, 900, 900)));
    grid.set_size(512, 512);
    assert_eq!(
        grid.opaque_bounds(),
        Some(DocRect::new(10, 10, 10, 10)),
        "the far pixel is off-canvas now, though it is still stored"
    );
    grid.set_size(1024, 1024);
    assert_eq!(
        grid.opaque_bounds(),
        Some(DocRect::new(10, 10, 900, 900)),
        "and growing back finds it again"
    );
}

/// History clones grids constantly; a clone that shared a cache with its original by accident
/// would answer for the wrong pixels the moment either one was painted on.
#[test]
fn a_cloned_grid_keeps_its_own_box() {
    let mut grid = TileGrid::new(512, 512);
    grid.set_pixel(50, 50, [1, 2, 3, 255]);
    let before = grid.opaque_bounds();
    let mut copy = grid.clone();
    assert_eq!(copy.opaque_bounds(), before);
    copy.set_pixel(400, 400, [1, 2, 3, 255]);
    assert_eq!(copy.opaque_bounds(), Some(DocRect::new(50, 50, 400, 400)));
    assert_eq!(grid.opaque_bounds(), before, "the original is untouched");
}

#[test]
fn content_revision_moves_only_when_pixels_do() {
    let mut grid = TileGrid::new(256, 256);
    let start = grid.content_revision();

    grid.set_pixel(10, 10, [1, 2, 3, 255]);
    let painted = grid.content_revision();
    assert_ne!(painted, start, "painting is a content change");

    let preview = grid.preview();
    assert_eq!(
        grid.content_revision(),
        painted,
        "asking for the preview is not a content change"
    );
    assert!(Arc::ptr_eq(&preview, &grid.preview()), "and it is cached");

    grid.mark_channel_dirty(DirtyChannel::Render);
    assert_eq!(
        grid.content_revision(),
        painted,
        "an opacity or adjustment change re-renders but repaints nothing"
    );

    grid.set_pixel(20, 20, [4, 5, 6, 255]);
    assert_ne!(grid.content_revision(), painted, "the next stroke does");
}
