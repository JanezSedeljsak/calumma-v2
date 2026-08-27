use crate::tile_atlas::tile_mip_levels;
use bytemuck::{Pod, Zeroable};
use calumma_core::filters::AdjustmentLut;
use calumma_core::tile::{TileCoord, TILE_BYTES, TILE_SIZE};
use calumma_core::{
    BrushProfile, Document, GuideAxis, Layer, Selection, SelectionShape, StrokePoint, Tool,
    TransformHandles,
};

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
const TRANSFORM_HANDLE_BORDER_COLOR: [f32; 4] = [0.35, 0.38, 0.42, 0.9];
const TRANSFORM_HANDLE_BORDER_PX: f32 = 1.0;

const TEXT_BOX_COLOR: [f32; 4] = [0.24, 0.78, 0.84, 0.45];
const TEXT_BOX_WIDTH_PX: f32 = 0.5;
const TEXT_CARET_WIDTH_PX: f32 = 1.0;
/// Seconds for one on-off caret cycle. The blink runs off the renderer clock rather than a
/// shell timer, so nothing outside the engine has to know a caret exists.
const TEXT_CARET_BLINK_SECONDS: f32 = 1.06;

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

pub fn rgba_unit(rgba: [u8; 4]) -> [f32; 4] {
    [
        rgba[0] as f32 / 255.0,
        rgba[1] as f32 / 255.0,
        rgba[2] as f32 / 255.0,
        rgba[3] as f32 / 255.0,
    ]
}

const LAYER_HIGHLIGHT_COLOR: [f32; 4] = [0.24, 0.78, 0.84, 0.85];
const LAYER_HIGHLIGHT_WIDTH_PX: f32 = 1.0;
const LAYER_HIGHLIGHT_DASH_PX: f32 = 8.0;
const LAYER_HIGHLIGHT_GAP_PX: f32 = 8.0;
const LAYER_HIGHLIGHT_SPEED_PX: f32 = 40.0;

/// The hover outline, dashed at a constant screen period. `vs_overlay` fixes the *width* on
/// screen but not the dash pattern — the marching ants are cut here, in document space, one
/// instance per dash — so the period and the march speed are divided by the zoom instead. Both
/// halves have to be screen-anchored or the dash looks like a different pattern at every zoom.
pub fn layer_highlight_instances(
    corners: [(f32, f32); 4],
    elapsed: f32,
    zoom: f32,
) -> Vec<StrokeInstance> {
    let zoom = zoom.max(f32::MIN_POSITIVE);
    let phase = elapsed * LAYER_HIGHLIGHT_SPEED_PX / zoom;
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
            out.push(StrokeInstance {
                segment: [
                    a.0 + ux * start,
                    a.1 + uy * start,
                    a.0 + ux * end,
                    a.1 + uy * end,
                ],
                color,
                brush: brush_params(width, &BrushProfile::HARD),
            });
        }
        t += period;
    }
    out
}

/// One guide rule, as the guide pass wants it: a document-space segment plus a color. Width
/// is not per-instance because every guide is the same hairline — `board.wgsl`'s
/// `GUIDE_HALF_WIDTH_PX` owns it, in screen pixels.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct GuideInstance {
    pub segment: [f32; 4],
    pub color: [f32; 4],
}

/// Orange rather than the teal every selection and transform overlay uses, so a rule the board
/// snaps to never reads as something that is selected.
const GUIDE_COLOR: [f32; 4] = [0.94, 0.58, 0.29, 0.85];
const GUIDE_DRAGGED_COLOR: [f32; 4] = [0.94, 0.58, 0.29, 1.0];

