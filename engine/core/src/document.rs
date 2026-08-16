use crate::camera::Camera;
use crate::filters::AdjustmentLut;
use crate::history::{History, TileSnapshot};
use crate::layer::Layer;
use crate::limits::{
    BRUSH_SIZE_DEFAULT, DEFAULT_INK, EFFECT_CHUNK_BYTES, FILL_TOLERANCE_DEFAULT,
    INK_OPACITY_DEFAULT, INK_OPACITY_MAX, INK_OPACITY_MIN, MAX_CANVAS_SIDE, MIN_CANVAS_SIDE,
    MIN_STAMP_SPACING, MIN_STROKE_POINT_DISTANCE, PAPER_WHITE, STAMP_COVERAGE_PADDING,
    STAMP_SPACING_RATIO, STROKE_POINT_CAPACITY,
};
use crate::palette::BoardColors;
use crate::selection::{Selection, SelectionShape};
use crate::shape::{Shape, Tool};
use crate::text_edit::TextEdit;
use crate::tile::{
    blend_over, blend_with_mode, DirtyChannel, DocRect, TileCoord, TileSet, TILE_SIZE,
};
use crate::transform::{bounds_center, clipped_pixel_span, LayerTransform};
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

fn apply_mask(rgba: &mut [u8], mask: Option<&[u8]>) {
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
        return [0, 0, 0, 0];
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

/// Hit-testing a vector layer evaluates its items' coverage directly rather than sampling
/// pixels — there are no pixels to sample. The point is inverse-mapped through the layer
/// transform first, exactly as the rasterizer and the shader both do.
fn vector_alpha_at(items: &[vector::VectorItem], layer: &Layer, doc_x: f32, doc_y: f32) -> u8 {
    let local = match layer
        .transform
        .filter(|t| !t.is_identity())
        .zip(vector::items_bounds(items))
    {
        Some((t, raw)) => t.inverse(bounds_center(raw), (doc_x, doc_y)),
        None => (doc_x, doc_y),
    };
    let mut acc = 0.0f32;
    for item in items {
        let coverage = item.coverage(local.0, local.1) * (item.color()[3] as f32 / 255.0);
        acc = acc.max(coverage);
    }
    (acc * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Alpha alone, for hit-testing. Adjustments never touch alpha, so picking skips the
/// colour work `layer_composited_pixel` does — an HSL round trip per layer per click.
pub(crate) fn layer_alpha_at(layer: &Layer, doc_x: f32, doc_y: f32, doc_w: u32, doc_h: u32) -> u8 {
    let alpha = match layer.content.items() {
        Some(items) => vector_alpha_at(items, layer, doc_x, doc_y),
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

fn layer_pickable_at(layer: &Layer, doc_x: f32, doc_y: f32, doc_w: u32, doc_h: u32) -> bool {
    layer.visible
        && !layer.is_paper()
        && layer.opacity > 0.0
        && (layer.tiles().is_some() || layer.content.items().is_some())
        && layer_alpha_at(layer, doc_x, doc_y, doc_w, doc_h) != 0
}

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

fn copy_layer_into_rgba(layer: &Layer, buf: &mut [u8], w: u32, h: u32) {
    if let Some(items) = layer.content.items() {
        vector::rasterize_into_rgba(items, layer.transform, buf, w, h);
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

fn apply_layer_effects(rgba: &mut [u8], layer: &Layer, lut: Option<&AdjustmentLut>) {
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
    pub brush_size: f32,
    pub ink_opacity: f32,
    pub fill: bool,
    pub dark_theme: bool,
    pub accent: [u8; 3],
    pub board_colors: BoardColors,
    pub hover_layer: Option<usize>,
    pub stroke_active: bool,
    pub stroke_points: Vec<StrokePoint>,
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
    pub shift_held: bool,
    /// Whether the shape tools and the pen commit as resolution-independent vector items
    /// instead of stamping pixels. A shell knob, like `fill`.
    pub vector_mode: bool,
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
    stroke_before: TileSnapshot,
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

#[derive(Clone, Copy, Debug)]
pub(crate) struct TransformDrag {
    pub(crate) layer_index: usize,
    pub(crate) handle: TransformHandle,
    pub(crate) pivot: (f32, f32),
    pub(crate) raw_bounds: (f32, f32, f32, f32),
    pub(crate) start_transform: LayerTransform,
    pub(crate) start_pointer: (f32, f32),
}

impl TransformDrag {
    pub(crate) fn layer_move(
        layer_index: usize,
        pivot: (f32, f32),
        raw_bounds: (f32, f32, f32, f32),
        start_transform: LayerTransform,
        start_pointer: (f32, f32),
    ) -> Self {
        Self {
            layer_index,
            handle: TransformHandle::Move,
            pivot,
            raw_bounds,
            start_transform,
            start_pointer,
        }
    }
}

pub type TransformHandles = (usize, [(f32, f32); 4], (f32, f32));

const HANDLE_HIT_RADIUS_PX: f32 = 10.0;
const ROTATE_HANDLE_OFFSET_PX: f32 = 24.0;
const TRANSFORM_MIN_SCALE_GUARD: f32 = 0.02;

fn point_dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
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
            brush_size: BRUSH_SIZE_DEFAULT,
            ink_opacity: INK_OPACITY_DEFAULT,
            fill: false,
            dark_theme: true,
            accent: crate::palette::random_project_color(),
            board_colors: BoardColors::fallback(true),
            hover_layer: None,
            stroke_active: false,
            stroke_points: Vec::with_capacity(STROKE_POINT_CAPACITY),
            stroke_straight_anchor: None,
            shape_drag: None,
            selection: None,
            shift_held: false,
            vector_mode: false,
            vector_revision: 0,
            selected_vector: None,
            vector_drag: None,
            last_shape_tool: Tool::Rect,
            last_select_tool: Tool::SelectRect,
            transform_active: false,
            text_style: TextRun::default(),
            text_edit: None,
            transform_drag: None,
            stroke_before: TileSnapshot::default(),
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
        self.layers.push(Layer::new(name, self.width, self.height));
        self.active_layer = self.layers.len() - 1;
    }

    /// Whether paint tools may write into the active layer.
    ///
    /// A text layer's tiles are a cache of its run — `text_layer::resync` clears and rebuilds
    /// them on the next keystroke, so a stroke committed there would disappear without a
    /// trace. Painting is refused until the layer is turned into ordinary pixels with
    /// `rasterize_text_layer`, the same bargain every editor with live text makes.
    pub fn active_layer_accepts_paint(&self) -> bool {
        self.layers
            .get(self.active_layer)
            .is_some_and(|layer| layer.tiles().is_some() && !layer.is_text())
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
        if index >= self.layers.len() {
            return false;
        }
        self.commit_text();
        self.clear_vector_selection();
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

    pub fn set_layer_opacity(&mut self, index: usize, opacity: f32) {
        if let Some(layer) = self.layers.get_mut(index) {
            layer.opacity = opacity.clamp(0.0, 1.0);
            layer.mark_channel_dirty(DirtyChannel::Render);
        }
    }

    pub fn set_layer_blend_mode(&mut self, index: usize, mode: crate::layer::BlendMode) {
        if let Some(layer) = self.layers.get_mut(index) {
            layer.blend_mode = mode;
        }
    }

    pub fn set_layer_adjustments(
        &mut self,
        index: usize,
        adjustments: crate::filters::Adjustments,
    ) {
        if let Some(layer) = self.layers.get_mut(index) {
            let adjustments = adjustments.clamped();
            layer.adjustments = if adjustments.is_neutral() {
                None
            } else {
                Some(adjustments)
            };
            layer.mark_channel_dirty(DirtyChannel::Render);
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

    pub fn add_vector_layer(&mut self, name: impl Into<String>) -> usize {
        self.layers.push(Layer::vector(name, Vec::new()));
        self.active_layer = self.layers.len() - 1;
        self.active_layer
    }

    /// Where a vector shape or path lands: the active layer if it is already a vector layer,
    /// otherwise a fresh vector layer above the stack, which then becomes active so the next
    /// item joins it. That is what lets a freehand drawing accumulate as many paths in one
    /// layer instead of one layer per stroke.
    fn vector_target_layer(&mut self) -> usize {
        if self
            .layers
            .get(self.active_layer)
            .is_some_and(|l| l.content.is_vector())
        {
            return self.active_layer;
        }
        let n = self.layers.iter().filter(|l| l.content.is_vector()).count() + 1;
        self.add_vector_layer(crate::names::numbered_vector_layer(n))
    }

    fn push_vector_item(&mut self, item: vector::VectorItem) {
        let index = self.vector_target_layer();
        let Some(items) = self
            .layers
            .get_mut(index)
            .and_then(|l| l.content.items_mut())
        else {
            return;
        };
        items.push(item);
        self.vector_revision = self.vector_revision.wrapping_add(1);
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
        if self.shift_held && shape.tool.constrains_to_square() {
            shape.end = crate::shape::square_end(shape.start, shape.end);
        }
        Some(shape)
    }

    pub fn reset_layer_transform(&mut self, index: usize) {
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
        let Some(layer) = self.layers.get(self.active_layer) else {
            return false;
        };
        if layer.content.is_text() || layer.content_bounds().is_none() {
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

    fn rotate_handle_position(
        pivot: (f32, f32),
        raw_bounds: (f32, f32, f32, f32),
        t: LayerTransform,
        zoom: f32,
    ) -> (f32, f32) {
        let top_mid_raw = ((raw_bounds.0 + raw_bounds.2) * 0.5, raw_bounds.1);
        let top_mid = t.forward(pivot, top_mid_raw);
        let dir = (top_mid.0 - pivot.0, top_mid.1 - pivot.1);
        let dir_len = point_dist(pivot, top_mid).max(1e-3);
        (
            top_mid.0 + dir.0 / dir_len * ROTATE_HANDLE_OFFSET_PX / zoom.max(1e-6),
            top_mid.1 + dir.1 / dir_len * ROTATE_HANDLE_OFFSET_PX / zoom.max(1e-6),
        )
    }

    pub fn transform_handles(&self) -> Option<TransformHandles> {
        if !self.transform_active {
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
        let rotate_handle = Self::rotate_handle_position(pivot, raw_bounds, t, self.camera.zoom);
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
        let names = [
            TransformHandle::TopLeft,
            TransformHandle::TopRight,
            TransformHandle::BottomRight,
            TransformHandle::BottomLeft,
        ];
        let point = (doc_x, doc_y);
        for (corner, handle) in corners.iter().zip(names) {
            if point_dist(*corner, point) <= hit_r {
                return Some(TransformDrag {
                    layer_index: index,
                    handle,
                    pivot,
                    raw_bounds,
                    start_transform: t,
                    start_pointer: point,
                });
            }
        }
        let rotate_handle = Self::rotate_handle_position(pivot, raw_bounds, t, zoom);
        if point_dist(rotate_handle, point) <= hit_r {
            return Some(TransformDrag {
                layer_index: index,
                handle: TransformHandle::Rotate,
                pivot,
                raw_bounds,
                start_transform: t,
                start_pointer: point,
            });
        }
        if point_in_quad(point, corners) {
            return Some(TransformDrag {
                layer_index: index,
                handle: TransformHandle::Move,
                pivot,
                raw_bounds,
                start_transform: t,
                start_pointer: point,
            });
        }
        None
    }

    fn begin_transform_drag(&mut self, doc_x: f32, doc_y: f32) -> bool {
        self.transform_drag = self.transform_handle_at(doc_x, doc_y);
        self.transform_drag.is_some()
    }

    pub fn layer_at(&self, doc_x: f32, doc_y: f32) -> Option<usize> {
        if doc_x < 0.0 || doc_y < 0.0 || doc_x >= self.width as f32 || doc_y >= self.height as f32 {
            return None;
        }
        self.layers
            .iter()
            .enumerate()
            .rev()
            .find(|(_, layer)| layer_pickable_at(layer, doc_x, doc_y, self.width, self.height))
            .map(|(index, _)| index)
    }

    pub fn pick_layer(&mut self, doc_x: f32, doc_y: f32) -> Option<usize> {
        let index = self.layer_at(doc_x, doc_y)?;
        self.active_layer = index;
        Some(index)
    }

    fn retarget_transform(&mut self, doc_x: f32, doc_y: f32) -> bool {
        if self.pick_layer(doc_x, doc_y).is_none() {
            return false;
        }
        self.begin_transform_drag(doc_x, doc_y);
        true
    }

    /// Same rule as picking a layer from the stack (`layer_pickable_at`): a layer that is
    /// hidden, has no opacity, or has nothing painted here does not get to claim the click.
    /// Without the `visible` check, hiding the active layer while it still has paint under
    /// the cursor made every later click there grab the invisible layer instead of falling
    /// through to whatever visible layer is actually underneath — dragging looked like a
    /// no-op because the layer that moved couldn't be seen.
    fn active_layer_covers(&self, doc_x: f32, doc_y: f32) -> bool {
        self.layers
            .get(self.active_layer)
            .is_some_and(|l| layer_pickable_at(l, doc_x, doc_y, self.width, self.height))
    }

    /// The transform box is `content_bounds()`, which is tile-granular — for a small
    /// scribble on a big canvas it can be the whole document. Taking the Move handle on
    /// every click inside it would make picking unreachable, so a click that lands on
    /// *nothing the active layer actually painted* gets offered to the layer stack first.
    /// Clicking the active layer's own pixels always keeps it, so a transform target is
    /// never lost to a layer that merely overlaps it.
    ///
    /// A vector item under the cursor outranks the whole-layer Move handle — a layer that is
    /// a *list* of items is moved one item at a time by default, and the corner and rotate
    /// handles (checked first) are still how the whole layer is scaled or turned.
    fn transform_pointer_down(&mut self, doc_x: f32, doc_y: f32) {
        let handle = self.transform_handle_at(doc_x, doc_y);
        if let Some(drag) = handle.filter(|d| d.handle != TransformHandle::Move) {
            self.clear_vector_selection();
            self.transform_drag = Some(drag);
            return;
        }
        if self.begin_vector_item_drag(doc_x, doc_y) {
            return;
        }
        self.clear_vector_selection();
        let handled_directly = handle.is_some() && self.active_layer_covers(doc_x, doc_y);
        if handled_directly {
            self.transform_drag = handle;
            return;
        }
        if self.retarget_transform(doc_x, doc_y) {
            return;
        }
        if handle.is_some() {
            self.transform_drag = handle;
            return;
        }
        self.exit_transform();
    }

    pub(crate) fn update_transform_drag(&mut self, doc_x: f32, doc_y: f32) {
        let Some(drag) = self.transform_drag else {
            return;
        };
        let mut next = drag.start_transform;
        match drag.handle {
            TransformHandle::Move => {
                next.offset_x = drag.start_transform.offset_x + (doc_x - drag.start_pointer.0);
                next.offset_y = drag.start_transform.offset_y + (doc_y - drag.start_pointer.1);
            }
            TransformHandle::Rotate => {
                let start_angle = angle_from(drag.pivot, drag.start_pointer);
                let now_angle = angle_from(drag.pivot, (doc_x, doc_y));
                next.rotation = drag.start_transform.rotation + (now_angle - start_angle);
            }
            corner => {
                let (sign_x, sign_y) = match corner {
                    TransformHandle::TopLeft => (-1.0, -1.0),
                    TransformHandle::TopRight => (1.0, -1.0),
                    TransformHandle::BottomRight => (1.0, 1.0),
                    TransformHandle::BottomLeft => (-1.0, 1.0),
                    TransformHandle::Rotate | TransformHandle::Move => unreachable!(),
                };
                let half_w = ((drag.raw_bounds.2 - drag.raw_bounds.0) * 0.5).max(1e-3);
                let half_h = ((drag.raw_bounds.3 - drag.raw_bounds.1) * 0.5).max(1e-3);
                let local_now = drag.start_transform.to_local(drag.pivot, (doc_x, doc_y));
                let raw_x = sign_x * half_w;
                let raw_y = sign_y * half_h;
                if self.shift_held {
                    next.scale_x = (local_now.0 / raw_x).abs().max(TRANSFORM_MIN_SCALE_GUARD);
                    next.scale_y = (local_now.1 / raw_y).abs().max(TRANSFORM_MIN_SCALE_GUARD);
                } else {
                    let raw_dist = point_dist((0.0, 0.0), (raw_x, raw_y)).max(1e-3);
                    let now_dist = point_dist((0.0, 0.0), local_now);
                    let s = (now_dist / raw_dist).max(TRANSFORM_MIN_SCALE_GUARD);
                    next.scale_x = s;
                    next.scale_y = s;
                }
            }
        }
        let next = next.clamped();
        if let Some(layer) = self.layers.get_mut(drag.layer_index) {
            layer.transform = Some(next);
        }
    }

    pub fn duplicate_layer(&mut self, index: usize) -> bool {
        if index >= self.layers.len() {
            return false;
        }
        self.commit_text();
        let Some(source) = self.layers.get(index) else {
            return false;
        };
        let base_name = source.name.clone();
        let mut copy = source.clone();
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
        self.layers.swap(index, other);
        let remap = |i: usize| {
            if i == index {
                other
            } else if i == other {
                index
            } else {
                i
            }
        };
        self.active_layer = remap(self.active_layer);
        self.hover_layer = self.hover_layer.map(remap);
        if let Some(drag) = &mut self.transform_drag {
            drag.layer_index = remap(drag.layer_index);
        }
        if let Some(pick) = &mut self.selected_vector {
            pick.layer = remap(pick.layer);
        }
        if let Some(edit) = &mut self.text_edit {
            edit.layer = remap(edit.layer);
        }
        true
    }

    pub fn merge_layer_down(&mut self, index: usize) -> bool {
        if index == 0 || index >= self.layers.len() {
            return false;
        }
        if self.layers[index - 1].is_paper() {
            return false;
        }
        self.commit_text();
        self.rasterize_text_layer(index - 1);
        if self.layers[index].tiles().is_none() && self.layers[index].content.items().is_none() {
            return false;
        }
        if self.layers[index - 1].tiles().is_none() {
            return false;
        }
        self.clear_vector_selection();
        let mode = self.layers[index].blend_mode;
        let w = self.width.max(1);
        let h = self.height.max(1);
        let mut src_buf = vec![0u8; (w as usize) * (h as usize) * 4];
        copy_layer_into_rgba(&self.layers[index], &mut src_buf, w, h);
        let lut = self.layers[index].adjustments.map(|a| a.lut());
        apply_layer_effects(&mut src_buf, &self.layers[index], lut.as_ref());

        let Some(dst) = self.layers[index - 1].tiles_mut() else {
            return false;
        };
        dst.paint_rect(DocRect::from_size(w, h), |x, y, dst_px| {
            let i = ((y as usize) * (w as usize) + (x as usize)) * 4;
            let src_px = [src_buf[i], src_buf[i + 1], src_buf[i + 2], src_buf[i + 3]];
            if src_px[3] == 0 {
                return None;
            }
            Some(blend_with_mode(dst_px, src_px, mode))
        });

        self.layers.remove(index);
        if self.active_layer >= self.layers.len() {
            self.active_layer = self.layers.len().saturating_sub(1);
        } else if self.active_layer > index {
            self.active_layer -= 1;
        } else {
            self.active_layer = index - 1;
        }
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
        for layer in &mut self.layers {
            layer.resize_mask(old_width, old_height, new_width, new_height);
            let is_paper = layer.is_paper();
            let Some(tiles) = layer.tiles_mut() else {
                continue;
            };
            tiles.width = new_width;
            tiles.height = new_height;
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
        if self.transform_active {
            self.transform_pointer_down(dx, dy);
            return;
        }
        if self.tool == Tool::Move {
            self.commit_text();
            self.begin_move_at(dx, dy);
            return;
        }
        if self.tool == Tool::Text {
            self.begin_text_at(dx, dy);
            return;
        }
        self.commit_text();
        if self.tool == Tool::Fill {
            self.commit_fill(dx, dy);
            return;
        }
        if self.tool == Tool::Eyedropper {
            let _ = self.pick_color(dx, dy);
            return;
        }
        if matches!(self.tool, Tool::Pen | Tool::Eraser | Tool::SelectLasso) {
            self.begin_stroke();
            self.push_stroke_point(dx, dy);
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
            });
        }
    }

    pub fn pointer_move(&mut self, screen_x: f32, screen_y: f32) {
        let (dx, dy) = self.camera.to_doc(screen_x, screen_y);
        if self.transform_active {
            if !self.update_vector_item_drag(dx, dy) {
                self.update_transform_drag(dx, dy);
            }
            return;
        }
        if self.tool == Tool::Move {
            self.update_move_drag(dx, dy);
            return;
        }
        if matches!(self.tool, Tool::Pen | Tool::Eraser | Tool::SelectLasso) && self.stroke_active {
            self.push_stroke_point(dx, dy);
        } else if let Some(shape) = &mut self.shape_drag {
            shape.end = (dx, dy);
            shape.half_width = self.brush_size * 0.5;
            shape.fill = self.fill;
        }
    }

    pub fn pointer_up(&mut self, screen_x: f32, screen_y: f32) {
        let (dx, dy) = self.camera.to_doc(screen_x, screen_y);
        if self.transform_active {
            self.end_vector_item_drag();
            self.transform_drag = None;
            return;
        }
        if self.tool == Tool::Move {
            self.end_move_drag();
            return;
        }
        if matches!(self.tool, Tool::Pen | Tool::Eraser | Tool::SelectLasso) {
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
        self.stroke_straight_anchor = None;
        self.stroke_before.clear();
    }

    fn push_stroke_point(&mut self, x: f32, y: f32) {
        if self.shift_held && matches!(self.tool, Tool::Pen | Tool::Eraser) {
            let anchor = *self
                .stroke_straight_anchor
                .get_or_insert(self.stroke_points.len().saturating_sub(1));
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

    fn commit_stroke(&mut self) {
        if !self.stroke_active {
            return;
        }
        if !self.vector_mode && !self.active_layer_accepts_paint() {
            self.stroke_active = false;
            self.stroke_points.clear();
            return;
        }
        self.stroke_active = false;
        let points = std::mem::take(&mut self.stroke_points);
        if points.is_empty() {
            return;
        }
        if self.vector_mode && self.tool == Tool::Pen {
            let pts: Vec<(f32, f32)> = points.iter().map(|p| (p.x, p.y)).collect();
            if let Some(item) = vector::item_from_points(&pts, self.ink_rgba(), self.brush_size) {
                self.push_vector_item(item);
            }
            return;
        }
        let radius = self.brush_size * 0.5;
        let color = self.ink_rgba();
        let erasing = self.tool == Tool::Eraser;
        let active = self.active_layer;
        let stamps = stroke_stamps(&points, radius);

        let Some(span) = stamps_bounds(&stamps, radius) else {
            return;
        };
        let Some(span) = span.intersect(self.bounds()) else {
            return;
        };

        let mut touched = TileSet::default();
        tiles_covering(span, &mut touched);

        let Some(layer) = self.layers.get(active) else {
            return;
        };
        let layer_id = layer.id.clone();
        let Some(grid) = layer.tiles() else {
            return;
        };
        let touched: Vec<TileCoord> = touched
            .into_iter()
            .filter(|c| grid.tile_in_bounds(*c))
            .collect();
        if touched.is_empty() {
            return;
        }
        self.stroke_before = grid.snapshot_tiles(&touched);

        let mut painted = false;
        if let Some(tiles) = self.layers.get_mut(active).and_then(|l| l.tiles_mut()) {
            for p in &stamps {
                let touched = if erasing {
                    tiles.stamp_disc_erase(p.x, p.y, radius)
                } else {
                    tiles.stamp_disc(p.x, p.y, radius, color)
                };
                if touched > 0 {
                    painted = true;
                }
            }
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
        if !self.vector_mode && !self.active_layer_accepts_paint() {
            return;
        }
        if self.vector_mode {
            if let Some(item) = vector::item_from_shape(shape, self.ink_rgba()) {
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
        let color = self.ink_rgba();

        let mut painted = false;
        if let Some(tiles) = self.layers.get_mut(active).and_then(|l| l.tiles_mut()) {
            let touched = tiles.paint_rect(rect, |px, py, dst| {
                let coverage = shape.coverage(px as f32 + 0.5, py as f32 + 0.5);
                if coverage <= 0.0 {
                    return None;
                }
                let mut rgba = color;
                rgba[3] = ((color[3] as f32) * coverage) as u8;
                if rgba[3] == 0 {
                    return None;
                }
                Some(blend_over(dst, rgba))
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
        let selection_shape = match shape.tool {
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
        self.selection = Some(Selection {
            shape: selection_shape,
        });
    }

    fn commit_lasso_selection(&mut self) {
        self.stroke_active = false;
        let points: Vec<(f32, f32)> = std::mem::take(&mut self.stroke_points)
            .into_iter()
            .map(|p| (p.x, p.y))
            .collect();
        if points.len() < 3 {
            return;
        }
        self.selection = Some(Selection {
            shape: SelectionShape::Lasso { points },
        });
    }

    pub fn deselect(&mut self) {
        self.exit_transform();
        self.commit_text();
        self.selection = None;
    }

    /// The one place the ink colour changes, so a text layer being typed into recolours with
    /// it instead of the shell having to know that text is special.
    pub fn set_color(&mut self, color: [u8; 4]) {
        self.color = color;
        self.apply_ink_to_text();
    }

    pub fn set_ink_opacity(&mut self, opacity: f32) {
        self.ink_opacity = opacity.clamp(INK_OPACITY_MIN, INK_OPACITY_MAX);
    }

    pub fn ink_rgba(&self) -> [u8; 4] {
        let mut rgba = self.color;
        let alpha = (self.color[3] as f32) * self.ink_opacity;
        rgba[3] = alpha.round().clamp(0.0, 255.0) as u8;
        rgba
    }

    fn commit_fill(&mut self, doc_x: f32, doc_y: f32) {
        if !self.active_layer_accepts_paint() {
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
                FILL_TOLERANCE_DEFAULT,
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
            if layer.tiles().is_none() && layer.content.items().is_none() {
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

    pub fn sample_color(&self, doc_x: f32, doc_y: f32) -> Option<[u8; 4]> {
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

    fn composite_pixel(&self, doc_x: f32, doc_y: f32) -> [u8; 4] {
        let mut acc = [0u8; 4];
        for layer in &self.layers {
            if !layer.visible {
                continue;
            }
            if layer.tiles().is_none() && layer.content.items().is_none() {
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
            px.copy_from_slice(&self.composite_pixel(doc_x, doc_y));
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
            let Some(raw) = layer.opaque_pixel_bounds() else {
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
        if layer.tiles().is_none() && layer.content.items().is_none() {
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
        let items = layer.content.items()?;
        let mut svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">",
            self.width, self.height, self.width, self.height
        );
        if let Some(group) = crate::vector::svg_transform_attr(items, layer.transform) {
            svg.push_str(&group);
        }
        for item in items {
            if let Some(markup) = crate::vector::item_svg(item) {
                svg.push_str(&markup);
            }
        }
        if layer.transform.is_some_and(|t| !t.is_identity()) {
            svg.push_str("</g>");
        }
        svg.push_str("</svg>");
        Some(svg)
    }

    pub fn selection_rgba(&self) -> Option<(u32, u32, Vec<u8>)> {
        let selection = self.selection.as_ref()?;
        let bounds = selection.bounds().intersect(self.bounds())?;
        let tiles = self.layers.get(self.active_layer)?.tiles()?;
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
                    let px = tiles.get_pixel(doc_x, doc_y);
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

    pub fn paste_image_as_layer(
        &mut self,
        name: impl Into<String>,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> bool {
        let expected = (width as usize) * (height as usize) * 4;
        if width == 0 || height == 0 || rgba.len() < expected {
            return false;
        }
        let (ox, oy) = self
            .selection
            .as_ref()
            .map(|s| {
                let b = s.bounds();
                (b.min_x, b.min_y)
            })
            .unwrap_or((0, 0));
        self.add_layer(name);
        let Some(tiles) = self.active_mut().and_then(|l| l.tiles_mut()) else {
            return false;
        };
        tiles.blit_rgba_at(rgba, width, height, ox, oy) > 0
    }

    pub fn undo(&mut self) -> bool {
        self.commit_text();
        let mut active = self.active_layer;
        let changed = self.history.undo(&mut self.layers, &mut active);
        self.active_layer = active.min(self.layers.len().saturating_sub(1));
        changed
    }

    pub fn redo(&mut self) -> bool {
        self.commit_text();
        let mut active = self.active_layer;
        let changed = self.history.redo(&mut self.layers, &mut active);
        self.active_layer = active.min(self.layers.len().saturating_sub(1));
        changed
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
    /// This deliberately uses `opaque_bounds` rather than the `content_bounds` the hover
    /// outline draws from. `content_bounds` is tile-granular, so a ten-pixel stroke reports a
    /// 256-pixel box — fine for an outline the eye reads as approximate, useless as a number
    /// in a panel and actively wrong as the thing a crop is measured against. The two can
    /// disagree by up to a tile; the readout is the exact one.
    pub fn layer_bounds(&self, index: usize) -> Option<(f32, f32, f32, f32)> {
        let layer = self.layers.get(index)?;
        let raw = match layer.tiles() {
            Some(grid) => {
                let r = grid.opaque_bounds()?;
                (
                    r.min_x as f32,
                    r.min_y as f32,
                    (r.max_x + 1) as f32,
                    (r.max_y + 1) as f32,
                )
            }
            None => layer.content_bounds()?,
        };
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
            let corners = self.layer_outline_corners(drag.layer_index)?;
            return Some((drag.layer_index, corners));
        }
        let index = self.hover_layer?;
        let corners = self.layer_outline_corners(index)?;
        Some((index, corners))
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

    /// Whether something on the board is mid-change and needs a frame per display refresh.
    /// A hovered layer deliberately does not count: its outline is a static overlay that
    /// `set_hover_layer` already invalidates once on the way in and once on the way out.
    /// Counting it here pinned `frame_dirty` to `Content` for as long as the cursor sat on a
    /// layer row — re-syncing every tile every frame — and forced the overview proxy off on
    /// exactly the documents that are too large to draw the full way.
    pub fn has_live_preview(&self) -> bool {
        self.stroke_active
            || self.shape_drag.is_some()
            || self.selection.is_some()
            || self.transform_active
            || self.transform_drag.is_some()
            || self.vector_drag.is_some()
            || self.text_edit.is_some()
    }

    pub fn tile_size(&self) -> u32 {
        TILE_SIZE
    }
}

/// `outer \ inner` as up to four non-overlapping bands. Cropping clears these rather than
/// rewriting the whole layer, so the cost is the discarded margin and not the picture.
fn outside_bands(outer: DocRect, inner: DocRect) -> Vec<DocRect> {
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
