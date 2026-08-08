use crate::selection::Selection;
use crate::tile::{DocRect, TileGrid};
use std::collections::{HashSet, VecDeque};

fn color_distance(a: [u8; 4], b: [u8; 4]) -> u32 {
    let dr = a[0] as i32 - b[0] as i32;
    let dg = a[1] as i32 - b[1] as i32;
    let db = a[2] as i32 - b[2] as i32;
    let da = a[3] as i32 - b[3] as i32;
    (dr * dr + dg * dg + db * db + da * da) as u32
}

#[allow(clippy::too_many_arguments)]
pub fn flood_fill(
    tiles: &mut TileGrid,
    bounds: DocRect,
    start_x: i32,
    start_y: i32,
    color: [u8; 4],
    selection: Option<&Selection>,
    tolerance: u8,
) -> usize {
    if !bounds.contains(start_x, start_y) {
        return 0;
    }
    let target = tiles.get_pixel(start_x, start_y);
    if target == color {
        return 0;
    }
    let tol2 = (tolerance as u32) * (tolerance as u32) * 4;
    let mut visited: HashSet<(i32, i32)> = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back((start_x, start_y));
    visited.insert((start_x, start_y));
    let mut touched = 0usize;
    while let Some((x, y)) = queue.pop_front() {
        let current = tiles.get_pixel(x, y);
        if color_distance(current, target) > tol2 {
            continue;
        }
        tiles.set_pixel(x, y, color);
        touched += 1;
        for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
            if !bounds.contains(nx, ny) || visited.contains(&(nx, ny)) {
                continue;
            }
            if let Some(sel) = selection {
                if !sel.contains(nx as f32 + 0.5, ny as f32 + 0.5) {
                    continue;
                }
            }
            visited.insert((nx, ny));
            queue.push_back((nx, ny));
        }
    }
    touched
}
