use crate::limits::{FILL_BARRIER_ALPHA, FILL_FRINGE_MAX_STEPS, FILL_GAP_CLOSE_PX};
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

fn tol2(tolerance: u8) -> u32 {
    (tolerance as u32) * (tolerance as u32) * 4
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
    pixel: F,
) -> Option<SelectionMask>
where
    F: FnMut(i32, i32) -> [u8; 4],
{
    flood(scope, start_x, start_y, tolerance, pixel, |_, _| true)
}

fn grow_one_step(
    working: &SelectionMask,
    selection: Option<&Selection>,
    may_enter: &impl Fn([u8; 4]) -> bool,
    pixel: &impl Fn(i32, i32) -> [u8; 4],
) -> (SelectionMask, bool) {
    let b = working.bounds();
    let expanded = DocRect::new(b.min_x - 1, b.min_y - 1, b.max_x + 1, b.max_y + 1);
    let width = (expanded.max_x - expanded.min_x + 1) as u32;
    let height = (expanded.max_y - expanded.min_y + 1) as u32;
    let mut next = SelectionMask::new((expanded.min_x, expanded.min_y), width, height);
    let mut grew = false;
    for y in expanded.min_y..=expanded.max_y {
        for x in expanded.min_x..=expanded.max_x {
            if working.get(x, y) {
                next.set(x, y);
                continue;
            }
            if let Some(sel) = selection {
                if !sel.contains(x as f32 + 0.5, y as f32 + 0.5) {
                    continue;
                }
            }
            let touches = [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]
                .iter()
                .any(|(nx, ny)| working.get(*nx, *ny));
            if touches && may_enter(pixel(x, y)) {
                next.set(x, y);
                grew = true;
            }
        }
    }
    (next.finish().unwrap_or_else(|| working.clone()), grew)
}

fn grow_steps(
    region: SelectionMask,
    selection: Option<&Selection>,
    may_enter: impl Fn([u8; 4]) -> bool,
    pixel: &impl Fn(i32, i32) -> [u8; 4],
    steps: u32,
) -> SelectionMask {
    let mut working = region;
    for _ in 0..steps {
        let (next, grew) = grow_one_step(&working, selection, &may_enter, pixel);
        if !grew {
            break;
        }
        working = next;
    }
    working
}

fn grow_fill_region(
    pixel: impl Fn(i32, i32) -> [u8; 4],
    region: SelectionMask,
    selection: Option<&Selection>,
) -> SelectionMask {
    grow_steps(
        region,
        selection,
        |px| px[3] > 0 && px[3] < FILL_BARRIER_ALPHA,
        &pixel,
        FILL_FRINGE_MAX_STEPS,
    )
}

const DILATE_N8: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

fn dilate_8(mask: &SelectionMask) -> SelectionMask {
    let b = mask.bounds();
    let origin = (b.min_x, b.min_y);
    let width = (b.max_x - b.min_x + 1) as u32;
    let height = (b.max_y - b.min_y + 1) as u32;
    let mut next = SelectionMask::new(origin, width, height);
    for y in b.min_y..=b.max_y {
        for x in b.min_x..=b.max_x {
            if mask.get(x, y) || DILATE_N8.iter().any(|(dx, dy)| mask.get(x + dx, y + dy)) {
                next.set(x, y);
            }
        }
    }
    next
}

fn ink_walls(
    scope: DocRect,
    target: [u8; 4],
    tolerance: u8,
    pixel: &impl Fn(i32, i32) -> [u8; 4],
) -> SelectionMask {
    let limit = tol2(tolerance);
    let origin = (scope.min_x, scope.min_y);
    let width = (scope.max_x - scope.min_x + 1) as u32;
    let height = (scope.max_y - scope.min_y + 1) as u32;
    let mut walls = SelectionMask::new(origin, width, height);
    for y in scope.min_y..=scope.max_y {
        for x in scope.min_x..=scope.max_x {
            if color_distance(pixel(x, y), target) > limit {
                walls.set(x, y);
            }
        }
    }
    walls
}

fn close_gaps(walls: SelectionMask) -> SelectionMask {
    let mut closed = walls;
    for _ in 0..FILL_GAP_CLOSE_PX {
        closed = dilate_8(&closed);
    }
    closed
}

