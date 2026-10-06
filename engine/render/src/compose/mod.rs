mod crop;
mod cursor;
mod frames;
mod guides;
mod selection;
mod text;
mod tile_upload;

pub use crop::crop_overlay_instances;
pub use cursor::{brush_ring_instances, clone_source_overlay_instances};
pub use frames::{
    box_overlay_instances, layer_highlight_instances, subject_sweep_instances,
    transform_overlay_instances,
};
pub use guides::{guide_instances, GuideInstance};
pub use selection::{selection_lasso_points, selection_mask_edges, selection_rect_or_ellipse};
pub use text::{text_caret_visible, text_overlay_instances};
pub use tile_upload::{composited_tile_payload, tile_mip_chain, tile_upload_mips};

use bytemuck::{Pod, Zeroable};
use calumma_core::{BrushProfile, StrokePoint};

const OVERLAY_BORDER_COLOR: [f32; 4] = [0.35, 0.38, 0.42, 0.9];
const OVERLAY_BORDER_PX: f32 = 1.0;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct StrokeInstance {
    pub segment: [f32; 4],
    pub color: [f32; 4],
    pub brush: [f32; 4],
}

/// `(radius, hardness, grain, grain_scale)` as the shader wants it. The brush table lives in
/// the engine and rides to the GPU as instance data, so `board.wgsl` never keeps a second copy
/// of it that could drift.
pub fn brush_params(radius: f32, profile: &BrushProfile) -> [f32; 4] {
    [radius, profile.hardness, profile.grain, profile.grain_scale]
}

/// Overlay instance parameters for a filled rectangle rather than a capsule.
///
/// `fs_overlay` reads a non-zero half *height* as "this is a box", which is why every existing
/// overlay instance — all of them `BrushProfile::HARD`, whose grain is 0 — keeps drawing a
/// capsule with no change. The height is in screen pixels like the width, so a selection row
/// is handed `row_height * zoom * 0.5` and tracks the glyphs it covers instead of the chrome.
pub fn overlay_rect_params(half_height_px: f32) -> [f32; 4] {
    [0.0, 0.0, half_height_px, 0.0]
}

pub fn rgba_unit(rgba: [u8; 4]) -> [f32; 4] {
    [
        rgba[0] as f32 / 255.0,
        rgba[1] as f32 / 255.0,
        rgba[2] as f32 / 255.0,
        rgba[3] as f32 / 255.0,
    ]
}

fn push_outlined_segment(
    out: &mut Vec<StrokeInstance>,
    a: (f32, f32),
    b: (f32, f32),
    color: [f32; 4],
    width: f32,
) {
    let segment = [a.0, a.1, b.0, b.1];
    out.push(StrokeInstance {
        segment,
        color: OVERLAY_BORDER_COLOR,
        brush: brush_params(width + OVERLAY_BORDER_PX, &BrushProfile::HARD),
    });
    out.push(StrokeInstance {
        segment,
        color,
        brush: brush_params(width, &BrushProfile::HARD),
    });
}

pub fn stroke_instances(
    points: &[StrokePoint],
    radius: f32,
    color: [f32; 4],
    profile: &BrushProfile,
) -> Vec<StrokeInstance> {
    stroke_instances_from(points, 0, radius, color, profile)
}

/// How many instances [`stroke_instances`] emits for this many points: one capsule per pair,
/// or a single degenerate one for a lone point so a tap still leaves a dot.
pub fn stroke_segment_count(points: usize) -> usize {
    match points {
        0 => 0,
        1 => 1,
        n => n - 1,
    }
}

/// The tail of [`stroke_instances`] from `first_segment` on, so a live stroke can hand the GPU
/// only the segments the pointer has travelled since the last frame instead of the whole line
/// again. Segment `i` is the capsule between points `i` and `i + 1`, which makes the numbering
/// append-only for any stroke past its first point — the one-point case emits a degenerate
/// capsule that segment 0 later replaces, so callers restart rather than append across that
/// boundary.
pub fn stroke_instances_from(
    points: &[StrokePoint],
    first_segment: usize,
    radius: f32,
    color: [f32; 4],
    profile: &BrushProfile,
) -> Vec<StrokeInstance> {
    if points.is_empty() || first_segment >= stroke_segment_count(points.len()) {
        return Vec::new();
    }
    let instance = |a: &StrokePoint, b: &StrokePoint| StrokeInstance {
        segment: [a.x, a.y, b.x, b.y],
        color,
        brush: brush_params(radius, profile),
    };
    if points.len() == 1 {
        return vec![instance(&points[0], &points[0])];
    }
    points
        .windows(2)
        .skip(first_segment)
        .map(|p| instance(&p[0], &p[1]))
        .collect()
}
