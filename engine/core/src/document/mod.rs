use crate::brush::{Brush, BrushProfile};
use crate::camera::Camera;
use crate::coverage::CoverageGrid;
use crate::crop_edit::CropDrag;
use crate::crop_overlay::CropOverlayStyle;
use crate::filters::AdjustmentLut;
use crate::guide::{Guide, GuideDrag};
use crate::history::{History, TileSnapshot};
use crate::layer::Layer;
use crate::limits::{
    ALPHA_MAX, ALPHA_OPAQUE, BLUR_STRENGTH_DEFAULT, BLUR_STRENGTH_MAX, BLUR_STRENGTH_MIN,
    BRUSH_SIZE_DEFAULT, CLONE_ALIGNED_DEFAULT, DEFAULT_INK, EFFECT_CHUNK_BYTES,
    ERASER_HARDNESS_DEFAULT, ERASER_HARDNESS_MAX, ERASER_HARDNESS_MIN, EYEDROPPER_RADIUS_DEFAULT,
    EYEDROPPER_RADIUS_MAX, EYEDROPPER_RADIUS_MIN, INK_OPACITY_DEFAULT, INK_OPACITY_MAX,
    INK_OPACITY_MIN, MAX_CANVAS_SIDE, MIN_CANVAS_SIDE, MIN_STAMP_SPACING,
    MIN_STROKE_POINT_DISTANCE, PAPER_WHITE, STAMP_COVERAGE_PADDING, STAMP_SPACING_RATIO,
    STROKE_POINT_CAPACITY, TOLERANCE_DEFAULT, TOLERANCE_MAX, TOLERANCE_MIN,
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

mod background;
mod canvas;
mod commit;
mod flatten;
mod layers;
mod stroke;
mod transform_drag;

pub(crate) use canvas::outside_bands;
pub(crate) use flatten::{
    apply_layer_effects, copy_layer_into_rgba, layer_alpha_at, layer_source_pixel,
};
use stroke::tiles_covering;
pub use stroke::{stamp_spacing, stroke_stamps, StrokePoint};
pub use transform_drag::TransformHandles;
pub(crate) use transform_drag::{
    point_dist, TransformDrag, TransformHandle, TransformTarget, HANDLE_HIT_RADIUS_PX,
};

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
    /// `⌥`, read the same live way as `shift_held`: the clone stamp and the healing brush use
    /// it to tell an anchor click apart from an ordinary paint press on the same tool.
    pub alt_held: bool,
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
    /// The active selection, snapshotted once when a live-committing stroke (blur, clone,
    /// heal) begins rather than re-cloned on every pointer event. `SelectionMask.bits` is a
    /// raw, un-`Arc`'d buffer sized to the document, so cloning it per pointer-move — as every
    /// other event on a dragged stroke does — turned a full-canvas lasso or wand selection into
    /// a fresh multi-megabyte copy every frame of the drag.
    stroke_selection: Option<Selection>,
    /// How far each pixel the blur brush passes over travels toward its blurred neighbourhood.
    /// A document-level knob like `brush_size`, not a shell one — see `blur.rs`.
    pub blur_strength: f32,
    /// Whether the clone stamp's / healing brush's source offset survives to the next stroke.
    /// A shell knob like `blur_strength`, shared by both tools since they read one
    /// `CloneSource`.
    pub clone_aligned: bool,
    /// Where the clone stamp / healing brush reads from — set by an `⌥`-click
    /// (`Document::set_clone_anchor`), `None` until the first one. Not a shell knob: unlike
    /// `clone_aligned`, a click position is state the engine owns outright.
    clone_source: Option<CloneSource>,
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
    /// How many of the current stroke's stamps the live-committing brushes (blur, clone, heal)
    /// have already applied. None of the three has an ink preview, so they paint as the pointer
    /// moves; this is what stops each event re-applying the whole stroke from the start.
    live_stamp_progress: usize,
    /// Whether the current live-committing stroke has actually changed a pixel. A stroke that
    /// touched nothing — blur strength at zero, or dragged across empty space — must not leave
    /// an undo entry behind, and the snapshot alone cannot tell the difference.
    live_stamp_painted: bool,
    /// The crop/expand rectangle being dragged, in document space, or `None` when `Tool::Crop`
    /// is not active. Set to the full canvas on entry (`enter_crop`) and cleared on exit or
    /// commit — see `crop_edit.rs`.
    pub(crate) crop_rect: Option<(f32, f32, f32, f32)>,
    pub(crate) crop_drag: Option<CropDrag>,
    /// The camera as it was just before the current Crop session zoomed out to make room for
    /// dragging a handle past the canvas edge, or `None` outside Crop. Restored on exit (or
    /// re-armed after a commit) so leaving Crop doesn't strand the board at that zoom — see
    /// `crop_edit.rs`.
    pub(crate) crop_saved_camera: Option<Camera>,
    /// `width / height` the crop rect is locked to, or `None` for a free-form drag. A shell
    /// knob, like `vector_mode` — the user's choice, not state the engine derives.
    pub crop_aspect_lock: Option<f32>,
    /// Which composition guide the crop overlay draws while dragging. A shell knob.
    pub crop_overlay_style: CropOverlayStyle,
    /// Layer id whose background is being removed, while that job runs off the engine lock.
    /// The renderer sweeps this layer until the result lands or the job is dropped.
    background_removal: Option<String>,
}

