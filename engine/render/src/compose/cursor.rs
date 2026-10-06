use super::{brush_params, StrokeInstance};
use calumma_core::{BrushProfile, Document};

/// Two rings a screen pixel apart, light inside dark. One colour cannot be legible over both
/// white paper and black ink, and the overlay pass has no difference blend to invert with — so
/// the cursor carries its own contrast, the way every two-tone cursor on the platform does.
const BRUSH_RING_LIGHT: [f32; 4] = [1.0, 1.0, 1.0, 0.9];
const BRUSH_RING_DARK: [f32; 4] = [0.0, 0.0, 0.0, 0.5];
const BRUSH_RING_WIDTH_PX: f32 = 1.0;
const BRUSH_RING_MIN_SEGMENTS: usize = 24;
const BRUSH_RING_MAX_SEGMENTS: usize = 96;

/// The brush cursor: where the next stamp will land, at the size it will land. Geometry is in
/// document units so the circle scales with the zoom exactly as the stamp does, while
/// `vs_overlay` holds the *line* at one screen pixel — the two halves of "screen-anchored"
/// that the hover outline splits the same way.
///
/// Empty whenever the engine says there is no ring to draw (`Document::brush_ring` owns every
/// rule about that: which tools, which layers, `⌘T`, and whether a stamp reaches this far), so
/// the renderer asks unconditionally.
///
/// There is no "too small to draw" case: `Document::effective_brush_size` holds the brush at
/// `BRUSH_MIN_SCREEN_PX` across however far the board is zoomed out.
pub fn brush_ring_instances(doc: &Document) -> Vec<StrokeInstance> {
    let Some((centre, radius)) = doc.brush_ring() else {
        return Vec::new();
    };
    let zoom = doc.camera.zoom.max(f32::MIN_POSITIVE);
    let screen_radius = radius * zoom;
    // A screen pixel in document units — the same conversion the marching ants make, and what
    // keeps the two rings exactly one pixel apart at every zoom.
    let pixel = 1.0 / zoom;
    let segments = (screen_radius as usize).clamp(BRUSH_RING_MIN_SEGMENTS, BRUSH_RING_MAX_SEGMENTS);
    let mut out = Vec::with_capacity(segments * 2);
    for (r, color) in [
        (radius + pixel * 0.5, BRUSH_RING_DARK),
        (radius - pixel * 0.5, BRUSH_RING_LIGHT),
    ] {
        push_circle(&mut out, centre, r.max(pixel), segments, color);
    }
    out
}

fn push_circle(
    out: &mut Vec<StrokeInstance>,
    centre: (f32, f32),
    radius: f32,
    segments: usize,
    color: [f32; 4],
) {
    let step = std::f32::consts::TAU / segments as f32;
    let point = |i: usize| {
        let (sin, cos) = (i as f32 * step).sin_cos();
        (centre.0 + cos * radius, centre.1 + sin * radius)
    };
    let mut previous = point(0);
    for i in 1..=segments {
        let next = point(i);
        out.push(StrokeInstance {
            segment: [previous.0, previous.1, next.0, next.1],
            color,
            brush: brush_params(BRUSH_RING_WIDTH_PX, &BrushProfile::HARD),
        });
        previous = next;
    }
}

/// White over a dark halo, the same two-tone contrast the brush ring uses — one colour cannot
/// stay legible over both white paper and black ink.
const CLONE_CROSSHAIR_LIGHT: [f32; 4] = [1.0, 1.0, 1.0, 0.95];
const CLONE_CROSSHAIR_DARK: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
const CLONE_CROSSHAIR_RADIUS_PX: f32 = 6.0;
const CLONE_CROSSHAIR_WIDTH_PX: f32 = 1.4;

/// The clone stamp's / healing brush's source indicator: a small crosshair that tracks where
/// the next stamp reads from. Fixed screen size regardless of zoom (`vs_overlay`/`fs_overlay`,
/// like every other piece of board furniture), unlike the brush ring at the destination, whose
/// radius *is* the stamp size and so scales with the zoom on purpose.
pub fn clone_source_overlay_instances(doc: &Document) -> Vec<StrokeInstance> {
    let Some(centre) = doc.clone_source_cursor() else {
        return Vec::new();
    };
    let zoom = doc.camera.zoom.max(f32::MIN_POSITIVE);
    let half = CLONE_CROSSHAIR_RADIUS_PX / zoom;
    let mut out = Vec::with_capacity(4);
    for color in [CLONE_CROSSHAIR_DARK, CLONE_CROSSHAIR_LIGHT] {
        let width = if color == CLONE_CROSSHAIR_DARK {
            CLONE_CROSSHAIR_WIDTH_PX + 1.0
        } else {
            CLONE_CROSSHAIR_WIDTH_PX
        };
        out.push(StrokeInstance {
            segment: [centre.0 - half, centre.1, centre.0 + half, centre.1],
            color,
            brush: brush_params(width, &BrushProfile::HARD),
        });
        out.push(StrokeInstance {
            segment: [centre.0, centre.1 - half, centre.0, centre.1 + half],
            color,
            brush: brush_params(width, &BrushProfile::HARD),
        });
    }
    out
}
