//! Where a size slider's thumb sits, and where one press of a size key lands.
//!
//! Both are product decisions, so both are here rather than in the shell: the panel hands the
//! engine a 0..1 unit exactly the way the zoom pill hands it `zoom_unit`, and gets a size
//! back. Nothing in Swift knows the curve, the exponent, or the range — the same reason
//! `Camera::zoom_from_unit` owns the log zoom curve.

use crate::limits::{BRUSH_SIZE_MAX, BRUSH_SIZE_MIN, BRUSH_SIZE_STEP_RATIO, SIZE_CURVE_EXPONENT};
use calumma_text::{TEXT_SIZE_MAX, TEXT_SIZE_MIN};

/// Slider travel → size. `unit` outside 0..1 is clamped rather than extrapolated: a slider
/// cannot be dragged past its own ends, and a caller that computed one out of range is asking
/// for a size out of range.
pub fn size_from_unit(unit: f32, min: f32, max: f32) -> f32 {
    if max <= min {
        return min;
    }
    min + (max - min) * unit.clamp(0.0, 1.0).powf(SIZE_CURVE_EXPONENT)
}

/// Size → slider travel, the exact inverse of `size_from_unit`. The round trip has to hold or
/// the thumb jumps away from the value the field just committed.
pub fn unit_from_size(size: f32, min: f32, max: f32) -> f32 {
    if max <= min {
        return 0.0;
    }
    (((size.clamp(min, max) - min) / (max - min)).powf(1.0 / SIZE_CURVE_EXPONENT)).clamp(0.0, 1.0)
}

/// Rounded, because the panel prints the size as a whole number and the field types one
/// back: a slider that quietly held 137.42 behind a label reading 137 would make the field
/// and the thumb disagree about the same brush.
pub fn brush_size_from_unit(unit: f32) -> f32 {
    size_from_unit(unit, BRUSH_SIZE_MIN, BRUSH_SIZE_MAX).round()
}

pub fn brush_size_unit(size: f32) -> f32 {
    unit_from_size(size, BRUSH_SIZE_MIN, BRUSH_SIZE_MAX)
}

/// Rounded like `brush_size_from_unit`. Only the *slider* is whole-numbered — a run that
/// already carries a fractional size keeps it, and so does one typed into the field.
pub fn text_size_from_unit(unit: f32) -> f32 {
    size_from_unit(unit, TEXT_SIZE_MIN, TEXT_SIZE_MAX).round()
}

pub fn text_size_unit(size: f32) -> f32 {
    unit_from_size(size, TEXT_SIZE_MIN, TEXT_SIZE_MAX)
}

/// One press of `[` or `]`. Proportional so the whole range is reachable, floored at a pixel
/// so a fine brush still moves by one, and rounded so the size the panel prints is the size
/// the next press starts from — stepping through fractional sizes would make `]` then `[`
/// land somewhere other than where it began.
pub fn step_brush_size(size: f32, increase: bool) -> f32 {
    let size = size.clamp(BRUSH_SIZE_MIN, BRUSH_SIZE_MAX);
    let next = if increase {
        (size * (1.0 + BRUSH_SIZE_STEP_RATIO)).max(size + 1.0)
    } else {
        (size / (1.0 + BRUSH_SIZE_STEP_RATIO)).min(size - 1.0)
    };
    next.round().clamp(BRUSH_SIZE_MIN, BRUSH_SIZE_MAX)
}