/// Where the clone stamp / healing brush reads from. `offset` is `anchor − first destination
/// point`, fixed once a stroke starts and cleared again — forcing a recompute from `anchor` on
/// the next one — whenever `⌥` sets a fresh anchor or `clone_aligned` is off (`begin_stroke`).
/// Shared by both tools rather than duplicated: they differ only in what they do with the pixels
/// this locates, never in how the source is tracked.
#[derive(Clone, Copy, Debug, PartialEq)]
struct CloneSource {
    anchor: (f32, f32),
    offset: Option<(f32, f32)>,
}

/// A color with the ink-opacity slider folded into its alpha. The one place that happens,
/// so a fill and its stroke cannot disagree about how translucent the shape is.
fn glazed(color: [u8; 4], opacity: f32) -> [u8; 4] {
    let mut rgba = color;
    rgba[3] = ((color[3] as f32) * opacity).round().clamp(0.0, 255.0) as u8;
    rgba
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
            alt_held: false,
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
            stroke_selection: None,
            blur_strength: BLUR_STRENGTH_DEFAULT,
            clone_aligned: CLONE_ALIGNED_DEFAULT,
            clone_source: None,
            tolerance: TOLERANCE_DEFAULT,
            select_color: DEFAULT_INK,
            eyedropper_radius: EYEDROPPER_RADIUS_DEFAULT,
            brush: Brush::default(),
            eraser_hardness: ERASER_HARDNESS_DEFAULT,
            live_stamp_progress: 0,
            live_stamp_painted: false,
            crop_rect: None,
            crop_drag: None,
            crop_saved_camera: None,
            crop_aspect_lock: None,
            crop_overlay_style: CropOverlayStyle::RuleOfThirds,
            background_removal: None,
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

    /// Whether the active layer can take pixels, for the commands that are not a tool press —
    /// paste, clear, clear-selection. Anything reached for *through a tool* asks `tool_block`
    /// instead, so it gets a reason it can show rather than a bare `false`.
    pub fn active_layer_accepts_paint(&self) -> bool {
        self.layers
            .get(self.active_layer)
            .is_some_and(accepts_pixels)
            && !self.clip_pair_locked(self.active_layer)
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

    pub fn set_alt_held(&mut self, held: bool) {
        self.alt_held = held;
    }

    /// The shape as it will be drawn and committed: the live drag with the Shift constraint
    /// applied. Deriving it here rather than storing it clamped keeps one source of truth —
    /// the raw drag — so the modifier can be pressed and released as often as the user likes
    /// and the answer is always current.
    pub fn preview_shape(&self) -> Option<Shape> {
        let mut shape = self.shape_drag?;
        shape.start = self.snap_doc_point(shape.start);
        shape.end = self.snap_doc_point(shape.end);
        if let Some(constraint) = shape.tool.shift_constraint().filter(|_| self.shift_held) {
            shape.end = constraint.apply(shape.start, shape.end);
        }
        Some(shape)
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
        if self.tool == Tool::Crop {
            self.begin_crop_drag(dx, dy);
            return;
        }
        let on_paper = self.screen_on_paper(screen_x, screen_y);
        if self.tool == Tool::Move || !on_paper {
            self.commit_text();
            if self.begin_guide_drag(screen_x, screen_y) {
                return;
            }
        }
        if self.tool == Tool::Move {
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
        // `⌥`-click sets where the clone stamp / healing brush reads from, rather than
        // painting — the one gesture that means something different on these two tools than
        // the plain press every other stroke tool takes.
        if self.alt_held && matches!(self.tool, Tool::Clone | Tool::Heal) {
            self.set_clone_anchor(dx, dy);
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
            self.live_stamp_pending();
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
        if self.update_guide_drag(screen_x, screen_y) {
            return false;
        }
        if self.transform_active {
            if !self.update_vector_item_drag(dx, dy) {
                self.update_transform_drag(dx, dy);
            }
            return true;
        }
        if self.tool == Tool::Crop {
            self.update_crop_drag(dx, dy);
            return true;
        }
        if self.tool == Tool::Move {
            self.update_move_drag(dx, dy);
            return true;
        }
        if self.tool == Tool::Text {
            return self.text_pointer_move(dx, dy);
        }
        if self.tool.is_stroke() && self.stroke_active {
            self.push_stroke_point(dx, dy);
            return self.live_stamp_pending();
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
        self.pointer_hover = Some((dx, dy));
        if self.end_guide_drag() {
            return;
        }
        if self.transform_active {
            self.commit_vector_drag_history();
            self.commit_transform_drag_history();
            return;
        }
        if self.tool == Tool::Crop {
            self.end_crop_drag();
            return;
        }
        if self.tool == Tool::Move {
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

    pub fn clear_layer_dirty(&mut self, channel: DirtyChannel) {
        for layer in &mut self.layers {
            layer.clear_dirty(channel);
        }
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
    /// and both draw static overlays; counting them would re-sync every tile at display rate
    /// for as long as the mode was open. Every `Engine` call that touches either one already
    /// calls `Renderer::invalidate`, which is the one frame they need.
    pub fn has_live_preview(&self) -> bool {
        self.stroke_active
            || self.shape_drag.is_some()
            || self.transform_drag.is_some()
            || self.vector_drag.is_some()
            || self.guide_drag.is_some()
            || self.crop_drag.is_some()
    }

    /// Whether an overlay is *animating* and so needs a frame per display refresh even though
    /// nothing about the document changed. The text caret blinks off the renderer's clock, and
    /// a background-removal sweep crosses the layer while that job runs. Both ask for an overlay
    /// frame, not a content one: the tiles, the draw list and the pan cache stay valid, so the
    /// frame is one instance-buffer write. The caret is a square wave and the sweep is not.
    pub fn has_animated_overlay(&self) -> bool {
        self.text_edit.is_some() || self.background_removal.is_some()
    }
}
