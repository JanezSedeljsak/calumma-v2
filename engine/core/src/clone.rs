//! The clone stamp's kernel — a straight copy from a source rect to a destination rect, offset
//! by a fixed vector, weighted by the brush's own coverage.
//!
//! Reuses `blur.rs`'s two hard problems and their answers rather than re-solving them: the
//! destination is snapshotted before any of it is written (`TileGrid::copy_rect_rgba`), so a
//! stamp cannot read pixels the same batch already overwrote, and the blend runs in
//! premultiplied space so a copy across a transparency boundary does not drag the edge toward
//! garbage. The offset is snapped to whole document pixels when it is set
//! (`Document::clone_pending_stamps`), the same nearest-neighbour trade-off the layer-transform
//! flatten already makes, so this kernel never has to resample.

use crate::blur::{disc_coverage, premultiply, unpremultiply};
use crate::limits::STAMP_COVERAGE_PADDING;
use crate::selection::Selection;
use crate::tile::{DocRect, TileGrid};

/// One pointer event's worth of cloning: every stamp in `stamps` copied from `offset` away into
/// `grid` in a single pass over the region they cover. See `blur::blur_stamps` for why a batch
/// rather than one stamp at a time, and why coverage rather than a plain overwrite — both
/// reasons apply here unchanged.
///
/// Returns the number of tiles touched, so the caller can tell a real edit from a no-op.
pub fn clone_stamps(
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
    let source = DocRect::new(
        target.min_x + offset.0,
        target.min_y + offset.1,
        target.max_x + offset.0,
        target.max_y + offset.1,
    );
    let width = (target.max_x - target.min_x + 1) as usize;
    // Pixels of `source` outside the document read as transparent, the same as pasting an
    // image that overflows the canvas — there is nothing there to clone, not an error.
    let src_buf = grid.copy_rect_rgba(source);

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
        let i = (((py - target.min_y) as usize) * width + (px - target.min_x) as usize) * 4;
        let src = premultiply([src_buf[i], src_buf[i + 1], src_buf[i + 2], src_buf[i + 3]]);
        let base = premultiply(dst);
        let mixed = [
            base[0] + (src[0] - base[0]) * coverage,
            base[1] + (src[1] - base[1]) * coverage,
            base[2] + (src[2] - base[2]) * coverage,
            base[3] + (src[3] - base[3]) * coverage,
        ];
        Some(unpremultiply(mixed))
    })
}
