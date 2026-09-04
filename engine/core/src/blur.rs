//! The blur brush's kernel — the first tool that *reads* the destination and writes a
//! function of it, rather than writing a color over it.
//!
//! Two things make this different from every other stamp:
//!
//! 1. **The read-write overlap.** `paint_rect`'s closure sees the pixel it is about to
//!    replace, but a blur needs the pixel's *neighbourhood*. Sampling pixels the same stamp
//!    has already written smears in the direction of iteration instead of blurring, so the
//!    source region is snapshotted out of the grid first and the kernel reads only the
//!    snapshot.
//! 2. **Alpha.** Tiles hold straight (unpremultiplied) alpha — `blend_over` weights each
//!    channel by alpha, which only makes sense if the stored channels are not already
//!    weighted. Averaging straight color would pull the edge of a painted region toward
//!    whatever garbage sits in the fully transparent pixels around it, so the kernel works in
//!    premultiplied space and converts back on the way out.

use crate::limits::{BLUR_BOX_PASSES, BLUR_RADIUS_RATIO, STAMP_COVERAGE_PADDING};
use crate::selection::Selection;
use crate::tile::{DocRect, TileGrid};

/// The kernel radius a brush of this radius buys, in document pixels.
///
/// Deliberately a fraction of the brush rather than the brush itself: the brush radius is how
/// *wide* a swathe one pass softens, the kernel radius is how *far* each pixel smears, and a
/// brush whose smear reached its own edge would blur the stroke's boundary into the untouched
/// pixels beside it no matter how low the strength.
pub fn blur_radius(brush_radius: f32) -> f32 {
    (brush_radius * BLUR_RADIUS_RATIO).max(1.0)
}

