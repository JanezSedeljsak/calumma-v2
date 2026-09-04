//! The healing brush's kernel: a frequency split rather than a copy. A blemish disappears
//! instead of being covered by a visible patch because only the source's *texture* travels —
//! its low frequencies (colour, lighting) are swapped for the destination's before the result
//! lands:
//!
//! ```text
//! out = src - blur(src) + blur(dst)
//! ```
//!
//! This is an approximation of the Poisson gradient-domain solve a real healing brush uses, and
//! the one this ships with: it is most of the value for a fraction of the work, and the
//! alternative is a Poisson solve over an arbitrary patch on every pointer event. It is honestly
//! worse than Poisson at strong edges, where the low-frequency mismatch shows as a halo — a
//! real limitation, disclosed in `docs/FLOW.md` rather than left to look like a bug.
//!
//! The blur half is `blur::box_blur_premultiplied`, unchanged: the expensive part of healing is
//! already written, tuned and shipped for the blur brush, so this kernel spends its own code
//! only on the split.

use crate::blur::{
    box_blur_premultiplied, disc_coverage, pass_radius_for, premultiply, snapshot_margin,
    to_premultiplied, unpremultiply,
};
use crate::limits::STAMP_COVERAGE_PADDING;
use crate::selection::Selection;
use crate::tile::{DocRect, TileGrid};

/// One pointer event's worth of healing: every stamp in `stamps` healed from `offset` away, in a
/// single pass over the region they cover. See `blur::blur_stamps` for the batching and
/// premultiplied-alpha rationale, and `clone::clone_stamps` for the offset — everything here is
/// the same scaffold with the copy replaced by the split above.
///
/// Returns the number of tiles touched, so the caller can tell a real edit from a no-op.
pub fn heal_stamps(
    grid: &mut TileGrid,
    stamps: &[(f32, f32)],
    radius: f32,
    offset: (i32, i32),
    selection: Option<&Selection>,
) -> usize {
    if radius <= 0.0 || stamps.is_empty() {
        return 0;
    }
    let pad = radius + STAMP_COVERAGE_PADDING;
    let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
    let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    for &(x, y) in stamps {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    let target = DocRect::from_floats(min_x - pad, min_y - pad, max_x + pad, max_y + pad);
    let Some(target) = target.intersect(grid.bounds()) else {
        return 0;
    };

    let pass_radius = pass_radius_for(radius);
    let margin = snapshot_margin(pass_radius);
    let dst_rect = DocRect::new(
        target.min_x - margin,
        target.min_y - margin,
        target.max_x + margin,
        target.max_y + margin,
    );
    let src_rect = DocRect::new(
        dst_rect.min_x + offset.0,
        dst_rect.min_y + offset.1,
        dst_rect.max_x + offset.0,
        dst_rect.max_y + offset.1,
    );
    let width = (dst_rect.max_x - dst_rect.min_x + 1) as usize;
    let height = (dst_rect.max_y - dst_rect.min_y + 1) as usize;

    let dst_buf = to_premultiplied(&grid.copy_rect_rgba(dst_rect), width * height);
    let src_buf = to_premultiplied(&grid.copy_rect_rgba(src_rect), width * height);
    let blurred_dst = box_blur_premultiplied(&dst_buf, width, height, pass_radius);
    let blurred_src = box_blur_premultiplied(&src_buf, width, height, pass_radius);

    let r_soft = radius + 0.5;
    grid.paint_rect(target, |px, py, dst| {
        let cx = px as f32 + 0.5;
        let cy = py as f32 + 0.5;
        let coverage = disc_coverage(stamps, cx, cy, r_soft);
        if coverage <= 0.0 {
            return None;
        }
        if let Some(sel) = selection {
            if !sel.contains(cx, cy) {
                return None;
            }
        }
        let i = ((py - dst_rect.min_y) as usize) * width + (px - dst_rect.min_x) as usize;
        let (src, bs, bd) = (src_buf[i], blurred_src[i], blurred_dst[i]);
        let healed = [
            (src[0] - bs[0] + bd[0]).clamp(0.0, 1.0),
            (src[1] - bs[1] + bd[1]).clamp(0.0, 1.0),
            (src[2] - bs[2] + bd[2]).clamp(0.0, 1.0),
            (src[3] - bs[3] + bd[3]).clamp(0.0, 1.0),
        ];
        let base = premultiply(dst);
        let mixed = [
            base[0] + (healed[0] - base[0]) * coverage,
            base[1] + (healed[1] - base[1]) * coverage,
            base[2] + (healed[2] - base[2]) * coverage,
            base[3] + (healed[3] - base[3]) * coverage,
        ];
        Some(unpremultiply(mixed))
    })
}
