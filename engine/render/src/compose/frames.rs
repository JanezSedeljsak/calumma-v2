use super::{
    brush_params, overlay_rect_params, push_outlined_segment, StrokeInstance, OVERLAY_BORDER_COLOR,
    OVERLAY_BORDER_PX,
};
use calumma_core::{BrushProfile, TransformHandles};

/// Every width and radius below is a **screen**-pixel half-width, read by `board.wgsl`'s
/// `vs_overlay` / `fs_overlay` rather than the stroke pass — board furniture is the same size
/// at every zoom or it is not furniture. An 8px grip radius draws a 16px grip, which finally
/// agrees with the 10px `HANDLE_HIT_RADIUS_PX` it is grabbed by: a ring of slack around
/// something visible, instead of two unrelated numbers that only matched at one zoom.
const TRANSFORM_OUTLINE_COLOR: [f32; 4] = [0.24, 0.78, 0.84, 0.95];
const TRANSFORM_OUTLINE_WIDTH_PX: f32 = 1.0;
const TRANSFORM_HANDLE_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const TRANSFORM_HANDLE_RADIUS_PX: f32 = 8.0;
/// A white grip on white paper is not a grip. The ring is drawn as a slightly larger disc
/// *under* the white one rather than as an outline of its own — the overlay pass has no stroked
/// circle, and two discs is the same primitive twice instead of a new one. Grey rather than the
/// frame's teal, so the thing you grab stays distinct from the frame it sits on, and thin enough
/// that the grip still reads as white: the visual radius goes to 9px, which keeps it inside the
/// 10px `HANDLE_HIT_RADIUS_PX` the grip is caught by.
const TRANSFORM_HANDLE_BORDER_COLOR: [f32; 4] = OVERLAY_BORDER_COLOR;
const TRANSFORM_HANDLE_BORDER_PX: f32 = OVERLAY_BORDER_PX;

const LAYER_HIGHLIGHT_COLOR: [f32; 4] = [0.24, 0.78, 0.84, 0.85];
const LAYER_HIGHLIGHT_WIDTH_PX: f32 = 1.0;
const LAYER_HIGHLIGHT_DASH_PX: f32 = 8.0;
const LAYER_HIGHLIGHT_GAP_PX: f32 = 8.0;

/// The one layer outline — hover, a Move selection and a Move drag all draw exactly this —
/// dashed at a constant screen period. `vs_overlay` fixes the *width* on screen but not the
/// dash pattern — the dashes are cut here, in document space, one instance per dash — so the
/// period is divided by the zoom instead. It is still: the board only redraws when something
/// changed, so a dash that marched would jump rather than move.
pub fn layer_highlight_instances(corners: [(f32, f32); 4], zoom: f32) -> Vec<StrokeInstance> {
    let zoom = zoom.max(f32::MIN_POSITIVE);
    let phase = 0.0;
    let mut out = Vec::with_capacity(32);
    for i in 0..4 {
        out.extend(dashed_edge(
            corners[i],
            corners[(i + 1) % 4],
            phase,
            LAYER_HIGHLIGHT_COLOR,
            LAYER_HIGHLIGHT_WIDTH_PX,
            LAYER_HIGHLIGHT_DASH_PX / zoom,
            LAYER_HIGHLIGHT_GAP_PX / zoom,
        ));
    }
    out
}

fn dashed_edge(
    a: (f32, f32),
    b: (f32, f32),
    phase: f32,
    color: [f32; 4],
    width: f32,
    dash: f32,
    gap: f32,
) -> Vec<StrokeInstance> {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-6 {
        return Vec::new();
    }
    let ux = dx / len;
    let uy = dy / len;
    let period = dash + gap;
    let mut t = -phase.rem_euclid(period);
    let mut out = Vec::new();
    while t < len {
        let start = t.max(0.0);
        let end = (t + dash).min(len);
        if end > start {
            push_outlined_segment(
                &mut out,
                (a.0 + ux * start, a.1 + uy * start),
                (a.0 + ux * end, a.1 + uy * end),
                color,
                width,
            );
        }
        t += period;
    }
    out
}

