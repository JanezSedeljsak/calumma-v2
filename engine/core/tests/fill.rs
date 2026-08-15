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
