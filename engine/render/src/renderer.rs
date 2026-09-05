use crate::compose::{
    brush_params, brush_ring_instances, clone_source_overlay_instances, composited_tile_payload,
    crop_overlay_instances, guide_instances, layer_highlight_instances, rgba_unit,
    selection_lasso_points, selection_mask_edges, selection_rect_or_ellipse, stroke_instances,
    stroke_instances_from, stroke_segment_count, text_caret_visible, text_overlay_instances,
    tile_upload_mips, transform_overlay_instances, GuideInstance, StrokeInstance,
};
use crate::desk::DeskLattice;
use crate::framebuffer::{self, PanCache, PxRect};
use crate::overview::OverviewPass;
use crate::stroke_coverage::StrokeCoverage;
use crate::tile_atlas::{SharedBindings, TileAtlas, TileSamplers};
use crate::vector_draw::{
    item_visible, push_path_instances, shape_instance, vector_placement,
    vector_selection_instances, VectorShapeInstance,
};
use bytemuck::{Pod, Zeroable};
use calumma_core::limits::{
    CAMERA_MOTION_IDLE_FRAMES, CRISP_PIXEL_ZOOM, FRAME_HINT_IDLE_FPS, GUIDES_LIMIT,
    LAYER_DATA_CAPACITY, STROKE_INSTANCE_CAPACITY, SURFACE_FRAME_LATENCY, TILE_ATLAS_MAX_CAPACITY,
    TILE_INSTANCE_CAPACITY, VECTOR_SHAPE_INSTANCE_CAPACITY,
};
use calumma_core::tile::{DirtyChannel, TileCoord, TileGrid};
use calumma_core::{
    BlendMode, BrushProfile, DeviceTier, Document, GpuBudget, GpuKind, MemoryPressureLevel, Tool,
    VectorItem,
};
use rayon::prelude::*;
use rustc_hash::FxHashSet;
use std::collections::HashMap;
use std::num::NonZeroU64;
use std::sync::Arc;
use std::time::Instant;

type TileKey = (u32, i32, i32);

/// One tile's upload, as the parallel bake hands it to the sequential wgpu loop: the baked
/// base level when the layer needed one (mask, adjustments or opacity), and the mip chain above
/// it. `None` for the base means nothing was baked, so the upload reads the tile's own `Arc`
/// and the tile may share an atlas slot with its siblings.
type TilePayload = (Option<Vec<u8>>, Vec<Vec<u8>>);

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct PaperUniforms {
    pub(crate) pan: [f32; 2],
    pub(crate) zoom: f32,
    pub(crate) dpr: f32,
    pub(crate) doc_size: [f32; 2],
    pub(crate) viewport: [f32; 2],
    pub(crate) dark: f32,
    /// Side of one period of the baked desk lattice in device texels, or 0 to put `fs_paper`
    /// back on evaluating the grid per pixel — see `crate::desk`.
    pub(crate) lattice_side: f32,
    pub(crate) _pad1: f32,
    pub(crate) _pad2: f32,
    /// `DeskMetrics` as the shader wants it — see `calumma_core::DeskMetrics` for why the
    /// squared paper is measured in Rust rather than in `board.wgsl`.
    pub(crate) desk_metrics: [f32; 4],
    pub(crate) desk: [f32; 4],
    pub(crate) grid: [f32; 4],
    pub(crate) paper_border: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TileCamera {
    pan: [f32; 2],
    zoom: f32,
    dpr: f32,
    viewport: [f32; 2],
    doc_size: [f32; 2],
    crisp: f32,
    _pad: [f32; 3],
}

/// One visible tile: where it sits in document space, which array layer of the shared atlas
/// holds its pixels, and which row of the layer table it is transformed by. This is the whole
/// per-tile payload — everything else a tile draw needs (camera, the atlas, every layer's
/// transform) is bound once for the entire board.
///
/// `layer_index` replaced what used to be padding, so carrying it costs nothing: the instance
/// was already 16 bytes.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TileInstance {
    origin: [f32; 2],
    slot: u32,
    layer_index: u32,
}

