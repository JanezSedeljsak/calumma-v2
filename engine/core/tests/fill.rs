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
