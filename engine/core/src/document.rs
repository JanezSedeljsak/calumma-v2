use crate::brush::{Brush, BrushProfile};
use crate::camera::Camera;
use crate::coverage::CoverageGrid;
use crate::filters::AdjustmentLut;
use crate::guide::{Guide, GuideDrag};
use crate::history::{History, TileSnapshot};
use crate::layer::Layer;
use crate::limits::{
    ALPHA_MAX, ALPHA_OPAQUE, BLUR_STRENGTH_DEFAULT, BLUR_STRENGTH_MAX, BLUR_STRENGTH_MIN,
    BRUSH_SIZE_DEFAULT, DEFAULT_INK, EFFECT_CHUNK_BYTES, ERASER_HARDNESS_DEFAULT,
    ERASER_HARDNESS_MAX, ERASER_HARDNESS_MIN, EYEDROPPER_RADIUS_DEFAULT, EYEDROPPER_RADIUS_MAX,
    EYEDROPPER_RADIUS_MIN, INK_OPACITY_DEFAULT, INK_OPACITY_MAX, INK_OPACITY_MIN, MAX_CANVAS_SIDE,
    MIN_CANVAS_SIDE, MIN_STAMP_SPACING, MIN_STROKE_POINT_DISTANCE, PAPER_WHITE,
    STAMP_COVERAGE_PADDING, STAMP_SPACING_RATIO, STROKE_POINT_CAPACITY, TOLERANCE_DEFAULT,
    TOLERANCE_MAX, TOLERANCE_MIN,
};
use crate::palette::BoardColors;
use crate::selection::{Selection, SelectionShape};
use crate::shape::{ink_sample, Shape, Tool};
use crate::text_edit::TextEdit;
use crate::tile::{blend_over, blend_with_mode, DirtyChannel, DocRect, TileCoord, TileSet};
use crate::tool_gate::{accepts_pixels, ToolBlock};
use crate::transform::{bounds_center, clipped_pixel_span, corner_scale, LayerTransform};
use crate::vector;
use crate::vector_edit::{VectorItemDrag, VectorPick};
use calumma_text::TextRun;
use rayon::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokePoint {
    pub x: f32,
    pub y: f32,
}

pub fn stamp_spacing(radius: f32) -> f32 {
    (radius * STAMP_SPACING_RATIO).max(MIN_STAMP_SPACING)
}

pub fn stroke_stamps(points: &[StrokePoint], radius: f32) -> Vec<StrokePoint> {
    let mut out = Vec::with_capacity(points.len());
    let Some(first) = points.first() else {
        return out;
    };
    out.push(*first);
    let spacing = stamp_spacing(radius);
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let distance = (dx * dx + dy * dy).sqrt();
        if !distance.is_finite() || distance <= spacing {
            out.push(b);
            continue;
        }
        let steps = (distance / spacing).ceil() as usize;
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            out.push(StrokePoint {
                x: a.x + dx * t,
                y: a.y + dy * t,
            });
        }
    }
    out
}

pub(crate) fn apply_mask(rgba: &mut [u8], mask: Option<&[u8]>) {
    let Some(mask) = mask else {
        return;
    };
    let chunk_pixels = EFFECT_CHUNK_BYTES / 4;
    rgba.par_chunks_mut(EFFECT_CHUNK_BYTES)
        .zip(mask.par_chunks(chunk_pixels))
        .for_each(|(block, mask_block)| {
            for (px, &m) in block.chunks_exact_mut(4).zip(mask_block.iter()) {
                px[3] = ((px[3] as u32 * m as u32) / 255) as u8;
            }
        });
}

fn layer_source_pixel(layer: &Layer, doc_x: f32, doc_y: f32) -> [u8; 4] {
    let Some(tiles) = layer.tiles() else {
        return match layer.content.item() {
            Some(item) => vector_source_pixel(item, layer, doc_x, doc_y),
            None => [0, 0, 0, 0],
        };
    };
    let (sx, sy) = match layer.transform {
        Some(t) => {
            let Some(raw_bounds) = layer.content_bounds() else {
                return [0, 0, 0, 0];
            };
            t.inverse(bounds_center(raw_bounds), (doc_x, doc_y))
        }
        None => (doc_x, doc_y),
    };
    tiles.get_pixel(sx.floor() as i32, sy.floor() as i32)
}

/// One point of a vector layer, as a color rather than the alpha `vector_alpha_at` answers
/// picking with. This is the per-pixel twin of `vector::rasterize_into_rgba`'s inner loop —
/// same inverse map, same coverage, same `blend_over` — so the zoomed-out overview proxy
/// shows a layer of shapes exactly as the flatten and the shader do, instead of the empty
/// board it would get from a layer that has no tiles to sample.
fn vector_source_pixel(
    item: &vector::VectorItem,
    layer: &Layer,
    doc_x: f32,
    doc_y: f32,
) -> [u8; 4] {
    let local = match layer
        .transform
        .filter(|t| !t.is_identity())
        .zip(item.bounds())
    {
        Some((t, raw)) => t.inverse(bounds_center(raw), (doc_x, doc_y)),
        None => (doc_x, doc_y),
    };
    let coverage = item.coverage(local.0, local.1);
    if coverage <= 0.0 {
        return [0, 0, 0, 0];
    }
    let mut src = item.color();
    src[3] = ((src[3] as f32) * coverage).round().clamp(0.0, 255.0) as u8;
    src
}

/// Hit-testing a vector layer evaluates its items' coverage directly rather than sampling
/// pixels — there are no pixels to sample. The point is inverse-mapped through the layer
/// transform first, exactly as the rasterizer and the shader both do.
fn vector_alpha_at(item: &vector::VectorItem, layer: &Layer, doc_x: f32, doc_y: f32) -> u8 {
    let local = match layer
        .transform
        .filter(|t| !t.is_identity())
        .zip(item.bounds())
    {
        Some((t, raw)) => t.inverse(bounds_center(raw), (doc_x, doc_y)),
        None => (doc_x, doc_y),
    };
    let coverage = item.coverage(local.0, local.1) * (item.color()[3] as f32 / 255.0);
    (coverage * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Alpha alone, for hit-testing. Adjustments never touch alpha, so picking skips the
/// color work `layer_composited_pixel` does — an HSL round trip per layer per click.
pub(crate) fn layer_alpha_at(layer: &Layer, doc_x: f32, doc_y: f32, doc_w: u32, doc_h: u32) -> u8 {
    let alpha = match layer.content.item() {
        Some(item) => vector_alpha_at(item, layer, doc_x, doc_y),
        None => layer_source_pixel(layer, doc_x, doc_y)[3],
    };
    if alpha == 0 {
        return 0;
    }
    let alpha = match layer.mask() {
        Some(mask) => {
            let ix = doc_x.floor() as i32;
            let iy = doc_y.floor() as i32;
            if ix < 0 || iy < 0 || (ix as u32) >= doc_w || (iy as u32) >= doc_h {
                return 0;
            }
            let index = (iy as u32 * doc_w + ix as u32) as usize;
            match mask.get(index) {
                Some(&m) => ((alpha as u32 * m as u32) / 255) as u8,
                None => alpha,
            }
        }
        None => alpha,
    };
    if layer.opacity < 1.0 {
        ((alpha as f32) * layer.opacity).round().clamp(0.0, 255.0) as u8
    } else {
        alpha
    }
}

/// A layer paired with the document-space box it can paint into — what
/// `Document::contributing_layers` hands the per-pixel composite so the loop can skip a layer
/// without touching its pixels.
type BoundedLayer<'a> = (&'a Layer, (f32, f32, f32, f32));

fn layer_composited_pixel(
    layer: &Layer,
    doc_x: f32,
    doc_y: f32,
    doc_w: u32,
    doc_h: u32,
) -> [u8; 4] {
    let mut px = layer_source_pixel(layer, doc_x, doc_y);
    if px[3] == 0 {
        return px;
    }
    if let Some(mask) = layer.mask() {
        let ix = doc_x.floor() as i32;
        let iy = doc_y.floor() as i32;
        if ix >= 0 && iy >= 0 && (ix as u32) < doc_w && (iy as u32) < doc_h {
            let index = (iy as u32 * doc_w + ix as u32) as usize;
            if let Some(&m) = mask.get(index) {
                px[3] = ((px[3] as u32 * m as u32) / 255) as u8;
            }
        } else {
            px[3] = 0;
        }
    }
    if let Some(adj) = layer.adjustments.as_ref().filter(|a| !a.is_neutral()) {
        let rgb = crate::filters::apply([px[0], px[1], px[2]], adj);
        px[0] = rgb[0];
        px[1] = rgb[1];
        px[2] = rgb[2];
    }
    if layer.opacity < 1.0 {
        px[3] = ((px[3] as f32) * layer.opacity).round().clamp(0.0, 255.0) as u8;
    }
    px
}

pub(crate) fn copy_layer_into_rgba(layer: &Layer, buf: &mut [u8], w: u32, h: u32) {
    if let Some(item) = layer.content.item() {
        vector::rasterize_into_rgba(item, layer.transform, buf, w, h);
        return;
    }
    let Some(tiles) = layer.tiles() else {
        return;
    };
    let Some(t) = layer.transform.filter(|t| !t.is_identity()) else {
        tiles.copy_into_rgba(buf, w, h);
        return;
    };
    let Some(raw_bounds) = layer.content_bounds() else {
        return;
    };
    let pivot = bounds_center(raw_bounds);
    let Some((x0, y0, x1, y1)) = clipped_pixel_span(t.transformed_aabb(raw_bounds), w, h) else {
        return;
    };
    let row_bytes = (w as usize) * 4;
    let y0 = y0 as usize;
    let x0 = x0 as usize;
    let x1 = x1 as usize;
    buf[y0 * row_bytes..y1 as usize * row_bytes]
        .par_chunks_mut(row_bytes)
        .enumerate()
        .for_each(|(i, row)| {
            let y = y0 + i;
            for x in x0..x1 {
                let (rx, ry) = t.inverse(pivot, (x as f32 + 0.5, y as f32 + 0.5));
                let px = tiles.get_pixel(rx.floor() as i32, ry.floor() as i32);
                if px[3] == 0 {
                    continue;
                }
                row[x * 4..x * 4 + 4].copy_from_slice(&px);
            }
        });
}

pub(crate) fn apply_layer_effects(rgba: &mut [u8], layer: &Layer, lut: Option<&AdjustmentLut>) {
    let lut = lut.filter(|l| !l.is_neutral());
    let opacity = layer.opacity;
    if lut.is_none() && opacity >= 1.0 {
        return;
    }
    rgba.par_chunks_mut(EFFECT_CHUNK_BYTES).for_each(|block| {
        for chunk in block.chunks_exact_mut(4) {
            if let Some(lut) = lut {
                let rgb = lut.apply([chunk[0], chunk[1], chunk[2]]);
                chunk[0..3].copy_from_slice(&rgb);
            }
            if opacity < 1.0 {
                chunk[3] = ((chunk[3] as f32) * opacity).round().clamp(0.0, 255.0) as u8;
            }
        }
    });
}

fn tiles_covering(rect: DocRect, out: &mut TileSet) {
    let (tx0, ty0, tx1, ty1) = rect.tile_span();
    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            out.insert(TileCoord { x: tx, y: ty });
        }
    }
}

fn stamps_bounds(stamps: &[StrokePoint], radius: f32) -> Option<DocRect> {
    let pad = radius + STAMP_COVERAGE_PADDING;
    let first = stamps.first()?;
    let mut min_x = first.x;
    let mut min_y = first.y;
    let mut max_x = first.x;
    let mut max_y = first.y;
    for p in stamps {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }
    Some(DocRect::from_floats(
        min_x - pad,
        min_y - pad,
        max_x + pad,
        max_y + pad,
    ))
}