/// One pointer event's worth of blur: every stamp in `stamps` softened into `grid` in a single
/// pass over the region they cover.
///
/// Taking the whole batch rather than one stamp at a time is what keeps this affordable.
/// Stamps along a stroke overlap by half their radius, so a per-stamp snapshot would read and
/// blur most pixels several times over; one union region is snapshotted, blurred once, and
/// written back weighted by how much of the brush passed over each pixel. Overlap inside a
/// single event therefore does not double-blur — accumulation comes from dragging back over
/// the same pixels on a *later* event, which reads the already-blurred result.
///
/// Returns the number of tiles touched, so the caller can tell a real edit from a no-op.
pub fn blur_stamps(
    grid: &mut TileGrid,
    stamps: &[(f32, f32)],
    radius: f32,
    strength: f32,
    selection: Option<&Selection>,
) -> usize {
    if radius <= 0.0 || strength <= 0.0 || stamps.is_empty() {
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
    let source = DocRect::new(
        target.min_x - margin,
        target.min_y - margin,
        target.max_x + margin,
        target.max_y + margin,
    );
    let width = (source.max_x - source.min_x + 1) as usize;
    let height = (source.max_y - source.min_y + 1) as usize;

    let buf = to_premultiplied(&grid.copy_rect_rgba(source), width * height);
    let buf = box_blur_premultiplied(&buf, width, height, pass_radius);

    let r2_soft = radius + 0.5;
    grid.paint_rect(target, |px, py, dst| {
        let cx = px as f32 + 0.5;
        let cy = py as f32 + 0.5;
        let coverage = disc_coverage(stamps, cx, cy, r2_soft);
        if coverage <= 0.0 {
            return None;
        }
        if let Some(sel) = selection {
            if !sel.contains(cx, cy) {
                return None;
            }
        }
        let i = ((py - source.min_y) as usize) * width + (px - source.min_x) as usize;
        let blurred = buf[i];
        let base = premultiply(dst);
        let w = coverage * strength;
        let mixed = [
            base[0] + (blurred[0] - base[0]) * w,
            base[1] + (blurred[1] - base[1]) * w,
            base[2] + (blurred[2] - base[2]) * w,
            base[3] + (blurred[3] - base[3]) * w,
        ];
        Some(unpremultiply(mixed))
    })
}

/// The box-pass radius one event's worth of `blur_radius(brush_radius)` spreads over, split
/// evenly across `BLUR_BOX_PASSES` passes. Shared with `clone.rs` and `heal.rs`, whose kernels
/// read a neighbourhood the same way and pay the same snapshot margin for it.
pub(crate) fn pass_radius_for(brush_radius: f32) -> i32 {
    ((blur_radius(brush_radius) / BLUR_BOX_PASSES as f32).round() as i32).max(1)
}

/// Three sliding box passes over a premultiplied buffer, stacked as a Gaussian approximation —
/// see the module doc for why premultiplied and why three passes. Pulled out of `blur_stamps`
/// so the healing brush's frequency split (`heal::heal_stamps`) can blur both its source and
/// destination snapshots with the exact same kernel blur already pays for.
pub(crate) fn box_blur_premultiplied(
    buf: &[[f32; 4]],
    width: usize,
    height: usize,
    pass_radius: i32,
) -> Vec<[f32; 4]> {
    let pass_radius = pass_radius.max(1) as usize;
    let mut buf = buf.to_vec();
    let mut scratch = vec![[0f32; 4]; width * height];
    for _ in 0..BLUR_BOX_PASSES {
        box_pass_horizontal(&buf, &mut scratch, width, height, pass_radius);
        box_pass_vertical(&scratch, &mut buf, width, height, pass_radius);
    }
    buf
}

/// How much of the brush passed over this pixel, antialiased over the outermost pixel of the
/// disc so the stamp has a soft edge instead of a stair-stepped one. Overlapping stamps take
/// the maximum, so one pointer event's worth of brush cannot blur the same pixel twice.
pub(crate) fn disc_coverage(stamps: &[(f32, f32)], cx: f32, cy: f32, outer: f32) -> f32 {
    let mut coverage = 0f32;
    for &(sx, sy) in stamps {
        let dx = cx - sx;
        let dy = cy - sy;
        let d = (dx * dx + dy * dy).sqrt();
        coverage = coverage.max((outer - d).clamp(0.0, 1.0));
        if coverage >= 1.0 {
            break;
        }
    }
    coverage
}

/// How far beyond the pixels being written the snapshot has to reach. Every box pass spreads
/// by its own radius, so a snapshot cut any tighter would blur the edge of the target rect
/// against whatever it happened to cut off — a visible seam along the stamp's bounding box,
/// which is the artefact the soft disc edge exists to avoid.
pub(crate) fn snapshot_margin(pass_radius: i32) -> i32 {
    pass_radius * BLUR_BOX_PASSES as i32
}

pub(crate) fn premultiply(px: [u8; 4]) -> [f32; 4] {
    let a = px[3] as f32 / 255.0;
    [
        px[0] as f32 / 255.0 * a,
        px[1] as f32 / 255.0 * a,
        px[2] as f32 / 255.0 * a,
        a,
    ]
}

pub(crate) fn unpremultiply(px: [f32; 4]) -> [u8; 4] {
    let a = px[3].clamp(0.0, 1.0);
    if a <= 0.0 {
        return [0, 0, 0, 0];
    }
    let to_byte = |v: f32| ((v / a).clamp(0.0, 1.0) * 255.0).round() as u8;
    [
        to_byte(px[0]),
        to_byte(px[1]),
        to_byte(px[2]),
        (a * 255.0).round() as u8,
    ]
}

pub(crate) fn to_premultiplied(rgba: &[u8], pixels: usize) -> Vec<[f32; 4]> {
    (0..pixels)
        .map(|i| {
            let o = i * 4;
            premultiply([rgba[o], rgba[o + 1], rgba[o + 2], rgba[o + 3]])
        })
        .collect()
}

/// A box blur is only worth running as a Gaussian approximation because the window slides:
/// each output pixel costs one add and one subtract regardless of radius, so widening the
/// kernel is free and the pass count — not the radius — is what the cost scales with. Edges
/// clamp to the outermost pixel, which is why the caller snapshots a margin: clamping inside
/// the *document* is correct, clamping inside an arbitrary crop of it is a seam. The window
/// starts half off the left edge, so `clamped_left_overhang` counts the pixels it hangs over
/// as copies of pixel 0 — the clamp folded into the running sum rather than branched on.
fn box_pass_horizontal(
    src: &[[f32; 4]],
    dst: &mut [[f32; 4]],
    width: usize,
    height: usize,
    radius: usize,
) {
    if width == 0 {
        return;
    }
    let window = (radius * 2 + 1) as f32;
    for y in 0..height {
        let row = y * width;
        let mut acc = [0f32; 4];
        for x in 0..=radius.min(width - 1) {
            for c in 0..4 {
                acc[c] += src[row + x][c];
            }
        }
        let clamped_left_overhang = radius as f32;
        for c in 0..4 {
            acc[c] += src[row][c] * clamped_left_overhang;
        }
        for x in 0..width {
            for c in 0..4 {
                dst[row + x][c] = acc[c] / window;
            }
            let leaving = src[row + x.saturating_sub(radius)];
            let entering = src[row + (x + radius + 1).min(width - 1)];
            for c in 0..4 {
                acc[c] += entering[c] - leaving[c];
            }
        }
    }
}

fn box_pass_vertical(
    src: &[[f32; 4]],
    dst: &mut [[f32; 4]],
    width: usize,
    height: usize,
    radius: usize,
) {
    if height == 0 {
        return;
    }
    let window = (radius * 2 + 1) as f32;
    for x in 0..width {
        let mut acc = [0f32; 4];
        for y in 0..=radius.min(height - 1) {
            for c in 0..4 {
                acc[c] += src[y * width + x][c];
            }
        }
        for c in 0..4 {
            acc[c] += src[x][c] * radius as f32;
        }
        for y in 0..height {
            for c in 0..4 {
                dst[y * width + x][c] = acc[c] / window;
            }
            let leaving = src[y.saturating_sub(radius) * width + x];
            let entering = src[(y + radius + 1).min(height - 1) * width + x];
            for c in 0..4 {
                acc[c] += entering[c] - leaving[c];
            }
        }
    }
}