fn flood_unblocked(
    scope: DocRect,
    start_x: i32,
    start_y: i32,
    blocked: &SelectionMask,
    selection: Option<&Selection>,
) -> Option<SelectionMask> {
    if !scope.contains(start_x, start_y) || scope.is_empty() {
        return None;
    }
    let origin = (scope.min_x, scope.min_y);
    let width = (scope.max_x - scope.min_x + 1) as u32;
    let height = (scope.max_y - scope.min_y + 1) as u32;
    let mut visited = SelectionMask::new(origin, width, height);
    let mut reached = SelectionMask::new(origin, width, height);
    let mut queue = VecDeque::new();
    queue.push_back((start_x, start_y));
    visited.set(start_x, start_y);

    while let Some((x, y)) = queue.pop_front() {
        if (x != start_x || y != start_y) && blocked.get(x, y) {
            continue;
        }
        if let Some(sel) = selection {
            if !sel.contains(x as f32 + 0.5, y as f32 + 0.5) {
                continue;
            }
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
    flood(
        scope,
        start_x,
        start_y,
        tolerance,
        |x, y| tiles.get_pixel(x, y),
        |x, y| selection.map_or(true, |sel| sel.contains(x as f32 + 0.5, y as f32 + 0.5)),
    )
}

fn flood(
    scope: DocRect,
    start_x: i32,
    start_y: i32,
    tolerance: u8,
    mut pixel: impl FnMut(i32, i32) -> [u8; 4],
    may_visit: impl Fn(i32, i32) -> bool,
) -> Option<SelectionMask> {
    if !scope.contains(start_x, start_y) || scope.is_empty() {
        return None;
    }
    let target = pixel(start_x, start_y);
    let limit = tol2(tolerance);
    let origin = (scope.min_x, scope.min_y);
    let width = (scope.max_x - scope.min_x + 1) as u32;
    let height = (scope.max_y - scope.min_y + 1) as u32;

    let mut visited = SelectionMask::new(origin, width, height);
    let mut reached = SelectionMask::new(origin, width, height);
    let mut queue = VecDeque::new();
    queue.push_back((start_x, start_y));
    visited.set(start_x, start_y);

    while let Some((x, y)) = queue.pop_front() {
        if color_distance(pixel(x, y), target) > limit {
            continue;
        }
        reached.set(x, y);
        for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
            if !scope.contains(nx, ny) || visited.get(nx, ny) || !may_visit(nx, ny) {
                continue;
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
    let limit = tol2(tolerance);
    let width = (scope.max_x - scope.min_x + 1) as u32;
    let height = (scope.max_y - scope.min_y + 1) as u32;
    SelectionMask::from_predicate((scope.min_x, scope.min_y), width, height, |x, y| {
        color_distance(pixel(x, y), target) <= limit
    })
    .finish()
}

fn paint_region(tiles: &mut TileGrid, region: &SelectionMask, color: [u8; 4]) -> usize {
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

/// Traverse, then paint what was reached. Returns the pixel count so the caller can tell a
/// real edit from a click that landed on the fill color already.
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
    let Some(region) = flood_region(tiles, bounds, start_x, start_y, selection, tolerance)
        .and_then(|m| m.finish())
    else {
        return 0;
    };
    let region = grow_fill_region(|x, y| tiles.get_pixel(x, y), region, selection);
    paint_region(tiles, &region, color)
}

/// Same as [`flood_fill`], but the walk reads `pixel` rather than the destination tiles — the
/// bucket uses the visible composite so a circle on another layer is still an edge. Ink is
/// dilated by [`FILL_GAP_CLOSE_PX`] first so a small gap in a hand-drawn loop still contains
/// the fill, then the interior halo is grown back so the paint still meets the stroke.
#[allow(clippy::too_many_arguments)]
pub fn flood_fill_sampled(
    tiles: &mut TileGrid,
    bounds: DocRect,
    start_x: i32,
    start_y: i32,
    color: [u8; 4],
    selection: Option<&Selection>,
    tolerance: u8,
    pixel: impl Fn(i32, i32) -> [u8; 4],
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
    let target = pixel(start_x, start_y);
    let blocked = close_gaps(ink_walls(bounds, target, tolerance, &pixel));
    let Some(region) = flood_unblocked(bounds, start_x, start_y, &blocked, selection) else {
        return 0;
    };
    let limit = tol2(tolerance);
    let region = grow_steps(
        region,
        selection,
        |px| color_distance(px, target) <= limit,
        &pixel,
        FILL_GAP_CLOSE_PX,
    );
    let region = grow_fill_region(&pixel, region, selection);
    paint_region(tiles, &region, color)
}

pub(crate) fn pixel_in_rgba(rgba: &[u8], width: u32, height: u32, x: i32, y: i32) -> [u8; 4] {
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
