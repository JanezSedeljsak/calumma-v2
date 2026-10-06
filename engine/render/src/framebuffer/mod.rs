mod pan_cache;

pub(crate) use pan_cache::PanCache;

pub type PxRect = (u32, u32, u32, u32);

/// Where a camera-only pan can read valid pixels from and where they land, given the on-screen
/// paper rect at the last full redraw (`reference`) and now (`current`), and the device-pixel
/// shift between them. `None` means the overlap is empty (a jump too large, or a corner case at
/// a viewport edge) — the caller falls back to a full redraw rather than blitting garbage.
pub fn shift_plan(
    reference: PxRect,
    current: PxRect,
    dx: i32,
    dy: i32,
    tex_w: u32,
    tex_h: u32,
) -> Option<(PxRect, PxRect)> {
    let (cx, cy, cw, ch) = current;
    let (rx, ry, rw, rh) = reference;

    let src_x0 = cx as i64 - dx as i64;
    let src_y0 = cy as i64 - dy as i64;
    let src_x1 = src_x0 + cw as i64;
    let src_y1 = src_y0 + ch as i64;

    let lo_x = src_x0.max(rx as i64).max(0);
    let lo_y = src_y0.max(ry as i64).max(0);
    let hi_x = src_x1.min(rx as i64 + rw as i64).min(tex_w as i64);
    let hi_y = src_y1.min(ry as i64 + rh as i64).min(tex_h as i64);
    if hi_x <= lo_x || hi_y <= lo_y {
        return None;
    }
    let src: PxRect = (
        lo_x as u32,
        lo_y as u32,
        (hi_x - lo_x) as u32,
        (hi_y - lo_y) as u32,
    );

    let dst_x0 = lo_x + dx as i64;
    let dst_y0 = lo_y + dy as i64;
    if dst_x0 < 0
        || dst_y0 < 0
        || dst_x0 + src.2 as i64 > tex_w as i64
        || dst_y0 + src.3 as i64 > tex_h as i64
    {
        return None;
    }
    let dst: PxRect = (dst_x0 as u32, dst_y0 as u32, src.2, src.3);
    Some((src, dst))
}

/// `outer \ inner` as up to four non-overlapping bands (top, bottom, left, right), assuming
/// `inner` is fully contained in `outer` — which `shift_plan`'s `dst` always is, by
/// construction. This is what has to be redrawn after a shift: everything the copy could not
/// have populated, because it was never part of the previous frame's paper rect.
pub fn exposed_rects(outer: PxRect, inner: PxRect) -> [Option<PxRect>; 4] {
    let (ox, oy, ow, oh) = outer;
    let (ix, iy, iw, ih) = inner;

    let top = (iy > oy).then_some((ox, oy, ow, iy - oy));
    let bottom_y = iy + ih;
    let bottom = (bottom_y < oy + oh).then_some((ox, bottom_y, ow, (oy + oh) - bottom_y));
    let left = (ix > ox).then_some((ox, iy, ix - ox, ih));
    let right_x = ix + iw;
    let right = (right_x < ox + ow).then_some((right_x, iy, (ox + ow) - right_x, ih));

    [top, bottom, left, right]
}

/// One camera-only frame's shift, in whole device pixels, plus where it reads and lands. The
/// shift travels with the rects because the caller has to commit it back into the reference
/// afterwards — see [`PanCache::commit_shift`].
#[derive(Clone, Copy)]
pub(crate) struct BlitPlan {
    pub(crate) src: PxRect,
    pub(crate) dst: PxRect,
    pub(crate) shift: (i32, i32),
}
