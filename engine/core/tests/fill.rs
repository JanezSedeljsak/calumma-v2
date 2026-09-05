use calumma_core::fill::*;
use calumma_core::selection::*;
use calumma_core::tile::*;

#[test]
fn fills_contiguous_region_only() {
    let mut grid = TileGrid::new(16, 16);
    for y in 0..8 {
        for x in 0..8 {
            grid.set_pixel(x, y, [10, 10, 10, 255]);
        }
    }
    let bounds = DocRect::from_size(16, 16);
    let touched = flood_fill(&mut grid, bounds, 3, 3, [200, 0, 0, 255], None, 4);
    assert_eq!(touched, 64);
    assert_eq!(grid.get_pixel(3, 3), [200, 0, 0, 255]);
    assert_eq!(grid.get_pixel(9, 9), [0, 0, 0, 0]);
}

#[test]
fn respects_selection_boundary() {
    let mut grid = TileGrid::new(16, 16);
    let bounds = DocRect::from_size(16, 16);
    let selection = Selection {
        shape: calumma_core::selection::SelectionShape::Rect {
            start: (0.0, 0.0),
            end: (4.0, 16.0),
        },
    };
    let touched = flood_fill(&mut grid, bounds, 0, 0, [1, 2, 3, 255], Some(&selection), 4);
    assert!(touched > 0);
    assert_eq!(grid.get_pixel(0, 0), [1, 2, 3, 255]);
    assert_eq!(grid.get_pixel(10, 0), [0, 0, 0, 0]);
}

#[test]
fn same_color_click_is_a_no_op() {
    let mut grid = TileGrid::new(8, 8);
    let bounds = DocRect::from_size(8, 8);
    let touched = flood_fill(&mut grid, bounds, 2, 2, [0, 0, 0, 0], None, 4);
    assert_eq!(touched, 0);
}

#[test]
fn translucent_fill_keeps_source_alpha_on_empty() {
    let mut grid = TileGrid::new(8, 8);
    let bounds = DocRect::from_size(8, 8);
    let touched = flood_fill(&mut grid, bounds, 0, 0, [200, 0, 0, 128], None, 4);
    assert_eq!(touched, 64);
    assert_eq!(grid.get_pixel(0, 0), [200, 0, 0, 128]);
    assert_eq!(grid.get_pixel(7, 7), [200, 0, 0, 128]);
}

#[test]
fn zero_alpha_fill_is_a_no_op() {
    let mut grid = TileGrid::new(8, 8);
    grid.set_pixel(1, 1, [10, 20, 30, 255]);
    let bounds = DocRect::from_size(8, 8);
    assert_eq!(
        flood_fill(&mut grid, bounds, 1, 1, [1, 2, 3, 0], None, 4),
        0
    );
    assert_eq!(grid.get_pixel(1, 1), [10, 20, 30, 255]);
}

#[test]
fn clicking_outside_the_bounds_is_a_no_op() {
    let mut grid = TileGrid::new(8, 8);
    let bounds = DocRect::from_size(8, 8);
    assert_eq!(
        flood_fill(&mut grid, bounds, -1, 0, [1, 2, 3, 255], None, 4),
        0
    );
    assert_eq!(
        flood_fill(&mut grid, bounds, 0, 20, [1, 2, 3, 255], None, 4),
        0
    );
}

/// `paint_rect` walks `region.bounds()` — the reached shape's rectangular bounding box, not
/// the shape itself — so a diagonal or L-shaped region has to gate every pixel in that box
/// against the mask, or the corners the flood never actually reached would get painted too.
#[test]
fn a_non_rectangular_region_leaves_its_bounding_boxs_corners_untouched() {
    let mut grid = TileGrid::new(8, 8);
    // A diagonal staircase of connected pixels: (0,0)-(1,0)-(1,1)-(2,1)-(2,2). Four-connected,
    // so this is exactly one contiguous region, and its bounding box (0..=2, 0..=2) contains
    // pixels — like (2,0) and (0,2) — the walk never actually reaches.
    for (x, y) in [(0, 0), (1, 0), (1, 1), (2, 1), (2, 2)] {
        grid.set_pixel(x, y, [10, 10, 10, 255]);
    }
    let bounds = DocRect::from_size(8, 8);
    let touched = flood_fill(&mut grid, bounds, 0, 0, [200, 0, 0, 255], None, 0);
    assert_eq!(touched, 5, "only the staircase itself");
    assert_eq!(
        grid.get_pixel(2, 0),
        [0, 0, 0, 0],
        "corner of the bbox, never reached"
    );
    assert_eq!(
        grid.get_pixel(0, 2),
        [0, 0, 0, 0],
        "the other corner, never reached"
    );
    assert_eq!(
        grid.get_pixel(1, 1),
        [200, 0, 0, 255],
        "the staircase itself is painted"
    );
}

#[test]
fn flood_region_pixels_refuses_a_start_point_outside_the_scope() {
    let scope = DocRect::from_size(8, 8);
    assert!(flood_region_pixels(scope, -1, 0, 4, |_, _| [0, 0, 0, 0]).is_none());
    assert!(flood_region_pixels(DocRect::from_size(0, 0), 0, 0, 4, |_, _| [0, 0, 0, 0]).is_none());
    assert!(flood_region_pixels(scope, 0, 0, 4, |_, _| [0, 0, 0, 0]).is_some());
}

#[test]
fn color_range_pixels_refuses_an_empty_scope() {
    let empty = DocRect::from_size(0, 0);
    assert!(color_range_pixels(empty, [0, 0, 0, 255], 4, |_, _| [0, 0, 0, 255]).is_none());
}
