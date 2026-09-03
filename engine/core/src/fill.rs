use crate::selection::Selection;
use crate::selection_mask::SelectionMask;
use crate::tile::{blend_over, DocRect, TileGrid};
use std::collections::VecDeque;

pub(crate) fn color_distance(a: [u8; 4], b: [u8; 4]) -> u32 {
    let dr = a[0] as i32 - b[0] as i32;
    let dg = a[1] as i32 - b[1] as i32;
    let db = a[2] as i32 - b[2] as i32;
    let da = a[3] as i32 - b[3] as i32;
    (dr * dr + dg * dg + db * db + da * da) as u32
}

/// The traversal itself: which pixels are contiguous with `(start_x, start_y)` and within
/// `tolerance` of its color.
///
/// The bucket and the magic wand are the same walk with different endings — one paints what it
/// reached, the other selects it — so they share this rather than each carrying a copy. A wand
/// that disagreed with the bucket about what "contiguous" or "within tolerance" means would be
/// a bug report, and two implementations is how that happens.
///
/// Tolerance is squared Euclidean distance over all four channels, alpha included: a
/// transparent region reads as its own color, which is what makes the wand able to select the
/// empty space around a sketch.
///
/// The two bitmaps double as the visited set this walk used to keep in a hash set — `visited`
/// marks enqueued, `reached` marks passed the tolerance test — at a bit per pixel rather than
/// a 64-bit hash entry per pixel, which is what lets the wand flood a whole document without
/// the bookkeeping outweighing the document.
pub fn flood_region_pixels<F>(
    scope: DocRect,
    start_x: i32,
    start_y: i32,
    tolerance: u8,
    mut pixel: F,
) -> Option<SelectionMask>
where
    F: FnMut(i32, i32) -> [u8; 4],
{
    if !scope.contains(start_x, start_y) || scope.is_empty() {
        return None;
    }
    let target = pixel(start_x, start_y);
    let tol2 = (tolerance as u32) * (tolerance as u32) * 4;
    let origin = (scope.min_x, scope.min_y);
    let width = (scope.max_x - scope.min_x + 1) as u32;
    let height = (scope.max_y - scope.min_y + 1) as u32;

    let mut visited = SelectionMask::new(origin, width, height);
    let mut reached = SelectionMask::new(origin, width, height);
    let mut queue = VecDeque::new();
    queue.push_back((start_x, start_y));
    visited.set(start_x, start_y);

    while let Some((x, y)) = queue.pop_front() {
        if color_distance(pixel(x, y), target) > tol2 {
            continue;
        }
        reached.set(x, y);
        for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
            if !scope.contains(nx, ny) || visited.get(nx, ny) {
                continue;
            }
            visited.set(nx, ny);
            queue.push_back((nx, ny));
        }
    }
    reached.finish()
}

pub fn flood_region(
    tiles: &TileGrid,
    scope: DocRect,
    start_x: i32,
    start_y: i32,
    selection: Option<&Selection>,
    tolerance: u8,
) -> Option<SelectionMask> {
    if !scope.contains(start_x, start_y) || scope.is_empty() {
        return None;
    }
    let target = tiles.get_pixel(start_x, start_y);
    let tol2 = (tolerance as u32) * (tolerance as u32) * 4;
    let origin = (scope.min_x, scope.min_y);
    let width = (scope.max_x - scope.min_x + 1) as u32;
    let height = (scope.max_y - scope.min_y + 1) as u32;

    let mut visited = SelectionMask::new(origin, width, height);
    let mut reached = SelectionMask::new(origin, width, height);
    let mut queue = VecDeque::new();
    queue.push_back((start_x, start_y));
    visited.set(start_x, start_y);

    while let Some((x, y)) = queue.pop_front() {
        if color_distance(tiles.get_pixel(x, y), target) > tol2 {
            continue;
        }
        reached.set(x, y);
        for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
            if !scope.contains(nx, ny) || visited.get(nx, ny) {
                continue;
            }
            if let Some(sel) = selection {
                if !sel.contains(nx as f32 + 0.5, ny as f32 + 0.5) {
                    continue;
                }
            }
            visited.set(nx, ny);
            queue.push_back((nx, ny));
        }
    }
    reached.finish()
}

pub fn color_range_pixels<F>(
    scope: DocRect,
    target: [u8; 4],
    tolerance: u8,
    pixel: F,
) -> Option<SelectionMask>
where
    F: Fn(i32, i32) -> [u8; 4] + Sync,
{
    if scope.is_empty() {
        return None;
    }
    let tol2 = (tolerance as u32) * (tolerance as u32) * 4;
    let width = (scope.max_x - scope.min_x + 1) as u32;
    let height = (scope.max_y - scope.min_y + 1) as u32;
    SelectionMask::from_predicate((scope.min_x, scope.min_y), width, height, |x, y| {
        color_distance(pixel(x, y), target) <= tol2
    })
    .finish()
}

/// Traverse, then paint what was reached. Returns the pixel count so the caller can tell a
/// real edit from a click that landed on the fill color already.
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
    if color[3] == 0 {
        return 0;
    }
    if !bounds.contains(start_x, start_y) {
        return 0;
    }
    if tiles.get_pixel(start_x, start_y) == color {
        return 0;
    }
    let Some(region) = flood_region(tiles, bounds, start_x, start_y, selection, tolerance) else {
        return 0;
    };
    let mut painted = 0usize;
    tiles.paint_rect(region.bounds(), |x, y, dst| {
        if !region.get(x, y) {
            return None;
        }
        painted += 1;
        Some(blend_over(dst, color))
    });
    painted
}