#[derive(Clone, Debug)]
pub struct Document {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub layers: Vec<Layer>,
    pub active_layer: usize,
    pub camera: Camera,
    pub history: History,
    pub tool: Tool,
    pub color: [u8; 4],
    /// The outline color the area shape tools use — the shell's **primary** swatch. Separate
    /// from `color` because `color` follows whichever swatch the picker has selected, while a
    /// shape's two parts are always the same two swatches. A shell knob like `color`.
    pub stroke_color: [u8; 4],
    /// The interior color the area shape tools use — the shell's **secondary** swatch. A
    /// rectangle is drawn the way it is described: outlined in the primary color, filled with
    /// the secondary one.
    pub shape_fill_color: [u8; 4],
    pub brush_size: f32,
    pub ink_opacity: f32,
    pub fill: bool,
    /// Whether the area shape tools draw an outline. Independent of `fill`: either, both, or
    /// neither.
    pub stroke: bool,
    pub dark_theme: bool,
    pub accent: [u8; 3],
    pub board_colors: BoardColors,
    pub hover_layer: Option<usize>,
    /// Where the pointer is on the paper, in document units, whether or not a button is down.
    /// Only the brush cursor reads it (`brush_cursor.rs`); it is `None` whenever the pointer is
    /// off the board.
    pub(crate) pointer_hover: Option<(f32, f32)>,
    pub stroke_active: bool,
    pub stroke_points: Vec<StrokePoint>,
    /// Bumped once per `begin_stroke`, never reused. The renderer accumulates a brush stroke's
    /// GPU coverage across frames instead of redrawing it from the first point every time, so
    /// it needs to know when the points it is appending to belong to a *different* stroke than
    /// the ones already in the coverage target. Point count alone cannot answer that: two
    /// strokes can pass through the same length between one frame and the next.
    ///
    /// Also bumped whenever `push_stroke_point` **rewinds** the list instead of extending it,
    /// which is what a Shift-held straight segment does on every event. That is the whole
    /// contract this number carries: while the generation holds, `stroke_points` is an
    /// append-only extension of what it was, so coverage already unioned into the GPU target is
    /// still a prefix of the answer. A `Max` blend cannot take a capsule back out, so a rewound
    /// tail has to read as a different stroke.
    stroke_generation: u64,
    /// Index into `stroke_points` the current straight segment pivots on, set the moment
    /// Shift is first seen held during a Pen/Eraser stroke and cleared on release — so toggling
    /// Shift mid-stroke straightens only the segment drawn while it was held, matching the
    /// Shift-constrain read-on-render pattern `preview_shape()` uses for shapes.
    stroke_straight_anchor: Option<usize>,
    /// The drag the pointer is describing, with its **unclamped** end. What gets drawn and
    /// committed is `preview_shape()`, which applies the Shift constraint on read — so
    /// pressing or releasing Shift mid-drag changes the shape without needing a pointer
    /// event to arrive first.
    pub shape_drag: Option<Shape>,
    pub selection: Option<Selection>,
    /// Guides pulled off the rulers, in document pixels. Board furniture rather than content:
    /// they draw over every layer, they scope nothing, and the only thing they change about an
    /// edit is where it lands (`snap_doc_point` / `snap_box_offset`).
    pub(crate) guides: Vec<Guide>,
    pub(crate) guide_drag: Option<GuideDrag>,
    pub shift_held: bool,
    /// Whether the shape tools and the pen commit as resolution-independent vector items
    /// instead of stamping pixels. A shell knob, like `fill`.
    pub vector_mode: bool,
    /// The reason the last board press did nothing, waiting to be said out loud, and the
    /// (layer, tool) pair it was already said for. It lives on the document because the
    /// document is the only thing that knows a press was refused.
    pub(crate) blocked_notice: Option<ToolBlock>,
    pub(crate) blocked_notice_key: Option<(usize, Tool)>,
    vector_revision: u64,
    pub(crate) selected_vector: Option<VectorPick>,
    pub(crate) vector_drag: Option<VectorItemDrag>,
    pub last_shape_tool: Tool,
    pub last_select_tool: Tool,
    pub transform_active: bool,
    /// Font, size and alignment the next text layer starts with — a document-level default
    /// carried between text layers, not a shell knob.
    pub text_style: TextRun,
    pub text_edit: Option<TextEdit>,
    pub(crate) transform_drag: Option<TransformDrag>,
    pub(crate) layer_selection: Vec<usize>,
    stroke_before: TileSnapshot,
    /// How far each pixel the blur brush passes over travels toward its blurred neighbourhood.
    /// A document-level knob like `brush_size`, not a shell one — see `blur.rs`.
    pub blur_strength: f32,
    /// How far a flood may stray from the color it started on. One knob for the bucket and
    /// the magic wand both, since they are one traversal.
    pub tolerance: u8,
    /// Match color for `Tool::SelectColor`, pushed from the shell's tertiary swatch.
    pub select_color: [u8; 4],
    /// Radius of the disc the eyedropper averages over — see
    /// `limits::EYEDROPPER_RADIUS_DEFAULT`.
    pub eyedropper_radius: u32,
    /// Which brush the pen lays ink down with. A shell knob like the active tool: the shell
    /// picks it, `brush.rs` owns what it means.
    pub brush: Brush,
    /// How sharp the eraser's rim is. The eraser's own knob rather than the pen's brush —
    /// see `limits::ERASER_HARDNESS_DEFAULT`.
    pub eraser_hardness: f32,
    /// How many of the current stroke's stamps the blur has already committed. Blur has no ink
    /// preview, so it paints as the pointer moves; this is what stops each event re-blurring
    /// the whole stroke from the start.
    blur_stamped: usize,
    /// Whether the current blur stroke has actually changed a pixel. A stroke that touched
    /// nothing — strength at zero, or dragged across empty space — must not leave an undo
    /// entry behind, and the snapshot alone cannot tell the difference.
    blur_painted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TransformHandle {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
    Rotate,
    Move,
}

impl TransformHandle {
    /// The four scale handles in the order `LayerTransform::transformed_corners` emits them,
    /// so a corner index and a handle are the same fact read two ways. Both the layer frame
    /// and a vector item's frame zip against this.
    pub(crate) const CORNERS: [Self; 4] = [
        Self::TopLeft,
        Self::TopRight,
        Self::BottomRight,
        Self::BottomLeft,
    ];