pub fn transform_overlay_instances(handles: TransformHandles) -> Vec<StrokeInstance> {
    let (_, corners, rotate_handle) = handles;
    box_overlay_instances(corners, Some(rotate_handle))
}

/// A frame with a grip at each corner, and the rotate stalk when the thing inside can be
/// turned. A vector item cannot — the shader's SDFs are axis-aligned — so its frame is this
/// same furniture minus the stalk, rather than a second kind of box to learn. Only one frame
/// is ever on screen: selecting an item is what takes it off the layer.
pub fn box_overlay_instances(
    corners: [(f32, f32); 4],
    rotate_handle: Option<(f32, f32)>,
) -> Vec<StrokeInstance> {
    let mut out = Vec::with_capacity((4 + 1 + 5) * 2);
    for i in 0..4 {
        push_outlined_segment(
            &mut out,
            corners[i],
            corners[(i + 1) % 4],
            TRANSFORM_OUTLINE_COLOR,
            TRANSFORM_OUTLINE_WIDTH_PX,
        );
    }
    if let Some(rotate_handle) = rotate_handle {
        let top_mid = (
            (corners[0].0 + corners[1].0) * 0.5,
            (corners[0].1 + corners[1].1) * 0.5,
        );
        push_outlined_segment(
            &mut out,
            top_mid,
            rotate_handle,
            TRANSFORM_OUTLINE_COLOR,
            TRANSFORM_OUTLINE_WIDTH_PX,
        );
    }
    for p in corners.iter().chain(rotate_handle.iter()) {
        // Border first, grip over it: instances paint in order, so the larger disc underneath
        // shows only as the ring left around the smaller one.
        out.push(StrokeInstance {
            segment: [p.0, p.1, p.0, p.1],
            color: TRANSFORM_HANDLE_BORDER_COLOR,
            brush: brush_params(
                TRANSFORM_HANDLE_RADIUS_PX + TRANSFORM_HANDLE_BORDER_PX,
                &BrushProfile::HARD,
            ),
        });
        out.push(StrokeInstance {
            segment: [p.0, p.1, p.0, p.1],
            color: TRANSFORM_HANDLE_COLOR,
            brush: brush_params(TRANSFORM_HANDLE_RADIUS_PX, &BrushProfile::HARD),
        });
    }
    out
}

const SUBJECT_SWEEP_COLOR: [f32; 4] = [0.24, 0.78, 0.84, 0.35];
const SUBJECT_SWEEP_PERIOD: f32 = 1.35;

/// A band crossing the layer while its background is removed. The band is a filled overlay
/// box — the same primitive a text selection row uses — so it stays a constant fraction of
/// the layer on screen and moves with `elapsed`.
pub fn subject_sweep_instances(
    corners: [(f32, f32); 4],
    elapsed: f32,
    zoom: f32,
) -> Vec<StrokeInstance> {
    let min_x = corners.iter().map(|c| c.0).fold(f32::INFINITY, f32::min);
    let max_x = corners
        .iter()
        .map(|c| c.0)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = corners.iter().map(|c| c.1).fold(f32::INFINITY, f32::min);
    let max_y = corners
        .iter()
        .map(|c| c.1)
        .fold(f32::NEG_INFINITY, f32::max);
    let span = (max_y - min_y).max(1.0);
    if max_x - min_x < 1e-3 {
        return Vec::new();
    }
    let t = (elapsed / SUBJECT_SWEEP_PERIOD).fract();
    let y = min_y + t * span;
    let half = (span * zoom.max(f32::MIN_POSITIVE) * 0.08).max(2.0);
    vec![StrokeInstance {
        segment: [min_x, y, max_x, y],
        color: SUBJECT_SWEEP_COLOR,
        brush: overlay_rect_params(half),
    }]
}