/// `LayerData.lut_mode` values — mirrored exactly in `apply_adjustments` (`board.wgsl`).
const LUT_MODE_IDENTITY: u32 = 0;
const LUT_MODE_TONE: u32 = 1;
const LUT_MODE_TONE_HSL: u32 = 2;

/// One row of the layer table the tile shader indexes — `LayerData` in `board.wgsl`, and the
/// two must agree byte for byte. Row *i* is `doc.layers[i]`, so a tile instance addresses its
/// layer by stack position and no side table is needed to resolve it.
///
/// This replaced a per-layer uniform buffer *and* a per-layer bind group. The win is not the
/// bytes — it is that a stack of Normal layers now draws with one `set_bind_group` for the
/// whole board instead of a rebind between every layer's instanced draw.
///
/// 1072 bytes: the original 32-byte transform block plus opacity, `atlas_slot`, `lut_mode` and
/// the adjustment LUT (`tone` — the 256-entry per-channel table `AdjustmentLut` already builds
/// on the CPU — plus `sat`/`vib`, since saturation and vibrance couple all three channels through
/// HSL and cannot ride a table). No tail padding: every field after the three `vec2<f32>`s is
/// 4-byte aligned in both Rust and WGSL, and 1072 is already a multiple of 8, the struct's own
/// alignment (from `pivot`/`offset`/`scale`). See `layer_table_tests::a_table_row_is_the_size_
/// the_shader_strides_by` — the one thing enforcing that this and `LayerData` in `board.wgsl`
/// agree byte for byte.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LayerData {
    pivot: [f32; 2],
    offset: [f32; 2],
    scale: [f32; 2],
    rotation: f32,
    opacity: f32,
    /// Read only by the solid-Paper quad, which has no instance buffer to carry it. `0` for
    /// every other layer, and unread there.
    atlas_slot: u32,
    /// `LUT_MODE_IDENTITY` / `LUT_MODE_TONE` / `LUT_MODE_TONE_HSL` — which of `tone`/`sat`/`vib`
    /// `fs_tile`'s `apply_adjustments` needs to read, so a neutral or tone-only layer skips the
    /// HSL round trip entirely.
    lut_mode: u32,
    /// `AdjustmentLut`'s own `tone` table, byte-indexed and channel-agnostic (gamma → contrast →
    /// brightness depends only on the input byte, not which channel it came from) — the exact
    /// values the CPU path already computes, just read by the shader instead of applied to
    /// tile bytes before upload.
    tone: [f32; 256],
    saturation: f32,
    vibrance: f32,
}

