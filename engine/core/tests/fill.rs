use calumma_core::fill::*;
use calumma_core::limits::TOLERANCE_DEFAULT;
use calumma_core::selection::*;
use calumma_core::shape::{ink_sample, Shape, Tool};
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
fn fill_bleeds_through_antialiased_stroke_fringe() {
    let mut grid = TileGrid::new(32, 32);
    let bounds = DocRect::from_size(32, 32);
    for y in 0..32 {
        for x in 0..32 {
            grid.set_pixel(x, y, [255, 255, 255, 255]);
        }
    }
    for y in 6..26 {
        for x in 6..26 {
            let dx = x - 16;
            let dy = y - 16;
            let d = ((dx * dx + dy * dy) as f32).sqrt();
            if d > 8.0 && d < 9.5 {
                grid.set_pixel(x, y, [255, 255, 255, 255]);
            } else if d > 9.0 && d < 10.5 {
                grid.set_pixel(x, y, [0, 0, 0, 60]);
            } else if (10.5..11.5).contains(&d) {
                grid.set_pixel(x, y, [0, 0, 0, 255]);
            }
        }
    }
    let touched = flood_fill(&mut grid, bounds, 16, 16, [200, 0, 0, 255], None, 0);
    assert!(touched > 0);
    assert_eq!(grid.get_pixel(16, 16)[0], 200);
    assert_eq!(
        grid.get_pixel(16, 9)[0],
        200,
        "fill should reach through soft AA and any opaque white pocket under the stroke"
    );
    assert_eq!(
        grid.get_pixel(16, 8)[0],
        200,
        "opaque white ring inside the stroke should be absorbed"
    );
}

#[test]
fn color_range_pixels_refuses_an_empty_scope() {
    let empty = DocRect::from_size(0, 0);
    assert!(color_range_pixels(empty, [0, 0, 0, 255], 4, |_, _| [0, 0, 0, 255]).is_none());
}

fn stamp_shape(grid: &mut TileGrid, shape: Shape, fill: [u8; 4], stroke: [u8; 4]) {
    let (x0, y0, x1, y1) = shape.bounds();
    let rect = DocRect::from_floats(x0, y0, x1, y1);
    grid.paint_rect(rect, |px, py, dst| {
        let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
        let parts = [
            ink_sample(shape.fill_distance(x, y), fill),
            ink_sample(shape.stroke_distance(x, y), stroke),
        ];
        let mut out = dst;
        let mut inked = false;
        for src in parts.into_iter().flatten() {
            out = blend_over(out, src);
            inked = true;
        }
        inked.then_some(out)
    });
}

fn outlined_ellipse(start: (f32, f32), end: (f32, f32)) -> Shape {
    Shape {
        tool: Tool::Ellipse,
        start,
        end,
        half_width: 4.0,
        fill: false,
        stroke: true,
    }
}

/// A filled circle already on the layer must not become a pipe that lets the bucket spill
/// out of a second outlined circle. The walk stops at the new outline, not at the old fill.
#[test]
fn filling_a_second_outlined_circle_stays_inside_it() {
    let mut grid = TileGrid::new(128, 128);
    let bounds = DocRect::from_size(128, 128);
    let ink = [26, 26, 26, 255];
    stamp_shape(
        &mut grid,
        Shape {
            tool: Tool::Ellipse,
            start: (8.0, 8.0),
            end: (48.0, 48.0),
            half_width: 4.0,
            fill: true,
            stroke: true,
        },
        [200, 30, 30, 255],
        ink,
    );
    stamp_shape(
        &mut grid,
        outlined_ellipse((56.0, 56.0), (112.0, 112.0)),
        ink,
        ink,
    );
    let touched = flood_fill(
        &mut grid,
        bounds,
        84,
        84,
        [0, 90, 200, 255],
        None,
        TOLERANCE_DEFAULT,
    );
    assert!(
        touched > 0,
        "the new circle's interior is empty and should fill"
    );
    assert_eq!(grid.get_pixel(84, 84)[2], 200, "clicked interior is filled");
    assert_eq!(
        grid.get_pixel(4, 4),
        [0, 0, 0, 0],
        "outside both circles stays empty"
    );
    assert_eq!(
        grid.get_pixel(28, 28)[0],
        200,
        "the first filled circle is left alone"
    );
}

/// Same walk on an already-opaque field (Paper): the second outline still contains the fill,
/// instead of the fringe growth leaking through antialiased stroke into the rest of the paper.
#[test]
fn filling_a_second_outlined_circle_on_paper_stays_inside_it() {
    let mut grid = TileGrid::new(128, 128);
    let bounds = DocRect::from_size(128, 128);
    grid.fill_uniform(bounds, [255, 255, 255, 255]);
    let ink = [26, 26, 26, 255];
    stamp_shape(
        &mut grid,
        Shape {
            tool: Tool::Ellipse,
            start: (8.0, 8.0),
            end: (48.0, 48.0),
            half_width: 4.0,
            fill: true,
            stroke: true,
        },
        [200, 30, 30, 255],
        ink,
    );
    stamp_shape(
        &mut grid,
        outlined_ellipse((56.0, 56.0), (112.0, 112.0)),
        ink,
        ink,
    );
    let touched = flood_fill(
        &mut grid,
        bounds,
        84,
        84,
        [0, 90, 200, 255],
        None,
        TOLERANCE_DEFAULT,
    );
    assert!(touched > 0);
    assert_eq!(grid.get_pixel(84, 84)[2], 200);
    assert_eq!(
        grid.get_pixel(4, 4),
        [255, 255, 255, 255],
        "paper outside the new circle is not filled"
    );
    assert_eq!(grid.get_pixel(28, 28)[0], 200);
}

fn grid_rgba(grid: &TileGrid, width: u32, height: u32) -> Vec<u8> {
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let px = grid.get_pixel(x, y);
            let i = ((y as u32 * width + x as u32) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&px);
        }
    }
    rgba
}

fn sample_rgba(rgba: &[u8], width: u32, height: u32, x: i32, y: i32) -> [u8; 4] {
    if x < 0 || y < 0 {
        return [0; 4];
    }
    let (x, y) = (x as u32, y as u32);
    if x >= width || y >= height {
        return [0; 4];
    }
    let i = ((y * width + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

/// A hand-drawn loop is rarely sealed to the pixel. A 2 px slit in an otherwise closed
/// outline still has to contain the fill, otherwise "fill this oval" paints the whole layer.
#[test]
fn filling_a_loop_with_a_small_gap_stays_inside() {
    let mut grid = TileGrid::new(128, 128);
    let bounds = DocRect::from_size(128, 128);
    let ink = [26, 26, 26, 255];
    stamp_shape(
        &mut grid,
        outlined_ellipse((16.0, 16.0), (112.0, 112.0)),
        ink,
        ink,
    );
    for y in 63..=64 {
        for x in 107..=117 {
            grid.set_pixel(x, y, [0, 0, 0, 0]);
        }
    }
    let rgba = grid_rgba(&grid, 128, 128);
    let touched = flood_fill_sampled(
        &mut grid,
        bounds,
        64,
        64,
        [0, 90, 200, 255],
        None,
        TOLERANCE_DEFAULT,
        |x, y| sample_rgba(&rgba, 128, 128, x, y),
    );
    assert!(touched > 0, "the interior should fill");
    assert_eq!(grid.get_pixel(64, 64)[2], 200);
    assert_eq!(
        grid.get_pixel(4, 4),
        [0, 0, 0, 0],
        "a 2 px gap in the outline must not spill the fill"
    );
}