    /// Which way this corner points from the box centre, or `None` for the two handles that
    /// do not scale anything.
    pub(crate) fn corner_signs(self) -> Option<(f32, f32)> {
        Some(match self {
            Self::TopLeft => (-1.0, -1.0),
            Self::TopRight => (1.0, -1.0),
            Self::BottomRight => (1.0, 1.0),
            Self::BottomLeft => (-1.0, 1.0),
            Self::Rotate | Self::Move => return None,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TransformTarget {
    pub(crate) layer_index: usize,
    pub(crate) pivot: (f32, f32),
    pub(crate) raw_bounds: (f32, f32, f32, f32),
    pub(crate) start_transform: LayerTransform,
}

#[derive(Clone, Debug)]
pub(crate) struct TransformDrag {
    pub(crate) targets: Vec<TransformTarget>,
    pub(crate) handle: TransformHandle,
    pub(crate) start_pointer: (f32, f32),
}

impl TransformDrag {
    pub(crate) fn single(
        layer_index: usize,
        handle: TransformHandle,
        pivot: (f32, f32),
        raw_bounds: (f32, f32, f32, f32),
        start_transform: LayerTransform,
        start_pointer: (f32, f32),
    ) -> Self {
        Self {
            targets: vec![TransformTarget {
                layer_index,
                pivot,
                raw_bounds,
                start_transform,
            }],
            handle,
            start_pointer,
        }
    }

    pub(crate) fn layer_move(targets: Vec<TransformTarget>, start_pointer: (f32, f32)) -> Self {
        Self {
            targets,
            handle: TransformHandle::Move,
            start_pointer,
        }
    }

    pub(crate) fn layer_index(&self) -> usize {
        self.targets[0].layer_index
    }

    pub(crate) fn primary(&self) -> &TransformTarget {
        &self.targets[0]
    }

    /// The frame's centre *on the board*. `pivot` is the raw content centre, which `forward`
    /// rotates and scales about before it translates — so once a layer has been moved, the
    /// box the user is dragging is no longer centred there. Rotation and corner scale both
    /// have to measure from what is on screen, or a moved layer turns about a point off in
    /// space and its corners jump the moment they are grabbed.
    pub(crate) fn center(&self) -> (f32, f32) {
        let target = self.primary();
        (
            target.pivot.0 + target.start_transform.offset_x,
            target.pivot.1 + target.start_transform.offset_y,
        )
    }
}

pub type TransformHandles = (usize, [(f32, f32); 4], (f32, f32));

pub(crate) const HANDLE_HIT_RADIUS_PX: f32 = 10.0;
const ROTATE_HANDLE_OFFSET_PX: f32 = 24.0;

pub(crate) fn point_dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// A direction of length one, or `None` when there is no direction to take — a degenerate
/// edge has to be caught by the caller, not normalised into a random bearing.
fn unit(v: (f32, f32)) -> Option<(f32, f32)> {
    let len = (v.0 * v.0 + v.1 * v.1).sqrt();
    (len > 1e-6).then(|| (v.0 / len, v.1 / len))
}

fn angle_from(pivot: (f32, f32), p: (f32, f32)) -> f32 {
    (p.1 - pivot.1).atan2(p.0 - pivot.0)
}

fn point_in_quad(p: (f32, f32), quad: [(f32, f32); 4]) -> bool {
    let mut sign = 0.0f32;
    for i in 0..4 {
        let a = quad[i];
        let b = quad[(i + 1) % 4];
        let edge = (b.0 - a.0, b.1 - a.1);
        let to_p = (p.0 - a.0, p.1 - a.1);
        let cross = edge.0 * to_p.1 - edge.1 * to_p.0;
        if cross.abs() < 1e-6 {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    true
}

/// A color with the ink-opacity slider folded into its alpha. The one place that happens,
/// so a fill and its stroke cannot disagree about how translucent the shape is.
fn glazed(color: [u8; 4], opacity: f32) -> [u8; 4] {
    let mut rgba = color;
    rgba[3] = ((color[3] as f32) * opacity).round().clamp(0.0, 255.0) as u8;
    rgba
}

fn union_aabb_after_delta(targets: &[TransformTarget], dx: f32, dy: f32) -> (f32, f32, f32, f32) {
    let mut union = (
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    );
    for target in targets {
        let mut next = target.start_transform;
        next.offset_x += dx;
        next.offset_y += dy;
        let aabb = next.transformed_aabb(target.raw_bounds);
        union.0 = union.0.min(aabb.0);
        union.1 = union.1.min(aabb.1);
        union.2 = union.2.max(aabb.2);
        union.3 = union.3.max(aabb.3);
    }
    union
}

impl Document {
    pub fn new(id: String, name: impl Into<String>, width: u32, height: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let paper = Layer::paper(width, height);
        let paint = Layer::new(crate::names::LAYER_ONE, width, height);
        Self {
            id,
            name: name.into(),
            width,
            height,
            layers: vec![paper, paint],
            active_layer: 1,
            camera: Camera::default(),
            history: History::default(),
            tool: Tool::Pen,
            color: DEFAULT_INK,
            stroke_color: DEFAULT_INK,
            shape_fill_color: PAPER_WHITE,
            brush_size: BRUSH_SIZE_DEFAULT,
            ink_opacity: INK_OPACITY_DEFAULT,
            fill: false,
            stroke: true,
            dark_theme: true,
            accent: crate::palette::random_project_color(),
            board_colors: BoardColors::fallback(true),
            hover_layer: None,
            pointer_hover: None,
            stroke_active: false,
            stroke_points: Vec::with_capacity(STROKE_POINT_CAPACITY),
            stroke_generation: 0,
            stroke_straight_anchor: None,
            shape_drag: None,
            selection: None,
            guides: Vec::new(),
            guide_drag: None,
            shift_held: false,
            vector_mode: false,
            blocked_notice: None,
            blocked_notice_key: None,
            vector_revision: 0,
            selected_vector: None,
            vector_drag: None,
            last_shape_tool: Tool::Rect,
            last_select_tool: Tool::SelectRect,
            transform_active: false,
            text_style: TextRun::default(),
            text_edit: None,
            transform_drag: None,
            layer_selection: Vec::new(),
            stroke_before: TileSnapshot::default(),
            blur_strength: BLUR_STRENGTH_DEFAULT,
            tolerance: TOLERANCE_DEFAULT,
            select_color: DEFAULT_INK,
            eyedropper_radius: EYEDROPPER_RADIUS_DEFAULT,
            brush: Brush::default(),
            eraser_hardness: ERASER_HARDNESS_DEFAULT,
            blur_stamped: 0,
            blur_painted: false,
        }
    }

    pub fn ensure_paper_layer(&mut self) {
        if self.layers.iter().any(Layer::is_paper) {
            return;
        }
        self.layers.insert(0, Layer::paper(self.width, self.height));
        self.active_layer += 1;
    }

    pub fn bounds(&self) -> DocRect {
        DocRect::from_size(self.width, self.height)
    }

    pub fn visible_rect(&self) -> Option<DocRect> {
        self.camera
            .visible_doc_rect(self.width as f32, self.height as f32)
    }

    pub fn active(&self) -> Option<&Layer> {
        self.layers.get(self.active_layer)
    }

    pub fn active_mut(&mut self) -> Option<&mut Layer> {
        self.layers.get_mut(self.active_layer)
    }

    pub fn add_layer(&mut self, name: impl Into<String>) {
        self.record_stack_history();
        self.layers.push(Layer::new(name, self.width, self.height));
        self.active_layer = self.layers.len() - 1;
    }

    pub(crate) fn push_layer(&mut self, name: impl Into<String>) {
        self.layers.push(Layer::new(name, self.width, self.height));
        self.active_layer = self.layers.len() - 1;
    }

    /// Whether the active layer can take pixels, for the commands that are not a tool press —
    /// paste, clear, clear-selection. Anything reached for *through a tool* asks `tool_block`
    /// instead, so it gets a reason it can show rather than a bare `false`.
    pub fn active_layer_accepts_paint(&self) -> bool {
        self.layers
            .get(self.active_layer)
            .is_some_and(accepts_pixels)
    }

    pub fn place_image(&mut self, rgba: &[u8], width: u32, height: u32) -> bool {
        if !self.active_layer_accepts_paint() {
            return false;
        }
        let expected = (width as usize) * (height as usize) * 4;
        if width == 0 || height == 0 || rgba.len() < expected {
            return false;
        }
        let Some(tiles) = self.active_mut().and_then(|layer| layer.tiles_mut()) else {
            return false;
        };
        tiles.blit_rgba(rgba, width, height) > 0
    }

    pub fn remove_layer(&mut self, index: usize) -> bool {
        self.remove_layer_inner(index, true)
    }

    pub(crate) fn remove_layer_inner(&mut self, index: usize, record: bool) -> bool {
        if index >= self.layers.len() {
            return false;
        }
        self.commit_text();
        self.clear_vector_selection();
        if record {
            self.record_stack_history();
        }
        self.layers.remove(index);
        if self.layers.is_empty() {
            self.active_layer = 0;
            self.hover_layer = None;
            return true;
        }
        if self.active_layer > index {
            self.active_layer -= 1;
        } else if self.active_layer >= self.layers.len() {
            self.active_layer = self.layers.len() - 1;
        }
        if let Some(hover) = self.hover_layer {
            if hover == index {
                self.hover_layer = None;
            } else if hover > index {
                self.hover_layer = Some(hover - 1);
            }
        }
        self.layer_selection.retain(|&i| i != index);
        for selected in &mut self.layer_selection {
            if *selected > index {
                *selected -= 1;
            }
        }
        true
    }

    pub fn set_layer_visible(&mut self, index: usize, visible: bool) {
        if let Some(layer) = self.layers.get_mut(index) {
            layer.visible = visible;
        }
    }

    pub fn set_active_layer(&mut self, index: usize) {
        if index < self.layers.len() {
            if index != self.active_layer {
                self.commit_text();
                self.clear_vector_selection();
            }
            self.active_layer = index;
        }
    }

    /// No `mark_channel_dirty(Render)`: opacity is read by `fs_tile` off the `LayerData` row
    /// (`Renderer::write_layer_data`), not baked into tile bytes, so nothing about the tiles
    /// themselves went stale. The caller still owes the renderer an `invalidate()` — same as a
    /// `⌘T` drag, which rewrites the same row for the same reason.
    pub fn set_layer_opacity(&mut self, index: usize, opacity: f32) {
        let new_opacity = opacity.clamp(0.0, 1.0);
        let Some(layer) = self.layers.get(index) else {
            return;
        };
        if (layer.opacity - new_opacity).abs() < 1e-6 {
            return;
        }
        self.record_layer_props_history(index);
        if let Some(layer) = self.layers.get_mut(index) {
            layer.opacity = new_opacity;
        }
    }

    pub fn set_layer_blend_mode(&mut self, index: usize, mode: crate::layer::BlendMode) {
        let Some(layer) = self.layers.get(index) else {
            return;
        };
        if layer.blend_mode == mode {
            return;
        }
        self.record_layer_props_history(index);
        if let Some(layer) = self.layers.get_mut(index) {
            layer.blend_mode = mode;
        }
    }

    /// No `mark_channel_dirty(Render)`, for the same reason `set_layer_opacity` above dropped
    /// it: the adjustment LUT is evaluated per pixel in `fs_tile` off the `LayerData` row, so a
    /// slider drag never re-walks a single tile. Masked tiles still bake on the CPU, but that is
    /// `composited_tile_payload` reacting to the mask, not to this.
    pub fn set_layer_adjustments(
        &mut self,
        index: usize,
        adjustments: crate::filters::Adjustments,
    ) {
        let Some(layer) = self.layers.get(index) else {
            return;
        };
        let adjustments = adjustments.clamped();
        let next = if adjustments.is_neutral() {
            None
        } else {
            Some(adjustments)
        };
        if layer.adjustments == next {
            return;
        }
        self.record_layer_props_history(index);
        if let Some(layer) = self.layers.get_mut(index) {
            layer.adjustments = next;
        }
    }

    pub fn nudge_layer_adjustment(
        &mut self,
        index: usize,
        kind: crate::filters::AdjustmentKind,
        steps: f32,
    ) -> bool {
        let Some(layer) = self.layers.get(index) else {
            return false;
        };
        let current = layer.adjustments.unwrap_or_default();
        let next = current.nudged(kind, steps);
        if next == current {
            return false;
        }
        self.set_layer_adjustments(index, next);
        true
    }

    pub fn add_vector_layer(&mut self, name: impl Into<String>, item: vector::VectorItem) -> usize {
        self.layers.push(Layer::vector(name, item));
        self.active_layer = self.layers.len() - 1;
        self.bump_vector_revision();
        self.active_layer
    }

    fn push_vector_item(&mut self, item: vector::VectorItem) {
        self.record_stack_history();
        let n = self.layers.iter().filter(|l| l.content.is_vector()).count() + 1;
        self.add_vector_layer(crate::names::numbered_vector_layer(n), item);
    }

    /// Vector layers have no tile cache to diff, so nothing about them is incremental: a
    /// counter is all the renderer needs to know its draw list is stale and must be rebuilt
    /// whole. Scaling one bumps this the same way adding an item does.
    pub fn vector_revision(&self) -> u64 {
        self.vector_revision
    }

    pub fn bump_vector_revision(&mut self) {
        self.vector_revision = self.vector_revision.wrapping_add(1);
    }

    pub fn set_vector_mode(&mut self, on: bool) {
        self.vector_mode = on;
    }

    pub fn set_shift_held(&mut self, held: bool) {
        self.shift_held = held;
    }

    /// The shape as it will be drawn and committed: the live drag with the Shift constraint
    /// applied. Deriving it here rather than storing it clamped keeps one source of truth —
    /// the raw drag — so the modifier can be pressed and released as often as the user likes
    /// and the answer is always current.
    pub fn preview_shape(&self) -> Option<Shape> {
        let mut shape = self.shape_drag?;
        shape.start = self.snap_doc_point(shape.start);
        shape.end = self.snap_doc_point(shape.end);
        if self.shift_held && shape.tool.constrains_to_square() {
            shape.end = crate::shape::square_end(shape.start, shape.end);
        }
        Some(shape)
    }

    pub fn reset_layer_transform(&mut self, index: usize) {
        let Some(layer) = self.layers.get(index) else {
            return;
        };
        if layer.locked || layer.transform.is_none() {
            return;
        }
        self.record_layer_props_history(index);
        if let Some(layer) = self.layers.get_mut(index) {
            layer.transform = None;
        }
    }

    pub fn layer_transform(&self, index: usize) -> LayerTransform {
        self.layers
            .get(index)
            .and_then(|l| l.transform)
            .unwrap_or_default()
    }

    pub fn enter_transform(&mut self) -> bool {
        if self.tool_blocked(Tool::Transform) {
            return false;
        }
        self.transform_active = true;
        true
    }

    pub fn exit_transform(&mut self) {
        self.transform_active = false;
        self.transform_drag = None;
        self.clear_vector_selection();
    }

    pub fn toggle_transform(&mut self) -> bool {
        if self.transform_active {
            self.exit_transform();
            false
        } else {
            self.enter_transform()
        }
    }

    /// The rotate grip: always `ROTATE_HANDLE_OFFSET_PX` clear of the middle of the frame's
    /// top edge and square to it, at every rotation, scale and flip.
    ///
    /// It reads the drawn corners rather than the transform, because the transform's own
    /// centre is `pivot` *before* translation. Measuring the stalk from there made a moved
    /// layer's grip lean off along the top edge by the offset — the further the layer was
    /// dragged, the further the grip slid sideways and, at a large enough offset, right off
    /// the corner. `forward` scales on the box's own axes before it rotates, so the frame is
    /// always a rectangle and its top edge has a true normal; there is no shear to correct.
    fn rotate_handle_position(corners: [(f32, f32); 4], zoom: f32) -> (f32, f32) {
        let [tl, tr, br, bl] = corners;
        let top_mid = ((tl.0 + tr.0) * 0.5, (tl.1 + tr.1) * 0.5);
        let center = ((tl.0 + br.0) * 0.5, (tl.1 + br.1) * 0.5);
        // A zero-width box has no top edge to stand square to, so fall back to the side it
        // does have; a box with neither keeps the grip above it rather than nowhere.
        let mut dir = unit((tr.1 - tl.1, tl.0 - tr.0))
            .or_else(|| unit((tl.0 - bl.0, tl.1 - bl.1)))
            .unwrap_or((0.0, -1.0));
        // Outward, never into the box: a vertical flip turns the local top edge into the
        // lower one on screen, and the normal has to turn with it.
        if dir.0 * (top_mid.0 - center.0) + dir.1 * (top_mid.1 - center.1) < 0.0 {
            dir = (-dir.0, -dir.1);
        }
        let reach = ROTATE_HANDLE_OFFSET_PX / zoom.max(1e-6);
        (top_mid.0 + dir.0 * reach, top_mid.1 + dir.1 * reach)
    }

    /// The whole-layer transform frame, or `None` when there is nothing to show one for.
    ///
    /// A selected vector item takes the frame over: its own corners are drawn and hit-tested
    /// in place of the layer's, so both cannot be on screen at once. Clicking off the item
    /// drops the selection and hands the frame back to the layer.
    pub fn transform_handles(&self) -> Option<TransformHandles> {
        if !self.transform_active || self.selected_vector_item().is_some() {
            return None;
        }
        let index = self.active_layer;
        let layer = self.layers.get(index)?;
        if layer.content.is_text() {
            return None;
        }
        let raw_bounds = layer.content_bounds()?;
        let pivot = bounds_center(raw_bounds);
        let t = layer.transform.unwrap_or_default();
        let corners = t.transformed_corners(pivot, raw_bounds);
        let rotate_handle = Self::rotate_handle_position(corners, self.camera.zoom);
        Some((index, corners, rotate_handle))
    }

    fn transform_handle_at(&self, doc_x: f32, doc_y: f32) -> Option<TransformDrag> {
        let index = self.active_layer;
        let layer = self.layers.get(index)?;
        if layer.content.is_text() {
            return None;
        }
        let raw_bounds = layer.content_bounds()?;
        let pivot = bounds_center(raw_bounds);
        let t = layer.transform.unwrap_or_default();
        let corners = t.transformed_corners(pivot, raw_bounds);
        let zoom = self.camera.zoom.max(1e-6);
        let hit_r = HANDLE_HIT_RADIUS_PX / zoom;
        let point = (doc_x, doc_y);
        // Scale and rotate belong to whatever the frame is around, and while an item is
        // selected that is the item — so only the Move quad answers here, which is what still
        // lets a click inside the box drop the item selection and take the layer.
        if self.selected_vector_item().is_none() {
            for (corner, handle) in corners.iter().zip(TransformHandle::CORNERS) {
                if point_dist(*corner, point) <= hit_r {
                    return Some(TransformDrag::single(
                        index, handle, pivot, raw_bounds, t, point,
                    ));
                }
            }
            let rotate_handle = Self::rotate_handle_position(corners, zoom);
            if point_dist(rotate_handle, point) <= hit_r {
                return Some(TransformDrag::single(
                    index,
                    TransformHandle::Rotate,
                    pivot,
                    raw_bounds,
                    t,
                    point,
                ));
            }
        }
        if point_in_quad(point, corners) {
            return Some(TransformDrag::single(
                index,
                TransformHandle::Move,
                pivot,
                raw_bounds,
                t,
                point,
            ));
        }
        None
    }

    fn begin_transform_drag(&mut self, doc_x: f32, doc_y: f32) -> bool {
        self.transform_drag = self.transform_handle_at(doc_x, doc_y);
        self.transform_drag.is_some()
    }

    fn retarget_transform(&mut self, doc_x: f32, doc_y: f32) -> bool {
        if self.pick_layer_for_move(doc_x, doc_y).is_none() {
            self.note_locked_pick_for_move(doc_x, doc_y);
            return false;
        }
        self.begin_transform_drag(doc_x, doc_y);
        true
    }

    /// The transform box is `content_bounds()` — tight to what the layer actually shows, mask
    /// included. Inside that box a click always keeps the active layer and may start a move
    /// drag even on transparent pixels; outside it the stack is offered the click first.
    ///
    /// A vector item under the cursor outranks the whole-layer Move handle — the layer is
    /// the item, so a click on it starts an item drag, and the corner and rotate handles
    /// (checked first) are still how the whole layer is scaled or turned.
    fn transform_pointer_down(&mut self, doc_x: f32, doc_y: f32) {
        let handle = self.transform_handle_at(doc_x, doc_y);
        if let Some(drag) = handle
            .as_ref()
            .filter(|d| d.handle != TransformHandle::Move)
            .cloned()
        {
            self.clear_vector_selection();
            self.transform_drag = Some(drag);
            return;
        }
        if self.begin_vector_item_drag(doc_x, doc_y) {
            return;
        }
        self.clear_vector_selection();
        if let Some(drag) = handle {
            if self
                .layers
                .get(self.active_layer)
                .is_some_and(|layer| layer.visible)
            {
                self.transform_drag = Some(drag);
                return;
            }
        }
        if self.retarget_transform(doc_x, doc_y) {
            return;
        }
        if self.tool == Tool::Move {
            self.note_locked_pick_for_move(doc_x, doc_y);
            return;
        }
        self.exit_transform();
    }

    pub(crate) fn update_transform_drag(&mut self, doc_x: f32, doc_y: f32) {
        let Some(drag) = self.transform_drag.clone() else {
            return;
        };
        let (doc_x, doc_y) = match drag.handle {
            TransformHandle::Move | TransformHandle::Rotate => (doc_x, doc_y),
            _ => self.snap_doc_point((doc_x, doc_y)),
        };
        match drag.handle {
            TransformHandle::Move => {
                let dx = doc_x - drag.start_pointer.0;
                let dy = doc_y - drag.start_pointer.1;
                let union = union_aabb_after_delta(&drag.targets, dx, dy);
                let (snap_x, snap_y) = self.snap_box_offset(union);
                for target in &drag.targets {
                    let mut next = target.start_transform;
                    next.offset_x += dx + snap_x;
                    next.offset_y += dy + snap_y;
                    let next = next.clamped();
                    if let Some(layer) = self.layers.get_mut(target.layer_index) {
                        layer.transform = Some(next);
                    }
                }
            }
            TransformHandle::Rotate => {
                let target = drag.primary();
                let mut next = target.start_transform;
                let center = drag.center();
                let start_angle = angle_from(center, drag.start_pointer);
                let now_angle = angle_from(center, (doc_x, doc_y));
                next.rotation = target.start_transform.rotation + (now_angle - start_angle);
                let next = next.clamped();
                if let Some(layer) = self.layers.get_mut(target.layer_index) {
                    layer.transform = Some(next);
                }
            }
            corner => {
                let target = drag.primary();
                let Some(signs) = corner.corner_signs() else {
                    return;
                };
                let half = (
                    (target.raw_bounds.2 - target.raw_bounds.0) * 0.5,
                    (target.raw_bounds.3 - target.raw_bounds.1) * 0.5,
                );
                let reach = target
                    .start_transform
                    .to_local(drag.center(), (doc_x, doc_y));
                let (scale_x, scale_y) = corner_scale(half, signs, reach, !self.shift_held);
                let mut next = target.start_transform;
                next.scale_x = scale_x;
                next.scale_y = scale_y;
                let next = next.clamped();
                if let Some(layer) = self.layers.get_mut(target.layer_index) {
                    layer.transform = Some(next);
                }
            }
        }
    }

    pub fn duplicate_layer(&mut self, index: usize) -> bool {
        if index >= self.layers.len() {
            return false;
        }
        self.commit_text();
        self.record_stack_history();
        let Some(source) = self.layers.get(index).cloned() else {
            return false;
        };
        let base_name = source.name.clone();
        let mut copy = source;
        copy.id = uuid::Uuid::new_v4().to_string();
        copy.name = crate::names::duplicate_layer_name(&base_name);
        self.layers.insert(index + 1, copy);
        self.active_layer = index + 1;
        true
    }

    pub fn move_layer_up(&mut self, index: usize) -> bool {
        self.move_layer_by(index, 1)
    }

    pub fn move_layer_down(&mut self, index: usize) -> bool {
        self.move_layer_by(index, -1)
    }

    /// Move a layer to an arbitrary position in the stack, the drag-reorder counterpart to the
    /// single-step `move_layer_up` / `move_layer_down`. `to` is where the layer ends up in the
    /// finished stack, not an insertion point measured against the old one.
    ///
    /// Paper is pinned: it cannot be dragged, and nothing can be dropped beneath it. It is the
    /// board's backing sheet rather than a layer in the composition, and a stack with paint
    /// hidden under it would look like the paint had vanished.
    pub fn move_layer(&mut self, from: usize, to: usize) -> bool {
        let count = self.layers.len();
        if from >= count || to >= count || from == to {
            return false;
        }
        if let Some(paper) = self.layers.iter().position(Layer::is_paper) {
            if from == paper || to <= paper {
                return false;
            }
        }
        self.commit_text();
        self.record_stack_history();
        let layer = self.layers.remove(from);
        self.layers.insert(to, layer);
        let remap = |i: usize| {
            if i == from {
                to
            } else if from < to && i > from && i <= to {
                i - 1
            } else if from > to && i >= to && i < from {
                i + 1
            } else {
                i
            }
        };
        self.remap_layer_indices(remap);
        true
    }

    /// The same move stated in panel rows. The layers panel draws the stack top-first while
    /// the document stores it bottom-first, so the two orders are mirror images — the engine
    /// owns that flip so the shell can hand over the row it dragged and the row it dropped on
    /// without ever computing a stack index.
    pub fn move_layer_row(&mut self, from_row: usize, to_row: usize) -> bool {
        let count = self.layers.len();
        if from_row >= count || to_row >= count {
            return false;
        }
        self.move_layer(count - 1 - from_row, count - 1 - to_row)
    }

    /// Rename a layer, or refuse.
    ///
    /// Paper is name-matched (`Layer::is_paper`), so its name is load-bearing: merge-down,
    /// click-to-pick and the Filters menu all key off it. Renaming Paper would quietly break
    /// all three, and renaming *another* layer to `Paper` would quietly turn it into one — so
    /// both directions are refused. An all-whitespace name is refused too, since a row with no
    /// label is unusable.
    pub fn set_layer_name(&mut self, index: usize, name: &str) -> bool {
        let trimmed = name.trim();
        if trimmed.is_empty() || trimmed == crate::names::PAPER {
            return false;
        }
        let Some(layer) = self.layers.get_mut(index) else {
            return false;
        };
        if layer.is_paper() || layer.name == trimmed {
            return false;
        }
        layer.name = trimmed.to_string();
        true
    }

    pub fn set_layer_locked(&mut self, index: usize, locked: bool) -> bool {
        let Some(layer) = self.layers.get_mut(index) else {
            return false;
        };
        if layer.locked == locked {
            return false;
        }
        layer.locked = locked;
        if locked && self.active_layer == index {
            self.exit_transform();
        }
        true
    }

    pub fn layer_locked(&self, index: usize) -> bool {
        self.layers.get(index).is_some_and(|l| l.locked)
    }

    /// Every index the document keeps into `layers`, moved through one mapping. Any stack
    /// mutation has to run all of them or something ends up pointing at the wrong layer — a
    /// stale text-edit index is what made hiding a text layer fail before.
    fn remap_layer_indices(&mut self, remap: impl Fn(usize) -> usize + Copy) {
        self.active_layer = remap(self.active_layer);
        self.hover_layer = self.hover_layer.map(remap);
        if let Some(drag) = &mut self.transform_drag {
            for target in &mut drag.targets {
                target.layer_index = remap(target.layer_index);
            }
        }
        self.layer_selection = self
            .layer_selection
            .iter()
            .map(|&index| remap(index))
            .collect();
        if let Some(pick) = &mut self.selected_vector {
            pick.layer = remap(pick.layer);
        }
        if let Some(edit) = &mut self.text_edit {
            edit.layer = remap(edit.layer);
        }
    }

    fn move_layer_by(&mut self, index: usize, delta: isize) -> bool {
        if index >= self.layers.len() {
            return false;
        }
        if self.layers[index].is_paper() {
            return false;
        }
        let other = match index.checked_add_signed(delta) {
            Some(other) if other < self.layers.len() => other,
            _ => return false,
        };
        if delta < 0 && self.layers[other].is_paper() {
            return false;
        }
        self.commit_text();
        self.record_stack_history();
        self.layers.swap(index, other);
        self.remap_layer_indices(|i| {
            if i == index {
                other
            } else if i == other {
                index
            } else {
                i
            }
        });
        true
    }

    pub fn resize(&mut self, new_width: u32, new_height: u32) {
        let new_width = new_width.clamp(MIN_CANVAS_SIDE, MAX_CANVAS_SIDE);
        let new_height = new_height.clamp(MIN_CANVAS_SIDE, MAX_CANVAS_SIDE);
        let (old_width, old_height) = (self.width, self.height);
        if new_width == old_width && new_height == old_height {
            return;
        }
        self.commit_text();
        self.record_stack_history();
        for layer in &mut self.layers {
            layer.resize_mask(old_width, old_height, new_width, new_height);
            let is_paper = layer.is_paper();
            let Some(tiles) = layer.tiles_mut() else {
                continue;
            };
            tiles.set_size(new_width, new_height);
            if !is_paper {
                continue;
            }
            if new_width > old_width {
                tiles.fill_uniform(
                    DocRect::new(
                        old_width as i32,
                        0,
                        new_width as i32 - 1,
                        new_height as i32 - 1,
                    ),
                    PAPER_WHITE,
                );
            }
            if new_height > old_height {
                tiles.fill_uniform(
                    DocRect::new(
                        0,
                        old_height as i32,
                        old_width as i32 - 1,
                        new_height as i32 - 1,
                    ),
                    PAPER_WHITE,
                );
            }
        }
        self.width = new_width;
        self.height = new_height;
        self.fit_to_view();
    }

    pub fn resize_viewport(&mut self, width: f32, height: f32, dpr: f32) {
        self.camera.viewport_width = width.max(1.0);
        self.camera.viewport_height = height.max(1.0);
        self.camera.dpr = dpr.max(1.0);
        self.camera
            .clamp_to_board(self.width as f32, self.height as f32);
    }

    pub fn fit_to_view(&mut self) {
        self.camera.fit(self.width as f32, self.height as f32);
    }

    pub fn pointer_down(&mut self, screen_x: f32, screen_y: f32) {
        let (dx, dy) = self.camera.to_doc(screen_x, screen_y);
        self.pointer_hover = Some((dx, dy));
        if self.transform_active {
            self.transform_pointer_down(dx, dy);
            return;
        }
        if self.tool == Tool::Move {
            self.commit_text();
            // A guide sits on top of everything it crosses, so it is what the Move tool grabs
            // first. Every other tool draws straight through one — a rule you cannot paint
            // across would be worse than no rule at all.
            if self.begin_guide_drag(screen_x, screen_y) {
                return;
            }
            self.begin_move_at(dx, dy);
            return;
        }
        // The one place a refusal is worth interrupting for: the user has just asked for
        // something and nothing happened. Every guard further down is the same rule again,
        // reached by callers that are not a board press. Text is asked first because its
        // branch never reaches the common one, and an open session is committed either way —
        // being refused a press is still leaving the text behind.
        if self.tool == Tool::Text {
            if self.press_blocked(Tool::Text) {
                self.commit_text();
                return;
            }
            self.begin_text_at(dx, dy);
            return;
        }
        self.commit_text();
        if self.press_blocked(self.tool) {
            return;
        }
        if self.tool == Tool::Fill {
            self.commit_fill(dx, dy);
            return;
        }
        if self.tool == Tool::MagicWand {
            self.commit_magic_wand(dx, dy);
            return;
        }
        if self.tool == Tool::SelectColor {
            self.commit_select_color(dx, dy);
            return;
        }
        if self.tool == Tool::Eyedropper {
            let _ = self.pick_color(dx, dy);
            return;
        }
        if self.tool.is_stroke() {
            self.begin_stroke();
            self.push_stroke_point(dx, dy);
            self.blur_pending_stamps();
        } else {
            let shape_tool = match self.tool {
                Tool::SelectRect => Tool::Rect,
                Tool::SelectEllipse => Tool::Ellipse,
                t => t,
            };
            self.shape_drag = Some(Shape {
                tool: shape_tool,
                start: (dx, dy),
                end: (dx, dy),
                half_width: self.brush_size * 0.5,
                fill: self.fill,
                stroke: self.stroke,
            });
        }
    }

    /// Returns whether this move changed anything the renderer caches — tile pixels, vector
    /// items, or a layer transform. `false` means the only thing that moved is the live overlay
    /// (a pen stroke's preview segments, a shape drag's SDF uniform), which is drawn on top of
    /// the cached content and costs one instance-buffer write.
    ///
    /// That distinction is the whole difference between a brush stroke that recomposites the
    /// visible stack at display rate and one that does not: a pen lays no pixels down until
    /// `pointer_up`, so every frame in between is an overlay frame.
    pub fn pointer_move(&mut self, screen_x: f32, screen_y: f32) -> bool {
        let (dx, dy) = self.camera.to_doc(screen_x, screen_y);
        // Keeps the brush cursor under the pointer mid-stroke without the shell having to
        // send the position twice.
        self.pointer_hover = Some((dx, dy));
        if self.transform_active {
            if !self.update_vector_item_drag(dx, dy) {
                self.update_transform_drag(dx, dy);
            }
            return true;
        }
        if self.tool == Tool::Move {
            // A guide is redrawn from scratch every frame, so moving one invalidates no cache —
            // it is the cheapest kind of overlay frame there is.
            if self.update_guide_drag(screen_x, screen_y) {
                return false;
            }
            self.update_move_drag(dx, dy);
            return true;
        }
        if self.tool == Tool::Text {
            return self.text_pointer_move(dx, dy);
        }
        if self.tool.is_stroke() && self.stroke_active {
            self.push_stroke_point(dx, dy);
            return self.blur_pending_stamps();
        }
        if let Some(shape) = &mut self.shape_drag {
            shape.end = (dx, dy);
            shape.half_width = self.brush_size * 0.5;
            shape.fill = self.fill;
            shape.stroke = self.stroke;
        }
        false
    }

    pub fn pointer_up(&mut self, screen_x: f32, screen_y: f32) {
        let (dx, dy) = self.camera.to_doc(screen_x, screen_y);
        if self.transform_active {
            self.commit_vector_drag_history();
            self.commit_transform_drag_history();
            return;
        }
        if self.tool == Tool::Move {
            if self.end_guide_drag() {
                return;
            }
            self.end_move_drag();
            return;
        }
        if self.tool == Tool::Text {
            self.text_pointer_up();
            return;
        }
        if self.tool.is_stroke() {
            self.push_stroke_point(dx, dy);
            if self.tool == Tool::SelectLasso {
                self.commit_lasso_selection();
            } else {
                self.commit_stroke();
            }
        } else if let Some(shape) = &mut self.shape_drag {
            shape.end = (dx, dy);
            shape.half_width = self.brush_size * 0.5;
            shape.fill = self.fill;
            shape.stroke = self.stroke;
            let Some(shape) = self.preview_shape() else {
                return;
            };
            self.shape_drag = None;
            if matches!(self.tool, Tool::SelectRect | Tool::SelectEllipse) {
                self.commit_selection_shape(shape);
            } else {
                self.commit_shape(shape);
            }
        }
    }

    fn begin_stroke(&mut self) {
        self.stroke_active = true;
        self.stroke_points.clear();
        self.stroke_generation = self.stroke_generation.wrapping_add(1);
        self.stroke_straight_anchor = None;
        self.stroke_before.clear();
        self.blur_stamped = 0;
        self.blur_painted = false;
    }

    /// Identifies the stroke `stroke_points` currently belongs to. See the field's own note —
    /// this exists so the renderer can append to GPU coverage it has already accumulated
    /// instead of rasterizing the whole stroke again every frame.
    pub fn stroke_generation(&self) -> u64 {
        self.stroke_generation
    }

    fn push_stroke_point(&mut self, x: f32, y: f32) {
        if self.shift_held && matches!(self.tool, Tool::Pen | Tool::Eraser) {
            let anchor = *self
                .stroke_straight_anchor
                .get_or_insert(self.stroke_points.len().saturating_sub(1));
            if self.stroke_points.len() > anchor + 1 {
                self.stroke_generation = self.stroke_generation.wrapping_add(1);
            }
            self.stroke_points.truncate(anchor + 1);
            if let Some(anchor_pt) = self.stroke_points.get(anchor) {
                let dx = x - anchor_pt.x;
                let dy = y - anchor_pt.y;
                if dx * dx + dy * dy < MIN_STROKE_POINT_DISTANCE * MIN_STROKE_POINT_DISTANCE {
                    return;
                }
            }
            self.stroke_points.push(StrokePoint { x, y });
            return;
        }
        self.stroke_straight_anchor = None;
        if let Some(last) = self.stroke_points.last() {
            let dx = x - last.x;
            let dy = y - last.y;
            if dx * dx + dy * dy < MIN_STROKE_POINT_DISTANCE * MIN_STROKE_POINT_DISTANCE {
                return;
            }
        }
        self.stroke_points.push(StrokePoint { x, y });
    }

    pub fn set_blur_strength(&mut self, strength: f32) {
        self.blur_strength = strength.clamp(BLUR_STRENGTH_MIN, BLUR_STRENGTH_MAX);
    }

    pub fn set_brush(&mut self, brush: Brush) {
        self.brush = brush;
    }

    /// The profile the *current* stroke lays ink down with. The pen carries a whole brush; the
    /// eraser carries only an edge, since grain and flow describe ink being put down and it is
    /// taking ink away. Everything else draws hard-edged.
    pub fn active_brush_profile(&self) -> BrushProfile {
        match self.tool {
            Tool::Pen => self.brush.profile(),
            Tool::Eraser => BrushProfile {
                hardness: self.eraser_hardness,
                ..BrushProfile::HARD
            },
            _ => BrushProfile::HARD,
        }
    }

    pub fn set_eraser_hardness(&mut self, hardness: f32) {
        self.eraser_hardness = hardness.clamp(ERASER_HARDNESS_MIN, ERASER_HARDNESS_MAX);
    }

    /// The ink a stroke actually lands, with the brush's flow folded into the alpha. The one
    /// place that happens, so the GPU preview and the committed pixels cannot disagree about
    /// how translucent a marker is.
    pub fn stroke_ink(&self) -> [u8; 4] {
        let mut ink = self.ink_rgba();
        let flow = self.active_brush_profile().flow;
        ink[3] = ((ink[3] as f32) * flow).round().clamp(0.0, 255.0) as u8;
        ink
    }

    /// Whether the in-progress stroke is ink going onto a raster layer — the case that has to
    /// preview through the coverage pass so overlapping segments do not compound. A vector pen
    /// stroke and a lasso are outlines, not ink, and draw straight.
    pub fn previews_brush_stroke(&self) -> bool {
        if self.stroke_points.is_empty() {
            return false;
        }
        match self.tool {
            Tool::Pen => !self.effective_vector_mode(),
            Tool::Eraser => true,
            _ => false,
        }
    }

    /// Tolerance is one knob for the bucket, the wand and Select Color. Only the last of the
    /// three re-runs on it: it is Color Range's Fuzziness, where the point is watching the
    /// selection open up as you drag. The other two apply it to their next click, which is what
    /// a flood from a pixel you are no longer pointing at would have to guess.
    pub fn set_tolerance(&mut self, tolerance: u8) {
        let next = tolerance.clamp(TOLERANCE_MIN, TOLERANCE_MAX);
        if self.tolerance == next {
            return;
        }
        self.tolerance = next;
        self.reselect_color();
    }

    /// The match colour, pushed from the shell's tertiary swatch. Ringing that swatch while
    /// Select Color is in hand re-runs the selection, which is what makes it *the* match colour
    /// rather than a note about one — Photoshop's Color Range updates as you re-sample too.
    pub fn set_select_color(&mut self, color: [u8; 4]) {
        if self.select_color == color {
            return;
        }
        self.select_color = color;
        self.reselect_color();
    }

    pub fn select_color(&self) -> [u8; 4] {
        self.select_color
    }

    pub fn set_eyedropper_radius(&mut self, radius: u32) {
        self.eyedropper_radius = radius.clamp(EYEDROPPER_RADIUS_MIN, EYEDROPPER_RADIUS_MAX);
    }

    /// Blur whatever part of the stroke has not been blurred yet, straight into the layer.
    ///
    /// Unlike every other stamp tool this runs *during* the drag rather than at pointer-up:
    /// there is nothing to preview on the GPU, so committing as it goes is what makes the
    /// brush visible while you use it. `blur_stamped` is the boundary — `stroke_stamps` only
    /// ever appends as points arrive, so a stamp already applied is never re-applied and the
    /// spacing phase along the polyline stays the same as the pen's.
    ///
    /// The tiles the whole stroke touches accumulate into one `stroke_before` snapshot, so a
    /// blur is still a single undo step no matter how many pointer events it spanned. Only
    /// tiles not already in the snapshot are captured — re-snapshotting a tile the stroke has
    /// already blurred would record the blurred state as the "before".
    fn blur_pending_stamps(&mut self) -> bool {
        if self.tool != Tool::Blur || !self.stroke_active {
            return false;
        }
        if self.tool_blocked(Tool::Blur) {
            return false;
        }
        let radius = self.effective_brush_size() * 0.5;
        let strength = self.blur_strength;
        if strength <= 0.0 {
            return false;
        }
        let all = stroke_stamps(&self.stroke_points, radius);
        let Some(fresh) = all.get(self.blur_stamped..).filter(|s| !s.is_empty()) else {
            return false;
        };
        let stamps: Vec<(f32, f32)> = fresh.iter().map(|p| (p.x, p.y)).collect();
        self.blur_stamped = all.len();

        let Some(span) = stamps_bounds(fresh, radius).and_then(|r| r.intersect(self.bounds()))
        else {
            return false;
        };
        let mut touched = TileSet::default();
        tiles_covering(span, &mut touched);

        let active = self.active_layer;
        let Some(grid) = self.layers.get(active).and_then(Layer::tiles) else {
            return false;
        };
        let unseen: Vec<TileCoord> = touched
            .into_iter()
            .filter(|c| grid.tile_in_bounds(*c) && !self.stroke_before.contains_key(c))
            .collect();
        self.stroke_before.extend(grid.snapshot_tiles(&unseen));

        let selection = self.selection.clone();
        let mut painted_now = false;
        if let Some(tiles) = self.layers.get_mut(active).and_then(|l| l.tiles_mut()) {
            let touched =
                crate::blur::blur_stamps(tiles, &stamps, radius, strength, selection.as_ref());
            painted_now = touched > 0;
            self.blur_painted |= painted_now;
        }
        painted_now
    }

    /// Close out a blur stroke: the pixels are already committed, so all that is left is
    /// turning the accumulated snapshot into one history entry.
    fn commit_blur_stroke(&mut self) {
        self.stroke_active = false;
        self.stroke_points.clear();
        self.blur_stamped = 0;
        let painted = std::mem::take(&mut self.blur_painted);
        let before = std::mem::take(&mut self.stroke_before);
        if !painted || before.is_empty() {
            return;
        }
        let Some(layer_id) = self.layers.get(self.active_layer).map(|l| l.id.clone()) else {
            return;
        };
        self.history
            .push_layer_tiles(layer_id, before, Some(self.active_layer));
    }

    fn commit_stroke(&mut self) {
        if !self.stroke_active {
            return;
        }
        if self.tool_blocked(self.tool) {
            self.stroke_active = false;
            self.stroke_points.clear();
            return;
        }
        if self.tool == Tool::Blur {
            self.blur_pending_stamps();
            self.commit_blur_stroke();
            return;
        }
        self.stroke_active = false;
        let points = std::mem::take(&mut self.stroke_points);
        if points.is_empty() {
            return;
        }
        if self.effective_vector_mode() && self.tool == Tool::Pen {
            let pts: Vec<(f32, f32)> = points.iter().map(|p| (p.x, p.y)).collect();
            if let Some(item) = vector::item_from_points(&pts, self.ink_rgba(), self.brush_size) {
                self.push_vector_item(item);
            }
            return;
        }
        let erasing = self.tool == Tool::Eraser;
        let profile = self.active_brush_profile();
        let ink = if erasing {
            [0, 0, 0, ALPHA_OPAQUE]
        } else {
            self.stroke_ink()
        };
        let active = self.active_layer;

        // The stroke was aimed at document coordinates; a layer holds its pixels in its own
        // grid, and the renderer maps that grid into the document through the layer's transform.
        // Everything from here down is in *grid* space — the points, the radius, and the area
        // the coverage may cover. For an untransformed layer the two are the same thing; for a
        // moved or scaled one this is the difference between the stroke staying where it was
        // drawn and jumping the moment the GPU preview hands over to the commit.
        let Some(layer) = self.layers.get(active) else {
            return;
        };
        let layer_id = layer.id.clone();
        let radius = layer.doc_length_to_grid(self.effective_brush_size() * 0.5);
        let points: Vec<(f32, f32)> = points
            .iter()
            .map(|p| layer.doc_point_to_grid((p.x, p.y)))
            .collect();
        let Some(grid) = layer.tiles() else {
            return;
        };
        // Bounded by what the grid may hold, not by the paper: a pasted image reaches past the
        // document, and a stroke on the part hanging off it is still a stroke on the layer.
        let area = grid.extent();

        let mut coverage = CoverageGrid::new(area);
        match points.len() {
            0 => return,
            1 => {
                coverage.add_segment(points[0], points[0], radius, &profile);
            }
            _ => {
                for pair in points.windows(2) {
                    coverage.add_segment(pair[0], pair[1], radius, &profile);
                }
            }
        }
        if coverage.is_empty() {
            return;
        }

        let touched: Vec<TileCoord> = coverage
            .tile_coords()
            .filter(|c| grid.tile_in_bounds(*c))
            .collect();
        if touched.is_empty() {
            return;
        }
        self.stroke_before = grid.snapshot_tiles(&touched);

        let mut painted = false;
        if let Some(tiles) = self.layers.get_mut(active).and_then(|l| l.tiles_mut()) {
            painted = coverage.paint_into(tiles, ink, erasing) > 0;
        }

        if !painted {
            self.stroke_before.clear();
            return;
        }
        let before = std::mem::take(&mut self.stroke_before);
        self.history
            .push_layer_tiles(layer_id, before, Some(active));
    }

    fn commit_shape(&mut self, shape: Shape) {
        if self.tool_blocked(self.tool) {
            return;
        }
        let (fill_color, stroke_color) = self.shape_paint(shape.tool);
        if self.effective_vector_mode() {
            if let Some(item) = vector::item_from_shape(shape, fill_color, stroke_color) {
                self.push_vector_item(item);
                return;
            }
        }
        let (x0, y0, x1, y1) = shape.bounds();
        let Some(rect) = DocRect::from_floats(x0, y0, x1, y1).intersect(self.bounds()) else {
            return;
        };

        let mut coords = TileSet::default();
        tiles_covering(rect, &mut coords);
        let coords: Vec<TileCoord> = coords.into_iter().collect();

        let active = self.active_layer;
        let Some(layer) = self.layers.get(active) else {
            return;
        };
        let layer_id = layer.id.clone();
        let Some(grid) = layer.tiles() else {
            return;
        };
        let before = grid.snapshot_tiles(&coords);

        let mut painted = false;
        if let Some(tiles) = self.layers.get_mut(active).and_then(|l| l.tiles_mut()) {
            // Fill first, stroke over it — the same order the shader composites in, so a
            // translucent border reads the same on the board as it does once committed.
            let touched = tiles.paint_rect(rect, |px, py, dst| {
                let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
                let parts = [
                    ink_sample(shape.fill_distance(x, y), fill_color),
                    ink_sample(shape.stroke_distance(x, y), stroke_color),
                ];
                let mut out = dst;
                let mut inked = false;
                for src in parts.into_iter().flatten() {
                    out = blend_over(out, src);
                    inked = true;
                }
                inked.then_some(out)
            });
            painted = touched > 0;
        }
        if !painted {
            return;
        }
        self.history
            .push_layer_tiles(layer_id, before, Some(active));
    }

    fn commit_selection_shape(&mut self, shape: Shape) {
        if self.tool_blocked(self.tool) {
            return;
        }
        let geom = match shape.tool {
            Tool::Rect => SelectionShape::Rect {
                start: shape.start,
                end: shape.end,
            },
            Tool::Ellipse => SelectionShape::Ellipse {
                start: shape.start,
                end: shape.end,
            },
            _ => return,
        };
        self.commit_geometry_selection(geom);
    }

    /// A marquee or a lasso, kept only where the active layer has ink.
    ///
    /// Two answers, not one. A layer with **nothing painted** has nothing for the region to hug,
    /// so the geometry stands as drawn — that is the only thing a marquee on a fresh layer could
    /// sensibly mean, and it is what Photoshop does everywhere. A layer that *does* have ink and
    /// simply none inside the region leaves the selection alone, the same as a wand that hit
    /// nothing: the gesture asked for artwork and found none.
    fn commit_geometry_selection(&mut self, geom: SelectionShape) {
        let doc_bounds = self.bounds();
        let Some(layer) = self.layers.get(self.active_layer) else {
            return;
        };
        if crate::select_sample::painted_scope(layer, doc_bounds).is_none() {
            self.selection = Some(Selection { shape: geom });
            return;
        }
        let Some(mask) = crate::select_sample::selection_from_geometry(layer, doc_bounds, &geom)
        else {
            return;
        };
        self.selection = Some(Selection {
            shape: SelectionShape::Mask(mask),
        });
    }

    /// Select by color: flood from the clicked pixel of the active layer and keep what the
    /// walk reached.
    ///
    /// Scope is the **document**, not the layer's painted box. Alpha counts toward the
    /// tolerance, so the empty space around a drawing is a colour like any other and has to be
    /// floodable — clicking beside a sketch selects the space beside it, which is how you get
    /// at a background to fill or delete it. Scoping the walk to the ink made that click a
    /// silent no-op and cut every flood off at the edge of the artwork it started on. Colour
    /// range is the one that stays scoped to the ink, because it walks every pixel rather than
    /// following one blob.
    ///
    /// The old selection does not clip the new one either: the wand *replaces* the selection,
    /// so letting the old one bound the walk would make a second click inside a previous wand
    /// result unable to grow past it. That is the one place the wand deliberately diverges from
    /// the bucket, which paints *into* the selection and so has to respect it.
    ///
    /// Reading the active layer (not the composite) is what makes the wand answer about the
    /// thing being edited: clicking a sketch's white background selects the background of that
    /// layer, not of the Paper showing through it — and outside that layer's ink the sample
    /// answers transparent, which is exactly what is there.
    fn commit_magic_wand(&mut self, doc_x: f32, doc_y: f32) {
        if self.tool_blocked(Tool::MagicWand) {
            return;
        }
        let x = doc_x.floor() as i32;
        let y = doc_y.floor() as i32;
        let scope = self.bounds();
        if !scope.contains(x, y) {
            return;
        }
        let Some(layer) = self.layers.get(self.active_layer) else {
            return;
        };
        let Some(sample) = crate::select_sample::LayerSelectSample::new(layer, scope) else {
            return;
        };
        let tolerance = self.tolerance;
        self.selection =
            crate::fill::flood_region_pixels(scope, x, y, tolerance, |px, py| sample.pixel(px, py))
                .map(|mask| Selection {
                    shape: SelectionShape::Mask(mask),
                });
    }

    /// Clicking with Select Color is the eyedropper half of Photoshop's Color Range: it samples
    /// the pixel into the match swatch and then selects everything that matches it. Changing the
    /// swatch or the tolerance afterwards re-runs against the same layer — see
    /// `Document::reselect_color`.
    fn commit_select_color(&mut self, doc_x: f32, doc_y: f32) {
        if self.tool_blocked(Tool::SelectColor) {
            return;
        }
        let x = doc_x.floor() as i32;
        let y = doc_y.floor() as i32;
        let doc_bounds = self.bounds();
        let Some(layer) = self.layers.get(self.active_layer) else {
            return;
        };
        let Some(sample) = crate::select_sample::LayerSelectSample::new(layer, doc_bounds) else {
            return;
        };
        if !sample.scope.contains(x, y) {
            return;
        }
        self.select_color = sample.pixel(x, y);
        self.apply_color_range();
    }

    fn commit_lasso_selection(&mut self) {
        self.stroke_active = false;
        if self.tool_blocked(Tool::SelectLasso) {
            self.stroke_points.clear();
            return;
        }
        let points: Vec<(f32, f32)> = std::mem::take(&mut self.stroke_points)
            .into_iter()
            .map(|p| (p.x, p.y))
            .collect();
        let points = crate::select_sample::simplify_lasso_points(points);
        if points.len() < 3 {
            return;
        }
        self.commit_geometry_selection(SelectionShape::Lasso { points });
    }

    /// The one place the ink color changes, so a text layer being typed into recolors with
    /// it instead of the shell having to know that text is special.
    pub fn set_color(&mut self, color: [u8; 4]) {
        self.color = color;
        self.apply_ink_to_text();
    }

    pub fn set_ink_opacity(&mut self, opacity: f32) {
        self.ink_opacity = opacity.clamp(INK_OPACITY_MIN, INK_OPACITY_MAX);
    }

    pub fn ink_rgba(&self) -> [u8; 4] {
        glazed(self.color, self.ink_opacity)
    }

    /// The outline color a shape lands, glazed by the same ink opacity the fill is — one
    /// slider governs how translucent the whole shape is, as it does in Figma.
    pub fn shape_stroke_rgba(&self) -> [u8; 4] {
        glazed(self.stroke_color, self.ink_opacity)
    }

    pub fn shape_fill_rgba(&self) -> [u8; 4] {
        glazed(self.shape_fill_color, self.ink_opacity)
    }

    /// The two colors a shape commits with: `(fill, outline)`. An area shape reads its own
    /// two swatches — primary outlines it, secondary fills it — rather than the ink, so the
    /// same rectangle is drawn the same way whichever swatch the picker happens to be pointed
    /// at. Line and Arrow have no interior and no second half: they are the ink, as they
    /// always were. Resolving that here is what lets everything downstream — the rasterizer,
    /// the SVG writer, the shader — read two colors with no tool test.
    pub fn shape_paint(&self, tool: Tool) -> ([u8; 4], [u8; 4]) {
        if !tool.takes_fill() {
            let ink = self.ink_rgba();
            return (ink, ink);
        }
        (self.shape_fill_rgba(), self.shape_stroke_rgba())
    }

    fn commit_fill(&mut self, doc_x: f32, doc_y: f32) {
        if self.tool_blocked(Tool::Fill) {
            return;
        }
        let x = doc_x.floor() as i32;
        let y = doc_y.floor() as i32;
        let scope = match &self.selection {
            Some(sel) => sel.bounds(),
            None => self.bounds(),
        };
        let Some(scope) = scope.intersect(self.bounds()) else {
            return;
        };
        if !scope.contains(x, y) {
            return;
        }
        if let Some(sel) = &self.selection {
            if !sel.contains(x as f32 + 0.5, y as f32 + 0.5) {
                return;
            }
        }
        let active = self.active_layer;
        let Some(layer) = self.layers.get(active) else {
            return;
        };
        let layer_id = layer.id.clone();
        let Some(grid) = layer.tiles() else {
            return;
        };
        let mut coords = TileSet::default();
        tiles_covering(scope, &mut coords);
        let coords: Vec<TileCoord> = coords
            .into_iter()
            .filter(|c| grid.tile_in_bounds(*c))
            .collect();
        if coords.is_empty() {
            return;
        }
        let before = grid.snapshot_tiles(&coords);
        let color = self.ink_rgba();
        let selection = self.selection.clone();
        let mut touched = 0;
        if let Some(tiles) = self.layers.get_mut(active).and_then(|l| l.tiles_mut()) {
            touched = crate::fill::flood_fill(
                tiles,
                scope,
                x,
                y,
                color,
                selection.as_ref(),
                self.tolerance,
            );
        }
        if touched == 0 {
            return;
        }
        self.history
            .push_layer_tiles(layer_id, before, Some(active));
    }

    pub fn composite_rgba(&self) -> (u32, u32, Vec<u8>) {
        let w = self.width.max(1);
        let h = self.height.max(1);
        let mut out = vec![0u8; (w as usize) * (h as usize) * 4];
        let mut layer_buf = vec![0u8; (w as usize) * (h as usize) * 4];
        for layer in &self.layers {
            if !layer.visible {
                continue;
            }
            if layer.tiles().is_none() && layer.content.item().is_none() {
                continue;
            }
            layer_buf.fill(0);
            copy_layer_into_rgba(layer, &mut layer_buf, w, h);
            apply_mask(&mut layer_buf, layer.mask());
            let lut = layer.adjustments.map(|a| a.lut());
            apply_layer_effects(&mut layer_buf, layer, lut.as_ref());
            let mode = layer.blend_mode;
            out.par_chunks_mut(EFFECT_CHUNK_BYTES)
                .zip(layer_buf.par_chunks(EFFECT_CHUNK_BYTES))
                .for_each(|(dst_block, src_block)| {
                    for (dst, src) in dst_block.chunks_exact_mut(4).zip(src_block.chunks_exact(4)) {
                        if src[3] == 0 {
                            continue;
                        }
                        let blended = blend_with_mode(
                            [dst[0], dst[1], dst[2], dst[3]],
                            [src[0], src[1], src[2], src[3]],
                            mode,
                        );
                        dst.copy_from_slice(&blended);
                    }
                });
        }
        (w, h, out)
    }

    /// One document pixel of the visible composite, as the eyedropper sees it. Vector layers
    /// are not sampled — they have no tiles, and this is the twin of what the board shows
    /// through the tile path.
    fn sampled_pixel(&self, doc_x: f32, doc_y: f32) -> Option<[u8; 4]> {
        let ix = doc_x.floor() as i32;
        let iy = doc_y.floor() as i32;
        if ix < 0 || iy < 0 || (ix as u32) >= self.width || (iy as u32) >= self.height {
            return None;
        }
        let mut acc = [0u8; 4];
        for layer in &self.layers {
            if !layer.visible || layer.tiles().is_none() {
                continue;
            }
            let src = layer_composited_pixel(layer, doc_x, doc_y, self.width, self.height);
            if src[3] == 0 {
                continue;
            }
            acc = blend_with_mode(acc, src, layer.blend_mode);
        }
        if acc[3] == 0 {
            None
        } else {
            Some(acc)
        }
    }

    /// The eyedropper's color: the mean of the disc of radius `r + 0.5` around the clicked
    /// pixel, so the default `r = 1` is the 3×3 every image editor offers.
    ///
    /// Averaged in **premultiplied** space, for the same reason `blur.rs` works there — tiles
    /// hold straight alpha, so a plain mean would pull the sample toward whatever color sits
    /// in the fully transparent pixels beside a painted edge. Weighting each sample by its own
    /// alpha is what makes picking on the boundary of a stroke return the stroke's color
    /// rather than a color that is nowhere on the board.
    ///
    /// Pixels off the paper are skipped rather than counted as transparent, so a sample near
    /// the edge is not darkened by the void outside it. A radius of 0 short-circuits to the
    /// single pixel, byte-for-byte what this returned before the average existed.
    pub fn sample_color(&self, doc_x: f32, doc_y: f32) -> Option<[u8; 4]> {
        let radius = self.eyedropper_radius as i32;
        if radius == 0 {
            return self.sampled_pixel(doc_x, doc_y);
        }
        let cx = doc_x.floor() as i32;
        let cy = doc_y.floor() as i32;
        let (mut sum_r, mut sum_g, mut sum_b, mut sum_a) = (0u32, 0u32, 0u32, 0u32);
        let mut count = 0u32;
        for y in (cy - radius)..=(cy + radius) {
            for x in (cx - radius)..=(cx + radius) {
                if x < 0 || y < 0 || (x as u32) >= self.width || (y as u32) >= self.height {
                    continue;
                }
                if !Self::eyedropper_covers(x - cx, y - cy, radius) {
                    continue;
                }
                count += 1;
                let Some(px) = self.sampled_pixel(x as f32 + 0.5, y as f32 + 0.5) else {
                    continue;
                };
                let a = px[3] as u32;
                sum_r += px[0] as u32 * a;
                sum_g += px[1] as u32 * a;
                sum_b += px[2] as u32 * a;
                sum_a += a;
            }
        }
        if count == 0 || sum_a == 0 {
            return None;
        }
        let unpremultiply = |sum: u32| ((sum + sum_a / 2) / sum_a).min(ALPHA_MAX) as u8;
        Some([
            unpremultiply(sum_r),
            unpremultiply(sum_g),
            unpremultiply(sum_b),
            ((sum_a + count / 2) / count).min(ALPHA_MAX) as u8,
        ])
    }

    fn eyedropper_covers(dx: i32, dy: i32, radius: i32) -> bool {
        let r = radius as f32 + 0.5;
        (dx * dx + dy * dy) as f32 <= r * r
    }

    /// The layers a composite has to sample, each with the box it can possibly paint into.
    /// Hoisting this out of the per-pixel loop is what keeps a whole-document flatten from
    /// scaling with the layers that are hidden, empty, or nowhere near the pixel being asked
    /// about — on a deep stack that is most of them, for most pixels.
    fn contributing_layers(&self) -> Vec<BoundedLayer<'_>> {
        self.layers
            .iter()
            .filter(|l| l.visible)
            .filter(|l| l.tiles().is_some() || l.content.item().is_some())
            .filter_map(|l| {
                let raw = l.content_bounds()?;
                let t = l.transform.unwrap_or_default();
                Some((l, t.transformed_aabb(raw)))
            })
            .collect()
    }

    fn composite_pixel_of(&self, layers: &[BoundedLayer<'_>], doc_x: f32, doc_y: f32) -> [u8; 4] {
        let mut acc = [0u8; 4];
        for (layer, bounds) in layers {
            if doc_x < bounds.0 || doc_y < bounds.1 || doc_x > bounds.2 || doc_y > bounds.3 {
                continue;
            }
            let src = layer_composited_pixel(layer, doc_x, doc_y, self.width, self.height);
            if src[3] == 0 {
                continue;
            }
            acc = blend_with_mode(acc, src, layer.blend_mode);
        }
        acc
    }

    pub fn composite_overview(&self, max_side: u32) -> (u32, u32, Vec<u8>) {
        let max_side = max_side.max(1);
        let dw = self.width.max(1);
        let dh = self.height.max(1);
        let scale = (max_side as f32 / dw as f32)
            .min(max_side as f32 / dh as f32)
            .min(1.0);
        let tw = ((dw as f32) * scale).round().max(1.0) as u32;
        let th = ((dh as f32) * scale).round().max(1.0) as u32;
        let mut rgba = vec![0u8; (tw as usize) * (th as usize) * 4];
        let contributing = self.contributing_layers();
        rgba.par_chunks_mut(4).enumerate().for_each(|(index, px)| {
            let tx = (index as u32) % tw;
            let ty = (index as u32) / tw;
            let doc_x = if tw <= 1 {
                0.0
            } else {
                tx as f32 * (dw - 1) as f32 / (tw - 1) as f32
            };
            let doc_y = if th <= 1 {
                0.0
            } else {
                ty as f32 * (dh - 1) as f32 / (th - 1) as f32
            };
            px.copy_from_slice(&self.composite_pixel_of(&contributing, doc_x, doc_y));
        });
        (tw, th, rgba)
    }

    pub fn pick_color(&mut self, doc_x: f32, doc_y: f32) -> Option<[u8; 4]> {
        let color = self.sample_color(doc_x, doc_y)?;
        self.color = color;
        Some(color)
    }

    pub fn composite_thumbnail(&self, max_side: u32) -> (u32, u32, Vec<u8>) {
        let max_side = max_side.max(1);
        let (dw, dh, full) = self.composite_rgba();
        let (crop_x, crop_y, crop_w, crop_h) = self
            .painted_content_bounds()
            .map(|(x0, y0, x1, y1)| {
                let x0 = x0.floor().max(0.0) as u32;
                let y0 = y0.floor().max(0.0) as u32;
                let x1 = x1.ceil().min(dw as f32).max(x0 as f32 + 1.0) as u32;
                let y1 = y1.ceil().min(dh as f32).max(y0 as f32 + 1.0) as u32;
                (x0, y0, (x1 - x0).max(1).min(dw), (y1 - y0).max(1).min(dh))
            })
            .unwrap_or((0, 0, dw, dh));
        let crop_w = crop_w.min(dw.saturating_sub(crop_x)).max(1);
        let crop_h = crop_h.min(dh.saturating_sub(crop_y)).max(1);

        let scale = (max_side as f32 / crop_w as f32)
            .min(max_side as f32 / crop_h as f32)
            .min(1.0);
        let tw = ((crop_w as f32) * scale).round().max(1.0) as u32;
        let th = ((crop_h as f32) * scale).round().max(1.0) as u32;

        if tw == dw && th == dh && crop_x == 0 && crop_y == 0 {
            return (dw, dh, full);
        }

        let mut rgba = vec![0u8; (tw as usize) * (th as usize) * 4];
        for ty in 0..th {
            for tx in 0..tw {
                let sx = if tw <= 1 {
                    crop_x
                } else {
                    crop_x
                        + ((tx as f32) * ((crop_w - 1) as f32) / ((tw - 1) as f32)).round() as u32
                };
                let sy = if th <= 1 {
                    crop_y
                } else {
                    crop_y
                        + ((ty as f32) * ((crop_h - 1) as f32) / ((th - 1) as f32)).round() as u32
                };
                let si = ((sy as usize) * (dw as usize) + (sx as usize)) * 4;
                let di = ((ty as usize) * (tw as usize) + (tx as usize)) * 4;
                rgba[di..di + 4].copy_from_slice(&full[si..si + 4]);
            }
        }
        (tw, th, rgba)
    }

    pub fn painted_content_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut any = false;
        for layer in &self.layers {
            if !layer.visible || layer.is_paper() {
                continue;
            }
            let Some(raw) = layer.content_bounds() else {
                continue;
            };
            let corners = match layer.transform {
                Some(t) if !t.is_identity() => {
                    let pivot = bounds_center(raw);
                    t.transformed_corners(pivot, raw)
                }
                _ => [
                    (raw.0, raw.1),
                    (raw.2, raw.1),
                    (raw.2, raw.3),
                    (raw.0, raw.3),
                ],
            };
            for (x, y) in corners {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
                any = true;
            }
        }
        if !any {
            return None;
        }
        Some((
            min_x.clamp(0.0, self.width as f32),
            min_y.clamp(0.0, self.height as f32),
            max_x.clamp(0.0, self.width as f32),
            max_y.clamp(0.0, self.height as f32),
        ))
    }

    pub fn layer_rgba(&self, index: usize) -> Option<(u32, u32, Vec<u8>)> {
        let layer = self.layers.get(index)?;
        if layer.tiles().is_none() && layer.content.item().is_none() {
            return None;
        }
        let w = self.width.max(1);
        let h = self.height.max(1);
        let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
        copy_layer_into_rgba(layer, &mut buf, w, h);
        apply_mask(&mut buf, layer.mask());
        if let Some(adj) = &layer.adjustments {
            let lut = adj.lut();
            buf.par_chunks_mut(EFFECT_CHUNK_BYTES)
                .for_each(|block| lut.apply_rgba(block));
        }
        Some((w, h, buf))
    }

    pub fn layer_svg(&self, index: usize) -> Option<String> {
        let layer = self.layers.get(index)?;
        let item = layer.content.item()?;
        let mut svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">",
            self.width, self.height, self.width, self.height
        );
        if let Some(group) = crate::vector_svg::svg_transform_attr(item, layer.transform) {
            svg.push_str(&group);
        }
        if let Some(markup) = crate::vector_svg::item_svg(item) {
            svg.push_str(&markup);
        }
        if layer.transform.is_some_and(|t| !t.is_identity()) {
            svg.push_str("</g>");
        }
        svg.push_str("</svg>");
        Some(svg)
    }

    /// The selected pixels of the active layer, for copy and cut.
    ///
    /// Read through the same `LayerSelectSample` the selection was *built* from, which is the
    /// only way the two can agree: a vector layer has no tiles to read at all, and a
    /// transformed raster layer's tiles sit in its own space while the selection is in the
    /// document's. Reading the grid directly answered both of those wrong.
    pub fn selection_rgba(&self) -> Option<(u32, u32, Vec<u8>)> {
        let selection = self.selection.as_ref()?;
        let bounds = selection.bounds().intersect(self.bounds())?;
        let layer = self.layers.get(self.active_layer)?;
        let sample = crate::select_sample::LayerSelectSample::new(layer, self.bounds())?;
        let w = (bounds.max_x - bounds.min_x + 1) as u32;
        let h = (bounds.max_y - bounds.min_y + 1) as u32;
        let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
        let row_bytes = (w as usize) * 4;
        buf.par_chunks_mut(row_bytes)
            .enumerate()
            .for_each(|(y, row)| {
                let doc_y = bounds.min_y + y as i32;
                for x in 0..w as i32 {
                    let doc_x = bounds.min_x + x;
                    if !selection.contains(doc_x as f32 + 0.5, doc_y as f32 + 0.5) {
                        continue;
                    }
                    let px = sample.pixel(doc_x, doc_y);
                    if px[3] == 0 {
                        continue;
                    }
                    let i = (x as usize) * 4;
                    row[i..i + 4].copy_from_slice(&px);
                }
            });
        Some((w, h, buf))
    }

    pub fn clear_selection_pixels(&mut self) -> bool {
        if !self.active_layer_accepts_paint() {
            return false;
        }
        let Some(selection) = self.selection.clone() else {
            return false;
        };
        let Some(bounds) = selection.bounds().intersect(self.bounds()) else {
            return false;
        };
        let active = self.active_layer;
        let Some(layer) = self.layers.get(active) else {
            return false;
        };
        let layer_id = layer.id.clone();
        let Some(grid) = layer.tiles() else {
            return false;
        };
        let mut coords = TileSet::default();
        tiles_covering(bounds, &mut coords);
        let coords: Vec<TileCoord> = coords
            .into_iter()
            .filter(|c| grid.tile_in_bounds(*c))
            .collect();
        if coords.is_empty() {
            return false;
        }
        let before = grid.snapshot_tiles(&coords);
        let mut touched = false;
        if let Some(tiles) = self.layers.get_mut(active).and_then(|l| l.tiles_mut()) {
            let count = tiles.paint_rect(bounds, |x, y, dst| {
                if dst[3] == 0 || !selection.contains(x as f32 + 0.5, y as f32 + 0.5) {
                    return None;
                }
                Some([0, 0, 0, 0])
            });
            touched = count > 0;
        }
        if !touched {
            return false;
        }
        self.history
            .push_layer_tiles(layer_id, before, Some(active));
        true
    }

    pub fn undo(&mut self) -> bool {
        self.commit_text();
        let Some(command) = self.history.take_undo() else {
            return false;
        };
        let inverse = self.invert_history_command(&command);
        self.apply_history_command(&command);
        if let Some(index) = command.active_layer_index {
            self.set_active_layer_index(index);
        }
        self.history.finish_undo(command, inverse);
        self.active_layer = self.active_layer.min(self.layers.len().saturating_sub(1));
        true
    }

    pub fn redo(&mut self) -> bool {
        self.commit_text();
        let Some(command) = self.history.take_redo() else {
            return false;
        };
        let inverse = self.invert_history_command(&command);
        self.apply_history_command(&command);
        if let Some(index) = command.active_layer_index {
            self.set_active_layer_index(index);
        }
        self.history.finish_redo(command, inverse);
        self.active_layer = self.active_layer.min(self.layers.len().saturating_sub(1));
        true
    }

    pub fn clear_active_layer(&mut self) {
        if !self.active_layer_accepts_paint() {
            return;
        }
        let active = self.active_layer;
        let Some(layer) = self.layers.get_mut(active) else {
            return;
        };
        let layer_id = layer.id.clone();
        let snap = layer.clear();
        if snap.is_empty() {
            return;
        }
        self.history.push_layer_tiles(layer_id, snap, Some(active));
    }

    pub fn mark_all_layers_dirty(&mut self) {
        for layer in &mut self.layers {
            layer.mark_all_dirty();
        }
    }

    pub fn clear_layer_dirty(&mut self, channel: DirtyChannel) {
        for layer in &mut self.layers {
            layer.clear_dirty(channel);
        }
    }

    /// A layer's box in document space, tight to what is actually painted.
    ///
    /// This used to have to reach past `content_bounds` for its own tile scan, because
    /// `content_bounds` was tile-granular and a ten-pixel stroke reported a 256-pixel box —
    /// fine for an outline the eye reads as approximate, useless as a number in a panel. Now
    /// that `content_bounds` is the tight box everywhere, the readout and the frame on the
    /// board are the same rectangle, and they cannot drift by up to a tile any more.
    pub fn layer_bounds(&self, index: usize) -> Option<(f32, f32, f32, f32)> {
        let layer = self.layers.get(index)?;
        let raw = layer.content_bounds()?;
        let t = layer.transform.unwrap_or_default();
        Some(t.transformed_aabb(raw))
    }

    /// Moves a layer so its box starts at `(x, y)`, and crops it to `width` × `height`.
    ///
    /// Size only ever **crops**. A size larger than the layer already is gets clamped rather
    /// than scaling the content up: there are no pixels to invent, and a number field that
    /// quietly resampled a layer would destroy detail on a typo. Scaling up is what the
    /// Transform tool is for.
    ///
    /// Position is a transform offset, so moving is non-destructive and undoes cleanly — the
    /// same thing the Move tool writes. Cropping is not: it discards pixels outside the box.
    /// A layer carrying a scale or rotation is moved but **not** cropped, since the crop
    /// rectangle would have to be resolved in the layer's own frame rather than the
    /// document's; the caller can see that from the bounds it reads back.
    pub fn set_layer_bounds(&mut self, index: usize, x: f32, y: f32, w: f32, h: f32) -> bool {
        if self.layer_locked(index) {
            return false;
        }
        let Some((cur_x, cur_y, cur_x1, cur_y1)) = self.layer_bounds(index) else {
            return false;
        };
        let Some(layer) = self.layers.get_mut(index) else {
            return false;
        };
        let mut t = layer.transform.unwrap_or_default();
        t.offset_x += x - cur_x;
        t.offset_y += y - cur_y;
        layer.transform = (!t.is_identity()).then_some(t);

        let crop_w = w.max(1.0).min(cur_x1 - cur_x);
        let crop_h = h.max(1.0).min(cur_y1 - cur_y);
        let shrinks = crop_w < cur_x1 - cur_x || crop_h < cur_y1 - cur_y;
        let square = t.scale_x == 1.0 && t.scale_y == 1.0 && t.rotation == 0.0;
        if !shrinks || !square {
            return true;
        }
        // The crop is stated in document space but the pixels live in the layer's own
        // untransformed space, so the offset comes back off before the rectangle is applied.
        let keep = DocRect::from_floats(
            x - t.offset_x,
            y - t.offset_y,
            x - t.offset_x + crop_w - 1.0,
            y - t.offset_y + crop_h - 1.0,
        );
        let Some(grid) = layer.tiles_mut() else {
            return true;
        };
        for band in outside_bands(grid.bounds(), keep) {
            grid.paint_rect(band, |_, _, _| Some([0, 0, 0, 0]));
        }
        true
    }

    pub fn layer_highlight(&self) -> Option<(usize, [(f32, f32); 4])> {
        if let Some(drag) = &self.transform_drag {
            if drag.handle != TransformHandle::Move {
                return None;
            }
            let corners = self.layer_outline_corners(drag.layer_index())?;
            return Some((drag.layer_index(), corners));
        }
        let index = self.hover_layer?;
        let corners = self.layer_outline_corners(index)?;
        Some((index, corners))
    }

    pub fn layer_highlights(&self) -> Vec<(usize, [(f32, f32); 4])> {
        if let Some(drag) = &self.transform_drag {
            if drag.handle == TransformHandle::Move {
                return drag
                    .targets
                    .iter()
                    .filter_map(|target| {
                        self.layer_outline_corners(target.layer_index)
                            .map(|corners| (target.layer_index, corners))
                    })
                    .collect();
            }
            if let Some(corners) = self.layer_outline_corners(drag.layer_index()) {
                return vec![(drag.layer_index(), corners)];
            }
            return Vec::new();
        }
        self.layer_highlight().into_iter().collect()
    }

    fn layer_outline_corners(&self, index: usize) -> Option<[(f32, f32); 4]> {
        let layer = self.layers.get(index)?;
        if layer.is_paper() {
            return None;
        }
        let raw_bounds = layer.content_bounds()?;
        let pivot = bounds_center(raw_bounds);
        let t = layer.transform.unwrap_or_default();
        Some(t.transformed_corners(pivot, raw_bounds))
    }

    /// Whether a *gesture* is in flight: the pointer is down and the board's geometry is being
    /// dragged out under it. Only these keep the renderer off its caches — the low-resolution
    /// overview proxy is wrong to show mid-drag, and a shifted pan-cache blit cannot apply to a
    /// frame whose content is changing.
    ///
    /// A hovered layer deliberately does not count: its outline is a static overlay that
    /// `set_hover_layer` already invalidates once on the way in and once on the way out.
    ///
    /// Neither does an **active selection** or **transform mode**, for exactly the same reason.
    /// Both are modes you sit in rather than gestures you perform — a marquee lives until ⌘D —
    /// and both draw static overlays. Counting them here pinned `frame_dirty` to `Content` at
    /// display rate for as long as the mode was open, which re-synced every tile, rebuilt the
    /// draw list and re-composited the whole visible stack every frame, and disabled the
    /// overview proxy on exactly the documents too large to draw the full way. Every FFI entry
    /// that touches either one already calls `Renderer::invalidate`, which is the one frame
    /// they need.
    pub fn has_live_preview(&self) -> bool {
        self.stroke_active
            || self.shape_drag.is_some()
            || self.transform_drag.is_some()
            || self.vector_drag.is_some()
            || self.guide_drag.is_some()
    }

    /// Whether an overlay is *animating* and so needs a frame per display refresh even though
    /// nothing about the document changed. Only the text caret, which blinks off the renderer's
    /// own clock. This asks for `FrameDirty::Overlay`, not `Content`: the tiles, the draw list
    /// and the pan cache are all still valid, so the frame is one instance-buffer write.
    pub fn has_animated_overlay(&self) -> bool {
        self.text_edit.is_some()
    }
}

/// `outer \ inner` as up to four non-overlapping bands. Cropping clears these rather than
/// rewriting the whole layer, so the cost is the discarded margin and not the picture.
pub(crate) fn outside_bands(outer: DocRect, inner: DocRect) -> Vec<DocRect> {
    let Some(inner) = outer.intersect(inner) else {
        return vec![outer];
    };
    let mut out = Vec::with_capacity(4);
    if inner.min_y > outer.min_y {
        out.push(DocRect::new(
            outer.min_x,
            outer.min_y,
            outer.max_x,
            inner.min_y - 1,
        ));
    }
    if inner.max_y < outer.max_y {
        out.push(DocRect::new(
            outer.min_x,
            inner.max_y + 1,
            outer.max_x,
            outer.max_y,
        ));
    }
    if inner.min_x > outer.min_x {
        out.push(DocRect::new(
            outer.min_x,
            inner.min_y,
            inner.min_x - 1,
            inner.max_y,
        ));
    }
    if inner.max_x < outer.max_x {
        out.push(DocRect::new(
            inner.max_x + 1,
            inner.min_y,
            outer.max_x,
            inner.max_y,
        ));
    }
    out
}
