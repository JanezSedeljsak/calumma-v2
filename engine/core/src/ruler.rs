use crate::limits::{RULER_MIN_MAJOR_SPACING_PX, RULER_MIN_MINOR_SPACING_PX};

/// One tick along a ruler axis, in document pixels — the same unit `Camera::to_doc` /
/// `to_screen` speak, so `doc * zoom + pan` maps a tick back to a screen coordinate.
/// Major ticks get a label; minor ticks are unlabeled marks between them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RulerTick {
    pub doc: f32,
    pub major: bool,
}

/// Rounds `min_step` up to the next `1 / 2 / 5 × 10ⁿ`, the spacing Photoshop/Figma rulers
/// use so labels land on round numbers at any zoom instead of an arbitrary fraction.
fn nice_step(min_step: f32) -> f32 {
    if !min_step.is_finite() || min_step <= 0.0 {
        return 1.0;
    }
    let exp = min_step.log10().floor();
    let base = 10f32.powf(exp);
    for mult in [1.0, 2.0, 5.0] {
        let step = mult * base;
        if step >= min_step {
            return step;
        }
    }
    10.0 * base
}

/// Major/minor tick positions, in document pixels, visible across one axis of the
/// viewport. `zoom`/`pan`/`viewport_extent` are the matching `Camera` fields for that axis
/// (`pan_x`/`viewport_width` or `pan_y`/`viewport_height`). Ticks are not clamped to the
/// paper bounds — like Photoshop, the ruler keeps counting into the margin around it.
pub fn ruler_ticks(zoom: f32, pan: f32, viewport_extent: f32) -> Vec<RulerTick> {
    if !zoom.is_finite() || zoom <= 0.0 || !viewport_extent.is_finite() || viewport_extent <= 0.0 {
        return Vec::new();
    }

    let minor_step = nice_step(RULER_MIN_MINOR_SPACING_PX / zoom);
    let major_ratio = (nice_step(RULER_MIN_MAJOR_SPACING_PX / zoom) / minor_step)
        .round()
        .max(1.0);
    let major_step = minor_step * major_ratio;

    let doc_start = -pan / zoom;
    let doc_end = (viewport_extent - pan) / zoom;
    let first = (doc_start / minor_step).floor() as i64;
    let last = (doc_end / minor_step).ceil() as i64;

    (first..=last)
        .map(|i| {
            let doc = i as f32 * minor_step;
            let nearest_major = (doc / major_step).round() * major_step;
            RulerTick {
                doc,
                major: (doc - nearest_major).abs() < minor_step * 0.01,
            }
        })
        .collect()
}