/// Guides span the *view*, not the paper — a rule you can only see where there is paper cannot
/// be lined up against a layer hanging off it, and never meets the ruler it was pulled from.
/// This is the one thing the board draws over the desk, which is why `Renderer::render` lifts
/// the paper scissor around the guide pass alone.
pub fn guide_instances(doc: &Document) -> Vec<GuideInstance> {
    let dragged = doc.dragged_guide();
    // Edge to edge of the *view*, not of the paper. A guide is an alignment reference for the
    // whole board, and one that stopped at the paper could not be lined up against anything
    // hanging off it — nor did it meet the ruler it was pulled from, which is where the eye
    // goes to read its position.
    let (min_x, min_y, max_x, max_y) = doc.camera.viewport_doc_bounds();
    doc.guides()
        .iter()
        .enumerate()
        .map(|(index, guide)| GuideInstance {
            segment: match guide.axis {
                GuideAxis::Horizontal => [min_x, guide.position, max_x, guide.position],
                GuideAxis::Vertical => [guide.position, min_y, guide.position, max_y],
            },
            color: if dragged == Some(index) {
                GUIDE_DRAGGED_COLOR
            } else {
                GUIDE_COLOR
            },
        })
        .collect()
}

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
/// There is no "too small to draw" case to handle here any more: `Document::effective_brush_size`
/// holds the brush at `BRUSH_MIN_SCREEN_PX` across however far the board is zoomed out, so a ring
/// is always wider than the line it is drawn with. This used to collapse to a dot below three
/// screen pixels — the same threshold, stated twice in two crates.
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
    let mut out = Vec::with_capacity(4 + 1 + 5 * 2);
    let outline = |a: (f32, f32), b: (f32, f32)| StrokeInstance {
        segment: [a.0, a.1, b.0, b.1],
        color: TRANSFORM_OUTLINE_COLOR,
        brush: brush_params(TRANSFORM_OUTLINE_WIDTH_PX, &BrushProfile::HARD),
    };
    for i in 0..4 {
        out.push(outline(corners[i], corners[(i + 1) % 4]));
    }
    if let Some(rotate_handle) = rotate_handle {
        let top_mid = (
            (corners[0].0 + corners[1].0) * 0.5,
            (corners[0].1 + corners[1].1) * 0.5,
        );
        out.push(outline(top_mid, rotate_handle));
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

/// The board furniture for a live text session: a hairline box around the run's layout and
/// a caret that blinks. Both are stroke segments, the same primitive the transform overlay
/// and the lasso already draw with — no new pipeline, and nothing drawn in Swift.
pub fn text_overlay_instances(doc: &Document, elapsed: f32) -> Vec<StrokeInstance> {
    let Some((x0, y0, x1, y1)) = doc.text_box() else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(5);
    let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    for i in 0..4 {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        out.push(StrokeInstance {
            segment: [a.0, a.1, b.0, b.1],
            color: TEXT_BOX_COLOR,
            brush: brush_params(TEXT_BOX_WIDTH_PX, &BrushProfile::HARD),
        });
    }
    let visible = (elapsed / TEXT_CARET_BLINK_SECONDS).fract() < 0.5;
    if let (true, Some((a, b))) = (visible, doc.text_caret_segment()) {
        out.push(StrokeInstance {
            segment: [a.0, a.1, b.0, b.1],
            color: rgba_unit(doc.text_caret_color()),
            brush: brush_params(TEXT_CARET_WIDTH_PX, &BrushProfile::HARD),
        });
    }
    out
}

pub fn selection_rect_or_ellipse(doc: &Document) -> Option<([f32; 2], [f32; 2], Tool)> {
    match &doc.selection.as_ref()?.shape {
        SelectionShape::Rect { start, end } => {
            Some(([start.0, start.1], [end.0, end.1], Tool::Rect))
        }
        SelectionShape::Ellipse { start, end } => {
            Some(([start.0, start.1], [end.0, end.1], Tool::Ellipse))
        }
        SelectionShape::Lasso { .. } | SelectionShape::Mask(_) => None,
    }
}

pub fn selection_lasso_points(doc: &Document) -> Option<Vec<StrokePoint>> {
    let Selection {
        shape: SelectionShape::Lasso { points },
    } = doc.selection.as_ref()?
    else {
        return None;
    };
    let mut closed: Vec<StrokePoint> = points.iter().map(|&(x, y)| StrokePoint { x, y }).collect();
    if let Some(&first) = closed.first() {
        closed.push(first);
    }
    Some(closed)
}

/// Marching ants for a mask selection: the boundary the mask traced when it was committed,
/// one stroke instance per run.
///
/// The trace itself lives in the engine (`SelectionMask::trace_outline`) and is already
/// merged into maximal runs, so this is a straight mapping — the render pass never walks the
/// bitmap, no matter how large the selection is.
pub fn selection_mask_edges(
    doc: &Document,
    width: f32,
    color: [f32; 4],
) -> Option<Vec<StrokeInstance>> {
    let Selection {
        shape: SelectionShape::Mask(mask),
    } = doc.selection.as_ref()?
    else {
        return None;
    };
    Some(
        mask.outline()
            .iter()
            .map(|&segment| StrokeInstance {
                segment,
                color,
                brush: brush_params(width, &BrushProfile::HARD),
            })
            .collect(),
    )
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

/// Mask / adjustments / opacity baked into a tile before upload. Returns `None` when the
/// layer needs none of that and the tile's own bytes can go to the GPU untouched — the
/// common case, and the reason this allocates only for layers that are actually filtered.
pub fn composited_tile_payload(
    pixels: &[u8],
    coord: TileCoord,
    layer: &Layer,
    lut: Option<&AdjustmentLut>,
    doc_width: u32,
) -> Option<Vec<u8>> {
    let mask = layer.mask();
    let lut = lut.filter(|l| !l.is_neutral());
    let opacity = layer.opacity;
    if mask.is_none() && lut.is_none() && opacity >= 1.0 {
        return None;
    }
    let mut out = Vec::with_capacity(TILE_BYTES);
    out.extend_from_slice(pixels);
    out.resize(TILE_BYTES, 0);
    let (ox, oy) = coord.origin();
    for ty in 0..TILE_SIZE {
        for tx in 0..TILE_SIZE {
            let x = ox + tx as i32;
            let y = oy + ty as i32;
            let i = ((ty * TILE_SIZE + tx) * 4) as usize;
            if let Some(lut) = lut {
                let rgb = lut.apply([out[i], out[i + 1], out[i + 2]]);
                out[i..i + 3].copy_from_slice(&rgb);
            }
            if x < 0 || y < 0 {
                continue;
            }
            if let Some(mask) = mask {
                let mi = (y as u32)
                    .saturating_mul(doc_width)
                    .saturating_add(x as u32) as usize;
                if let Some(&m) = mask.get(mi) {
                    let a = out[i + 3] as u16 * m as u16 / 255;
                    out[i + 3] = a as u8;
                }
            }
            if opacity < 1.0 {
                let a = (out[i + 3] as f32) * opacity;
                out[i + 3] = a.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    Some(out)
}

/// A tile's full mip chain — `base` as level 0, then each level halved down to 1×1 — for
/// `TileAtlas::write`. Panning or zooming a document out samples these coarser levels instead
/// of raw 256×256 texels through a plain bilinear filter, which is what shimmering/moiré during
/// a pan actually is: minification aliasing from sampling a texture well below its own
/// resolution with nothing pre-filtered to fall back to.
pub fn tile_mip_chain(base: &[u8]) -> Vec<Vec<u8>> {
    let mut out = vec![base.to_vec()];
    out.extend(tile_upload_mips(base, false));
    out
}

/// The mip chain *above* level 0, which the caller already holds. Empty during camera motion,
/// where the base level is all that gets written.
pub fn tile_upload_mips(base: &[u8], motion: bool) -> Vec<Vec<u8>> {
    if motion {
        return Vec::new();
    }
    let levels = tile_mip_levels();
    let mut out: Vec<Vec<u8>> = Vec::with_capacity(levels as usize - 1);
    let mut side = TILE_SIZE;
    for _ in 1..levels {
        let prev: &[u8] = out.last().map_or(base, Vec::as_slice);
        out.push(downsample_box(prev, side));
        side = (side / 2).max(1);
    }
    debug_assert_eq!(out.len() + 1, levels as usize);
    out
}

/// One 2×2 box-filter pass, straight (non-premultiplied) alpha in and out. Averaging straight
/// RGB directly would let a fully transparent neighbour's color bleed into a translucent or
/// opaque one — invisible pixels are not "no color", they are color nobody sees yet — so each
/// tap is weighted by its own alpha (premultiplied) before averaging, and the result is
/// unpremultiplied back out. This is the same reason blending happens in premultiplied space
/// everywhere else in this renderer.
fn downsample_box(src: &[u8], side: u32) -> Vec<u8> {
    let out_side = (side / 2).max(1);
    let mut out = vec![0u8; (out_side * out_side * 4) as usize];
    for dy in 0..out_side {
        for dx in 0..out_side {
            let sx0 = (dx * 2).min(side - 1);
            let sy0 = (dy * 2).min(side - 1);
            let sx1 = (dx * 2 + 1).min(side - 1);
            let sy1 = (dy * 2 + 1).min(side - 1);

            let mut sum_rgb = [0.0f32; 3];
            let mut sum_a = 0.0f32;
            for (sx, sy) in [(sx0, sy0), (sx1, sy0), (sx0, sy1), (sx1, sy1)] {
                let i = ((sy * side + sx) * 4) as usize;
                let a = src[i + 3] as f32 / 255.0;
                sum_rgb[0] += src[i] as f32 * a;
                sum_rgb[1] += src[i + 1] as f32 * a;
                sum_rgb[2] += src[i + 2] as f32 * a;
                sum_a += a;
            }

            let out_a = sum_a / 4.0;
            let rgb = if out_a > 0.0 {
                sum_rgb.map(|c| (c / (out_a * 4.0)).round().clamp(0.0, 255.0) as u8)
            } else {
                [0u8; 3]
            };
            let di = ((dy * out_side + dx) * 4) as usize;
            out[di..di + 3].copy_from_slice(&rgb);
            out[di + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}