impl Default for LayerData {
    fn default() -> Self {
        Self {
            pivot: [0.0, 0.0],
            offset: [0.0, 0.0],
            scale: [1.0, 1.0],
            rotation: 0.0,
            opacity: 1.0,
            atlas_slot: 0,
            lut_mode: LUT_MODE_IDENTITY,
            // Unread while `lut_mode` is identity — see `apply_adjustments` in board.wgsl.
            tone: [0.0; 256],
            saturation: 0.0,
            vibrance: 0.0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct PreviewUniforms {
    pub(crate) pan: [f32; 2],
    pub(crate) zoom: f32,
    pub(crate) dpr: f32,
    pub(crate) viewport: [f32; 2],
    pub(crate) _align_color: [f32; 2],
    pub(crate) color: [f32; 4],
    pub(crate) p0: [f32; 2],
    pub(crate) p1: [f32; 2],
    pub(crate) half_width: f32,
    pub(crate) tool: f32,
    pub(crate) fill: f32,
    pub(crate) shape_stroke: f32,
    pub(crate) stroke_ink: [f32; 4],
    pub(crate) shape_stroke_color: [f32; 4],
}

/// A tile GPU-resident in the shared atlas: just which array layer holds it. Texture and
/// bind group both moved to `TileAtlas`, shared across every tile of every layer.
struct GpuTile {
    array_layer: u32,
}

/// What the live brush stroke's coverage target already holds, so the next frame can union the
/// segments the pointer actually travelled onto it instead of rasterizing the whole stroke
/// again (`StrokeCoverage::accumulate`'s append contract).
///
/// Every field beyond `segments` is part of the cache key rather than payload: the coverage
/// pass rasterizes in *device* pixels off the preview uniform, so a camera that moved
/// underneath the accumulated pixels invalidates them wholesale, and the brush params and color
/// are baked into each capsule at the width it was drawn.
#[derive(Clone, Copy, PartialEq)]
struct CoverageProgress {
    generation: u64,
    points: usize,
    pan: (f32, f32),
    zoom: f32,
    dpr: f32,
    brush: [f32; 4],
    color: [f32; 4],
}

impl CoverageProgress {
    /// Whether `next` may be unioned onto what `self` left in the target.
    ///
    /// `points >= 2` is the one non-obvious condition: `stroke_segment_count` maps a lone point
    /// to *one* segment — the degenerate capsule that makes a tap leave a dot — and segment 0
    /// then **replaces** it when the second point arrives rather than following it. Appending
    /// across that boundary would emit nothing and leave the dot standing in for the first
    /// capsule, so a one-point stroke restarts.
    fn appendable(&self, next: &Self) -> bool {
        self.generation == next.generation
            && self.points >= 2
            && next.points >= self.points
            && self.pan == next.pan
            && self.zoom == next.zoom
            && self.dpr == next.dpr
            && self.brush == next.brush
            && self.color == next.color
    }
}

/// Which instance buffer a vector run draws from: parametric shapes evaluate an SDF per
/// pixel, freehand paths are stroke segments.
#[derive(Clone, Copy, PartialEq, Eq)]
enum VectorRun {
    Shapes,
    Paths,
}

/// One entry of the board's draw list, in stack order. Vector layers used to be drawn before
/// every tile — which put them under Paper, where nothing could be seen — so the list is
/// built across *all* layers and replayed once: a vector layer above a paint layer covers it,
/// exactly as the flattened composite already had it.
enum LayerDraw {
    /// A run of one document layer's visible tiles, as a range into the shared tile-instance
    /// buffer — one instanced draw regardless of how many tiles that is. Nothing else is needed
    /// at draw time: each instance carries the layer row it reads, so the draw does not have to
    /// name its layer at all.
    Tiles(BlendMode, std::ops::Range<u32>),
    /// Paper collapsed to one full-document quad. The layer's table row travels as the
    /// one-instance draw range, which is where `vs_doc_quad` reads its `instance_index` from.
    Solid(BlendMode, u32),
    Vector(VectorRun, std::ops::Range<u32>),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FrameDirty {
    Clean,
    Camera,
    /// Only the overlay layer changed — the hover outline, most often. Tiles, the draw list and
    /// the overview are all still valid, so the frame skips straight to rebuilding overlay
    /// instances. Treating this as `Content` meant every row the cursor crossed while scrolling
    /// the layers panel cost a full tile resync and draw-list rebuild.
    Overlay,
    Content,
}

enum FrameOutput {
    Surface(wgpu::Surface<'static>),
    #[cfg(test)]
    Headless(wgpu::Texture),
}

impl FrameOutput {
    #[cfg(test)]
    fn headless_texture(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("headless-frame"),
            size: wgpu::Extent3d {
                width: config.width.max(1),
                height: config.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
    }

    fn configure(&mut self, device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) {
        match self {
            Self::Surface(surface) => surface.configure(device, config),
            #[cfg(test)]
            Self::Headless(texture) => {
                *texture = Self::headless_texture(device, config);
            }
        }
    }
}

enum AcquiredFrame {
    Surface(wgpu::SurfaceTexture),
    #[cfg(test)]
    Headless,
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    output: FrameOutput,
    config: wgpu::SurfaceConfiguration,
    paper_pipeline: wgpu::RenderPipeline,
    tile_pipeline_normal: wgpu::RenderPipeline,
    tile_pipeline_multiply: wgpu::RenderPipeline,
    tile_pipeline_screen: wgpu::RenderPipeline,
    solid_pipeline_normal: wgpu::RenderPipeline,
    solid_pipeline_multiply: wgpu::RenderPipeline,
    solid_pipeline_screen: wgpu::RenderPipeline,
    stroke_pipeline: wgpu::RenderPipeline,
    overlay_pipeline: wgpu::RenderPipeline,
    guide_pipeline: wgpu::RenderPipeline,
    stroke_coverage: StrokeCoverage,
    shape_pipeline: wgpu::RenderPipeline,
    vector_shape_pipeline: wgpu::RenderPipeline,
    paper_buf: wgpu::Buffer,
    paper_bgl: wgpu::BindGroupLayout,
    paper_bg: wgpu::BindGroup,
    desk_lattice: DeskLattice,
    tile_shared_bgl: wgpu::BindGroupLayout,
    tile_camera_buf: wgpu::Buffer,
    layer_data_buf: wgpu::Buffer,
    layer_data_capacity: usize,
    layer_data_scratch: Vec<LayerData>,
    preview_buf: wgpu::Buffer,
    preview_bg: wgpu::BindGroup,
    stroke_buf: wgpu::Buffer,
    stroke_capacity: usize,
    guide_buf: wgpu::Buffer,
    guide_scratch: Vec<GuideInstance>,
    vector_shape_buf: wgpu::Buffer,
    vector_shape_capacity: usize,
    tile_instance_buf: wgpu::Buffer,
    tile_instance_capacity: usize,
    samplers: TileSamplers,
    atlas: TileAtlas,
    budget: GpuBudget,
    /// Whether every visible tile is already in the atlas, from the last frame that asked. The
    /// walk behind it is one hash lookup per candidate tile per layer — 3µs at one layer, 34µs
    /// at ten on a fit-to-view 8K board — and it is only *reached* on a frame where nothing
    /// dirtied content and the tile span did not move, which is exactly the frame where last
    /// frame's answer is still the answer. `None` means "ask again".
    visible_upload_needed: Option<bool>,
    tiles: HashMap<TileKey, GpuTile>,
    layer_slots: HashMap<String, u32>,
    next_layer_slot: u32,
    started: Instant,
    frame_dirty: FrameDirty,
    cached_retained_span: Option<(i32, i32, i32, i32)>,
    cached_visible_span: Option<(i32, i32, i32, i32)>,
    cached_tile_instances: Vec<TileInstance>,
    cached_strokes: Vec<StrokeInstance>,
    overlay_scratch: Vec<StrokeInstance>,
    screen_overlay_scratch: Vec<StrokeInstance>,
    cached_shapes: Vec<VectorShapeInstance>,
    cached_draws: Vec<LayerDraw>,
    overview: OverviewPass,
    camera_motion: bool,
    motion_idle_frames: u32,
    cached_tile_draw_count: Option<usize>,
    /// Tiles uploaded during camera motion, which skips the mip chain to keep a pan cheap and
    /// writes only level 0. Their remaining levels are whatever the atlas slot happened to
    /// hold, so they have to be uploaded again in full once the camera settles — otherwise
    /// zooming out far enough samples a level nobody ever wrote and the layer fades out.
    base_only_tiles: FxHashSet<TileKey>,
    pan_cache: PanCache,
    coverage_progress: Option<CoverageProgress>,
    /// Which half of the blink the caret was in on the last frame actually drawn, or `None` when
    /// there was no caret. A text session is the only thing that asks for a frame with nothing
    /// about the document changing, and it asks twice a second — so the frame loop draws when
    /// this answer moves rather than at display rate for as long as the session is open.
    drawn_caret_phase: Option<bool>,
}

mod cache;
mod camera_motion;
mod frame;
mod invalidation;
mod pipeline;

// Re-exported at the old path so `desk.rs`/`framebuffer.rs`/`stroke_coverage.rs` (siblings of
// `renderer` in the crate, not descendants) don't have to know the pipeline split happened.
pub(crate) use pipeline::{paper_bind_group, PREMULTIPLIED_ALPHA_COMPONENT};
// `stroke_coverage.rs`'s tests are this re-export's only consumer.
#[cfg(test)]
pub(crate) use pipeline::STROKE_ATTRS;
