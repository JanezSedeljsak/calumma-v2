use crate::compose::{
    brush_params, brush_ring_instances, composited_tile_payload, guide_instances,
    layer_highlight_instances, rgba_unit, selection_lasso_points, selection_mask_edges,
    selection_rect_or_ellipse, stroke_instances, stroke_instances_from, stroke_segment_count,
    text_caret_visible, text_overlay_instances, tile_upload_mips, transform_overlay_instances,
    GuideInstance, StrokeInstance,
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

#[derive(Clone, Copy, PartialEq, Eq)]
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

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
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

impl Renderer {
    pub fn from_surface(
        surface: wgpu::Surface<'static>,
        instance: &wgpu::Instance,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        // `HighPerformance` deliberately unchanged. On a dual-GPU Intel Mac it forces the
        // discrete part for a workload whose hot path is CPU→GPU tile uploads — free on unified
        // memory, a bus copy on a discrete one — so `LowPower` there is arguable. It is only
        // arguable: the integrated part it would pick instead is genuinely weaker on fill rate,
        // and this is not measurable on the machine the app is developed on. Deciding it by
        // guess would make things worse on exactly the machines a low tier is meant to help.
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            ..Default::default()
        }))
        .map_err(|e| e.to_string())?;

        // The tile atlas wants as many array layers as the adapter will give it, up to our own
        // safety ceiling — a low-end/downlevel adapter reporting only the WebGPU baseline (256)
        // still works fine, it just evicts prefetch-margin tiles under pressure sooner.
        let adapter_array_layers = adapter.limits().max_texture_array_layers;
        let atlas_max_capacity = adapter_array_layers.min(TILE_ATLAS_MAX_CAPACITY);
        // Classified here rather than anywhere later because it decides how the *device* is
        // created, not just how the atlas is sized. The adapter is the only thing that ever
        // answers this; a tier is fixed for the life of the surface.
        let budget = GpuBudget::new(DeviceTier::classify(
            gpu_kind(adapter.get_info().device_type),
            adapter_array_layers,
        ));

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("calumma-render"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits {
                max_texture_array_layers: atlas_max_capacity,
                ..wgpu::Limits::default()
            },
            memory_hints: if budget.tier().prefers_small_allocations() {
                wgpu::MemoryHints::MemoryUsage
            } else {
                wgpu::MemoryHints::Performance
            },
            trace: wgpu::Trace::Off,
            ..Default::default()
        }))
        .map_err(|e| e.to_string())?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        // `Fifo` is the only mode that paces to the display, and the display link already
        // drives the frame loop, so there is nothing to gain from tearing. A previous
        // preference for `Mailbox` was dead code on the only shipping backend — wgpu's Metal
        // surface reports `Fifo` and `Immediate` and nothing else.
        let present_mode = caps
            .present_modes
            .iter()
            .copied()
            .find(|m| *m == wgpu::PresentMode::Fifo)
            .unwrap_or(caps.present_modes[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::default(),
            width: width.max(1),
            height: height.max(1),
            present_mode,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: SURFACE_FRAME_LATENCY,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("board"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/board.wgsl").into()),
        });

        let paper_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("paper-bgl"),
            entries: &[
                uniform_entry(0, std::mem::size_of::<PaperUniforms>()),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let desk_lattice = DeskLattice::new(&device, &queue, 1.0);
        let paper_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("paper-uniform"),
            size: std::mem::size_of::<PaperUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let paper_bg = paper_bind_group(&device, &paper_bgl, &paper_buf, &desk_lattice);
        let paper_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("paper-pl"),
            bind_group_layouts: &[Some(&paper_bgl)],
            ..Default::default()
        });
        let paper_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("paper"),
            layout: Some(&paper_pl),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_paper"),
                compilation_options: Default::default(),
                targets: &[Some(replace_target(format))],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let tile_shared_bgl = tile_shared_bgl(&device);
        let tile_camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tile-camera"),
            size: std::mem::size_of::<TileCamera>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layer_data_capacity = LAYER_DATA_CAPACITY;
        let layer_data_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("layer-data"),
            size: (layer_data_capacity * std::mem::size_of::<LayerData>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let samplers = TileSamplers::new(&device);
        let atlas = TileAtlas::new(
            &device,
            &SharedBindings {
                layout: &tile_shared_bgl,
                camera: &tile_camera_buf,
                layers: &layer_data_buf,
                samplers: &samplers,
            },
            atlas_max_capacity.min(budget.atlas_max_capacity()),
        );
        let tile_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tile-pl"),
            bind_group_layouts: &[Some(&tile_shared_bgl)],
            ..Default::default()
        });
        let tile_instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TileInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: TILE_INSTANCE_ATTRS,
        };
        let tile_pipeline_for = |label: &str, target: wgpu::ColorTargetState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&tile_pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_tile"),
                    compilation_options: Default::default(),
                    buffers: &[Some(tile_instance_layout.clone())],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_tile"),
                    compilation_options: Default::default(),
                    targets: &[Some(target)],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let tile_pipeline_normal = tile_pipeline_for("tile-normal", premultiplied_target(format));
        let tile_pipeline_multiply = tile_pipeline_for("tile-multiply", multiply_target(format));
        let tile_pipeline_screen = tile_pipeline_for("tile-screen", screen_target(format));

        let solid_pipeline_for = |label: &str, target: wgpu::ColorTargetState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&tile_pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_doc_quad"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_solid_tile"),
                    compilation_options: Default::default(),
                    targets: &[Some(target)],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let solid_pipeline_normal =
            solid_pipeline_for("solid-normal", premultiplied_target(format));
        let solid_pipeline_multiply = solid_pipeline_for("solid-multiply", multiply_target(format));
        let solid_pipeline_screen = solid_pipeline_for("solid-screen", screen_target(format));

        let preview_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("preview-bgl"),
            entries: &[uniform_entry(0, std::mem::size_of::<PreviewUniforms>())],
        });
        let preview_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("preview-uniform"),
            size: std::mem::size_of::<PreviewUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let preview_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("preview-bg"),
            layout: &preview_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: preview_buf.as_entire_binding(),
            }],
        });
        let preview_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("preview-pl"),
            bind_group_layouts: &[Some(&preview_bgl)],
            ..Default::default()
        });

        let stroke_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<StrokeInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: STROKE_ATTRS,
        };

        let stroke_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("stroke"),
            layout: Some(&preview_pl),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_stroke"),
                compilation_options: Default::default(),
                buffers: &[Some(stroke_layout)],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_stroke"),
                compilation_options: Default::default(),
                targets: &[Some(alpha_target(format))],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let overlay_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("overlay"),
            layout: Some(&preview_pl),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_overlay"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<StrokeInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: STROKE_ATTRS,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_overlay"),
                compilation_options: Default::default(),
                targets: &[Some(alpha_target(format))],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let guide_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("guide"),
            layout: Some(&preview_pl),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_guide"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GuideInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: GUIDE_ATTRS,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_guide"),
                compilation_options: Default::default(),
                targets: &[Some(alpha_target(format))],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let stroke_coverage = StrokeCoverage::new(
            &device,
            &shader,
            &preview_bgl,
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<StrokeInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: STROKE_ATTRS,
            },
            format,
        );

        let shape_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shape-preview"),
            layout: Some(&preview_pl),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_shape_preview"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_shape_preview"),
                compilation_options: Default::default(),
                targets: &[Some(alpha_target(format))],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let vector_shape_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("vector-shape"),
                layout: Some(&preview_pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_vector_shape"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<VectorShapeInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: VECTOR_SHAPE_ATTRS,
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_vector_shape"),
                    compilation_options: Default::default(),
                    targets: &[Some(alpha_target(format))],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });

        let stroke_capacity = STROKE_INSTANCE_CAPACITY;
        let stroke_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stroke-instances"),
            size: (stroke_capacity * std::mem::size_of::<StrokeInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Guides are capped at `GUIDES_LIMIT`, so this buffer is allocated once at its
        // worst case and never grows — unlike the stroke buffer, which follows the document.
        let guide_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("guide-instances"),
            size: (GUIDES_LIMIT * std::mem::size_of::<GuideInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let vector_shape_capacity = VECTOR_SHAPE_INSTANCE_CAPACITY;
        let vector_shape_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vector-shape-instances"),
            size: (vector_shape_capacity * std::mem::size_of::<VectorShapeInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let tile_instance_capacity = TILE_INSTANCE_CAPACITY;
        let tile_instance_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tile-instances"),
            size: (tile_instance_capacity * std::mem::size_of::<TileInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let overview = OverviewPass::new(&device, &shader, format);
        let pan_cache = PanCache::new(&device, &shader, format);

        Ok(Self {
            device,
            queue,
            surface,
            config,
            paper_pipeline,
            tile_pipeline_normal,
            tile_pipeline_multiply,
            tile_pipeline_screen,
            solid_pipeline_normal,
            solid_pipeline_multiply,
            solid_pipeline_screen,
            stroke_pipeline,
            overlay_pipeline,
            guide_pipeline,
            stroke_coverage,
            shape_pipeline,
            vector_shape_pipeline,
            paper_buf,
            paper_bgl,
            paper_bg,
            desk_lattice,
            tile_shared_bgl,
            tile_camera_buf,
            preview_buf,
            preview_bg,
            stroke_buf,
            guide_buf,
            guide_scratch: Vec::new(),
            stroke_capacity,
            vector_shape_buf,
            vector_shape_capacity,
            tile_instance_buf,
            tile_instance_capacity,
            layer_data_buf,
            layer_data_capacity,
            layer_data_scratch: Vec::new(),
            samplers,
            atlas,
            budget,
            visible_upload_needed: None,
            tiles: HashMap::new(),
            layer_slots: HashMap::new(),
            next_layer_slot: 0,
            started: Instant::now(),
            frame_dirty: FrameDirty::Content,
            cached_retained_span: None,
            cached_visible_span: None,
            cached_tile_instances: Vec::new(),
            cached_strokes: Vec::new(),
            overlay_scratch: Vec::new(),
            screen_overlay_scratch: Vec::new(),
            cached_shapes: Vec::new(),
            cached_draws: Vec::new(),
            overview,
            camera_motion: false,
            motion_idle_frames: 0,
            cached_tile_draw_count: None,
            base_only_tiles: FxHashSet::default(),
            pan_cache,
            coverage_progress: None,
            drawn_caret_phase: None,
        })
    }

    fn tile_pipeline(&self, mode: BlendMode) -> &wgpu::RenderPipeline {
        match mode {
            BlendMode::Normal => &self.tile_pipeline_normal,
            BlendMode::Multiply => &self.tile_pipeline_multiply,
            BlendMode::Screen => &self.tile_pipeline_screen,
        }
    }

    fn solid_pipeline(&self, mode: BlendMode) -> &wgpu::RenderPipeline {
        match mode {
            BlendMode::Normal => &self.solid_pipeline_normal,
            BlendMode::Multiply => &self.solid_pipeline_multiply,
            BlendMode::Screen => &self.solid_pipeline_screen,
        }
    }

    pub fn begin_camera_motion(&mut self) {
        self.motion_idle_frames = 0;
        self.camera_motion = true;
    }

    pub fn end_camera_motion(&mut self) {
        if !self.camera_motion {
            return;
        }
        self.camera_motion = false;
        self.motion_idle_frames = 0;
        self.cached_tile_draw_count = None;
        // Anything uploaded mid-gesture is still missing its mip chain. Ask for one more
        // content frame so `sync_tiles` can finish those tiles now that there is idle time.
        if !self.base_only_tiles.is_empty() {
            self.frame_dirty = FrameDirty::Content;
        }
    }

    fn tick_camera_motion(&mut self) {
        if !self.camera_motion {
            return;
        }
        self.motion_idle_frames += 1;
        if self.motion_idle_frames >= CAMERA_MOTION_IDLE_FRAMES {
            self.end_camera_motion();
        }
    }

    fn visible_span(doc: &Document) -> Option<(i32, i32, i32, i32)> {
        doc.visible_rect().map(|visible| visible.tile_span())
    }

    /// How often the board wants to be drawn from here, in frames per second, or
    /// [`FRAME_HINT_DISPLAY_MAX`] for "as fast as the display allows".
    ///
    /// Read once per frame by the shell, which owns nothing but the ceiling. Everything that
    /// makes this answer `DISPLAY_MAX` is a thing already in flight — a gesture, a camera still
    /// settling, a text session, or a frame the renderer has already marked dirty for itself.
    /// Anything else is a board sitting still, where the display link is the only cost left and
    /// there is no picture waiting on it.
    pub fn frame_hint(&self, doc: &Document) -> u32 {
        if self.camera_motion
            || self.frame_dirty != FrameDirty::Clean
            || doc.has_live_preview()
            || doc.has_animated_overlay()
        {
            return self.budget.frame_hint_ceiling();
        }
        FRAME_HINT_IDLE_FPS
    }

    pub fn request_overview_prewarm(&mut self) {
        self.overview.request_prewarm();
    }

    /// Takes the margin as a parameter, rather than reading `self.budget` directly, so
    /// it stays a pure function of `(doc, margin)` — callers thread the effective level's margin
    /// through, and tests can exercise it without a real `wgpu::Surface`-backed `Renderer`.
    fn retained_span(doc: &Document, margin: i32) -> Option<(i32, i32, i32, i32)> {
        doc.visible_rect()
            .map(|visible| visible.expanded_by_tiles(margin).tile_span())
    }

    fn clear_layer_cache(&mut self) {
        self.cached_retained_span = None;
        self.cached_visible_span = None;
        self.cached_tile_instances.clear();
        self.cached_strokes.clear();
        self.cached_shapes.clear();
        self.cached_draws.clear();
    }

    fn rebuild_layer_cache(&mut self, doc: &Document) {
        let mut tile_instances = Vec::new();
        let mut strokes = Vec::new();
        let mut shapes = Vec::new();
        let draws = self.build_layer_draws(doc, &mut tile_instances, &mut strokes, &mut shapes);
        self.cached_tile_instances = tile_instances;
        self.cached_strokes = strokes;
        self.cached_shapes = shapes;
        self.cached_draws = draws;
        self.cached_retained_span = Self::retained_span(doc, self.budget.retention_margin_tiles());
        self.cached_visible_span = Self::visible_span(doc);
    }

    /// [`Self::visible_needs_gpu_upload`], answered from the previous frame where that answer
    /// cannot have moved. Cleared by [`Self::invalidate`] and by `sync_tiles` — between them
    /// those are the only ways a visible tile stops being resident.
    fn visible_upload_needed(&mut self, doc: &Document) -> bool {
        if let Some(cached) = self.visible_upload_needed {
            return cached;
        }
        let needed = self.visible_needs_gpu_upload(doc);
        self.visible_upload_needed = Some(needed);
        needed
    }

    fn visible_needs_gpu_upload(&self, doc: &Document) -> bool {
        let Some(visible) = doc.visible_rect() else {
            return false;
        };
        for layer in &doc.layers {
            if !layer.visible {
                continue;
            }
            let Some(grid) = layer.tiles() else {
                continue;
            };
            let Some(slot) = self.layer_slots.get(&layer.id) else {
                return true;
            };
            if layer.is_paper() {
                if layer.tiles().is_some_and(|g| g.whole_tiles_share_one_arc()) {
                    let key: TileKey = (*slot, 0, 0);
                    if !self.tiles.contains_key(&key) {
                        return true;
                    }
                }
                continue;
            }
            for coord in grid.coords_intersecting(layer.doc_rect_to_grid(visible)) {
                let key: TileKey = (*slot, coord.x, coord.y);
                if !self.tiles.contains_key(&key) {
                    return true;
                }
            }
        }
        false
    }

    fn visible_tile_draw_count(&self, doc: &Document) -> usize {
        let Some(visible) = doc.visible_rect() else {
            return 0;
        };
        let mut count = 0;
        for layer in &doc.layers {
            if !layer.visible {
                continue;
            }
            let Some(grid) = layer.tiles() else {
                continue;
            };
            if layer.is_paper() && grid.whole_tiles_share_one_arc() {
                count += 1;
                continue;
            }
            count += grid
                .coords_intersecting(layer.doc_rect_to_grid(visible))
                .count();
        }
        count
    }

    fn layer_slot(&mut self, layer_id: &str) -> u32 {
        if let Some(slot) = self.layer_slots.get(layer_id) {
            return *slot;
        }
        let slot = self.next_layer_slot;
        self.next_layer_slot += 1;
        self.layer_slots.insert(layer_id.to_string(), slot);
        slot
    }

    /// Grows the layer table to hold `count` rows, rebinding group 0 if the buffer had to be
    /// replaced. Doubling, like the instance buffers, so a document that keeps gaining layers
    /// does not reallocate on every one.
    fn ensure_layer_data_capacity(&mut self, count: usize) {
        if count <= self.layer_data_capacity {
            return;
        }
        let mut next = self.layer_data_capacity.max(1);
        while next < count {
            next *= 2;
        }
        self.layer_data_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("layer-data"),
            size: (next * std::mem::size_of::<LayerData>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.layer_data_capacity = next;
        // The old buffer is still held by the atlas's bind group, which would keep reading it.
        self.atlas.rebuild_bind_group(
            &self.device,
            &SharedBindings {
                layout: &self.tile_shared_bgl,
                camera: &self.tile_camera_buf,
                layers: &self.layer_data_buf,
                samplers: &self.samplers,
            },
        );
    }

    /// Writes one table row per document layer, in stack order, as one buffer write.
    ///
    /// Every layer gets a row — vector layers and hidden ones included — so that a row index is
    /// simply a stack position and never has to be mapped through a side table. An unread row
    /// costs 1072 bytes; an index that means different things in different frames costs
    /// correctness, which is what the old per-layer-id lookup was quietly risking.
    ///
    /// Must run after `sync_tiles`: solid Paper's `atlas_slot` is only known once its tile has
    /// an atlas slot. Runs whenever the draw list is rebuilt, which is exactly when a transform,
    /// the stack, the visible span, opacity or an adjustment can have changed — a slider drag
    /// reaches this the same way a `⌘T` drag already does, by calling `Renderer::invalidate`.
    fn write_layer_data(&mut self, doc: &Document) {
        self.ensure_layer_data_capacity(doc.layers.len().max(1));
        let mut rows = std::mem::take(&mut self.layer_data_scratch);
        rows.clear();
        rows.reserve(doc.layers.len());
        for layer in &doc.layers {
            let mut row = match (layer.transform, layer.content_bounds()) {
                (Some(t), Some(bounds)) => LayerData {
                    pivot: [(bounds.0 + bounds.2) * 0.5, (bounds.1 + bounds.3) * 0.5],
                    offset: [t.offset_x, t.offset_y],
                    scale: [t.scale_x, t.scale_y],
                    rotation: t.rotation,
                    ..LayerData::default()
                },
                _ => LayerData::default(),
            };
            row.opacity = layer.opacity;
            if let Some(adjustments) = layer.adjustments {
                // `Document::set_layer_adjustments` already clears this to `None` for a neutral
                // result, but a fresh `AdjustmentLut` re-checks: cheaper than trusting a state
                // no type here enforces, and `is_neutral` is one struct-field compare.
                let lut = adjustments.lut();
                if !lut.is_neutral() {
                    row.tone = *lut.tone_table();
                    if lut.is_tone_only() {
                        row.lut_mode = LUT_MODE_TONE;
                    } else {
                        row.lut_mode = LUT_MODE_TONE_HSL;
                        row.saturation = adjustments.saturation;
                        row.vibrance = adjustments.vibrance;
                    }
                }
            }
            if let Some(slot) = self.solid_atlas_slot(layer) {
                row.atlas_slot = slot;
            }
            rows.push(row);
        }
        if !rows.is_empty() {
            self.queue
                .write_buffer(&self.layer_data_buf, 0, bytemuck::cast_slice(&rows));
        }
        self.layer_data_scratch = rows;
    }

    /// The atlas slot behind a Paper layer that has collapsed to one shared tile, or `None` for
    /// every layer that draws its tiles the ordinary way.
    fn solid_atlas_slot(&self, layer: &calumma_core::Layer) -> Option<u32> {
        if !layer.is_paper() {
            return None;
        }
        let grid = layer.tiles()?;
        if !grid.whole_tiles_share_one_arc() {
            return None;
        }
        let slot = *self.layer_slots.get(&layer.id)?;
        self.tiles.get(&(slot, 0, 0)).map(|gpu| gpu.array_layer)
    }

    pub fn cached_tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// GPU-side bytes reserved for the open document's tiles: the atlas's whole declared
    /// capacity (mip chain included), not just the tiles currently written — a `wgpu::Texture`
    /// array reserves storage for every layer it declares regardless of how many are in use, so
    /// capacity is the number that actually reflects VRAM pressure.
    pub fn gpu_tile_bytes(&self) -> usize {
        self.atlas.capacity_bytes()
    }

    /// Forwards one OS memory-pressure report — the shell's only inbound knob for GPU
    /// residency, mirroring `DISPATCH_SOURCE_TYPE_MEMORYPRESSURE` on macOS (`Normal` / `Warn` /
    /// `Critical`). `PressureState` owns the hysteresis: escalating always applies on the very
    /// next report, while relaxing needs several consecutive reports at the lower level first,
    /// so a signal that oscillates doesn't thrash the retention margin every frame.
    ///
    /// Any effective-level change lowers the atlas's growth ceiling (or raises it back) and
    /// invalidates every cache keyed on the retention margin, which is what turns a narrower
    /// margin into eviction on the very next `sync_tiles` rather than only once the atlas
    /// happens to run out of room. Sustained `Critical` additionally recreates the atlas texture
    /// smaller — the one response expensive enough to reserve for pressure that has actually
    /// persisted rather than spiked once (`PressureState`'s shrink streak).
    pub fn set_memory_pressure(&mut self, level: MemoryPressureLevel) {
        let transition = self.budget.report_pressure(level);
        if !transition.effective_changed && !transition.shrink {
            return;
        }
        // Through the budget, never off the level directly: the device tier sets a floor under
        // the same two numbers, and a pressure report that recovered all the way to `Normal`
        // must not hand a weak GPU back the ceiling it never had.
        let capacity = self.budget.atlas_max_capacity();
        self.atlas.set_max_capacity(capacity);

        if transition.shrink {
            let shared = SharedBindings {
                layout: &self.tile_shared_bgl,
                camera: &self.tile_camera_buf,
                layers: &self.layer_data_buf,
                samplers: &self.samplers,
            };
            let remap = self
                .atlas
                .shrink_to(&self.device, &self.queue, &shared, capacity);
            for tile in self.tiles.values_mut() {
                if let Some(&new_layer) = remap.get(&tile.array_layer) {
                    tile.array_layer = new_layer;
                }
            }
        }

        self.invalidate();
    }

    /// Hand back everything that belonged to the document being closed — the atlas's slots and
    /// the per-layer uniform buffers keyed by its layer ids. Eviction otherwise only happens
    /// inside `sync_tiles`, which needs a document to run, so a closed project's textures
    /// would sit in VRAM until some *other* project was opened and drawn.
    pub fn release_document(&mut self) {
        self.tiles.clear();
        self.base_only_tiles.clear();
        self.atlas.clear();
        self.layer_slots.clear();
        self.next_layer_slot = 0;
        self.clear_layer_cache();
        self.overview.clear();
        self.pan_cache.invalidate();
        self.stroke_coverage.release();
        self.coverage_progress = None;
        self.visible_upload_needed = None;
        self.frame_dirty = FrameDirty::Content;
    }

    pub fn invalidate(&mut self) {
        self.frame_dirty = FrameDirty::Content;
        self.visible_upload_needed = None;
        self.cached_retained_span = None;
        self.cached_visible_span = None;
        self.cached_tile_draw_count = None;
        self.clear_layer_cache();
        self.overview.mark_dirty();
        self.pan_cache.invalidate();
    }

    /// Cheapest invalidation there is: redraw with fresh overlays, keep every cache. Never
    /// downgrades a pending `Camera` or `Content` frame — those already imply an overlay pass.
    pub fn invalidate_overlay(&mut self) {
        if self.frame_dirty == FrameDirty::Clean {
            self.frame_dirty = FrameDirty::Overlay;
        }
    }

    pub fn invalidate_camera(&mut self) {
        self.begin_camera_motion();
        if self.frame_dirty != FrameDirty::Content {
            self.frame_dirty = FrameDirty::Camera;
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if self.config.width != width || self.config.height != height {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            self.invalidate_camera();
        }
    }

    /// Grows the instance buffer if it has to, reporting whether it did. A reallocation
    /// discards the contents, so the caller has to rewrite the vector-path prefix it would
    /// otherwise have left in place.
    fn ensure_stroke_capacity(&mut self, count: usize) -> bool {
        if count <= self.stroke_capacity {
            return false;
        }
        let next = count.next_power_of_two().max(STROKE_INSTANCE_CAPACITY);
        self.stroke_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stroke-instances"),
            size: (next * std::mem::size_of::<StrokeInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.stroke_capacity = next;
        true
    }

    /// Rebuilds and uploads the guide instances, returning how many there are. Guides are
    /// their own tiny buffer rather than a slice of the overlay's on purpose: the overlay is
    /// skipped on a camera-only frame, and a rule that vanished every time the board was
    /// panned would not be a rule.
    fn write_guides(&mut self, doc: &Document) -> u32 {
        let mut guides = std::mem::take(&mut self.guide_scratch);
        guides.clear();
        guides.extend(guide_instances(doc));
        if !guides.is_empty() {
            self.queue
                .write_buffer(&self.guide_buf, 0, bytemuck::cast_slice(&guides));
        }
        let count = guides.len() as u32;
        self.guide_scratch = guides;
        count
    }

    fn ensure_vector_shape_capacity(&mut self, count: usize) {
        if count <= self.vector_shape_capacity {
            return;
        }
        let next = count
            .next_power_of_two()
            .max(VECTOR_SHAPE_INSTANCE_CAPACITY);
        self.vector_shape_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vector-shape-instances"),
            size: (next * std::mem::size_of::<VectorShapeInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.vector_shape_capacity = next;
    }

    fn ensure_tile_instance_capacity(&mut self, count: usize) {
        if count <= self.tile_instance_capacity {
            return;
        }
        let next = count.next_power_of_two().max(TILE_INSTANCE_CAPACITY);
        self.tile_instance_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tile-instances"),
            size: (next * std::mem::size_of::<TileInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.tile_instance_capacity = next;
    }

    /// Whether this tile is sitting in the atlas with only its base level written and the
    /// camera has since settled, so there is now time to finish it.
    fn needs_full_mips(&self, key: &TileKey) -> bool {
        !self.camera_motion && self.base_only_tiles.contains(key)
    }

    fn note_mip_state(&mut self, key: TileKey, skipped_mips: bool) {
        if skipped_mips {
            self.base_only_tiles.insert(key);
        } else {
            self.base_only_tiles.remove(&key);
        }
    }

    /// Motion mode skips the mip chain to keep a gesture cheap, but that is only safe when the
    /// slot already holds a chain to fall back on. A tile reaching the atlas for the first time
    /// mid-gesture has nothing in its upper levels, so it pays for them even during motion —
    /// otherwise zooming out samples levels that were never written.
    fn may_skip_mips(&self, key: &TileKey) -> bool {
        self.camera_motion && self.tiles.contains_key(key) && !self.base_only_tiles.contains(key)
    }

    fn sync_tiles(&mut self, doc: &mut Document) {
        let Some(visible) = doc.visible_rect() else {
            return;
        };
        let retained = visible.expanded_by_tiles(self.budget.retention_margin_tiles());
        let doc_width = doc.width;

        let mut live: FxHashSet<TileKey> = FxHashSet::default();
        let mut visible_keys: FxHashSet<TileKey> = FxHashSet::default();
        let mut uploads: Vec<(usize, TileCoord, TileKey, bool)> = Vec::new();

        for layer_index in 0..doc.layers.len() {
            let layer = &doc.layers[layer_index];
            if !layer.visible {
                // A hidden layer keeps whatever it already has in the atlas. Dropping it would
                // make the eye icon cost a full re-upload — every tile recomposited and
                // re-mipped — on the way back, which on a document with many layers is seconds
                // of stalled main thread per click. Its tiles stay out of `visible_keys`, so
                // they are the first thing `evictable` gives up when the atlas runs short.
                if let Some(&slot) = self.layer_slots.get(&layer.id) {
                    let retain: Vec<TileKey> = self
                        .tiles
                        .keys()
                        .filter(|(s, _, _)| *s == slot)
                        .copied()
                        .collect();
                    live.extend(retain);
                }
                continue;
            }
            let Some(grid) = layer.tiles() else {
                continue;
            };
            let slot = self.layer_slot(&layer.id);
            let dirty = grid.dirty_tiles(DirtyChannel::Render);
            let visible_grid = layer.doc_rect_to_grid(visible);
            let retained_grid = layer.doc_rect_to_grid(retained);

            if layer.is_paper() && grid.whole_tiles_share_one_arc() {
                let coord = TileCoord { x: 0, y: 0 };
                let key: TileKey = (slot, 0, 0);
                live.insert(key);
                visible_keys.insert(key);
                let known = self.tiles.contains_key(&key);
                if !known || dirty.contains(&coord) || self.needs_full_mips(&key) {
                    uploads.push((layer_index, coord, key, self.may_skip_mips(&key)));
                }
                continue;
            }

            for coord in grid.coords_intersecting(retained_grid) {
                let cell = TileGrid::tile_rect(coord);
                let key: TileKey = (slot, coord.x, coord.y);
                live.insert(key);
                if !cell.intersects(visible_grid) {
                    continue;
                }
                visible_keys.insert(key);
                let known = self.tiles.contains_key(&key);
                if known && !dirty.contains(&coord) && !self.needs_full_mips(&key) {
                    continue;
                }
                uploads.push((layer_index, coord, key, self.may_skip_mips(&key)));
            }
        }

        // Bake the mask for every dirty tile up front and in parallel, alongside the mip chain
        // every upload needs regardless — both are pure pixel math that scales with tile count,
        // so both go through rayon rather than running sequentially on the frame thread once the
        // wgpu upload loop below gets to them. Adjustments and opacity no longer bake here at
        // all: `write_layer_data` puts them in the `LayerData` row and `fs_tile` evaluates them
        // per pixel at draw time, so a filter slider drag reaches this loop only if it also
        // painted — the LUT itself never re-walks a tile.
        //
        // Whether the tile had to be baked travels back with its levels. The upload loop needs
        // that answer to decide if the tile may share an atlas slot with its siblings, and
        // re-deriving it there meant compositing every dirty tile a second time, sequentially,
        // on the frame thread — exactly doubling the cost of the one path that already
        // dominates a heavy frame.
        // The baked base level is only carried when there *was* something to bake. Otherwise it
        // stays `None` and the upload reads the tile's own `Arc` where it already lives, which
        // is also what tells the loop below the tile may share an atlas slot with its siblings.
        let payloads: Vec<Option<TilePayload>> = uploads
            .par_iter()
            .map(|(layer_index, coord, _, skip_mips)| {
                let layer = doc.layers.get(*layer_index)?;
                let pixels = layer.tiles()?.get(*coord)?;
                let composited = composited_tile_payload(pixels, *coord, layer, doc_width);
                let base: &[u8] = composited.as_deref().unwrap_or(pixels.as_slice());
                let mips = tile_upload_mips(base, *skip_mips);
                Some((composited, mips))
            })
            .collect();

        // Tiles retained only as prefetch margin (in `live`, but not currently on screen) are
        // the ones sacrificed first when the atlas is full — see the fallback inside the
        // upload loop below.
        let mut evictable: Vec<TileKey> = self
            .tiles
            .keys()
            .filter(|k| live.contains(*k) && !visible_keys.contains(*k))
            .copied()
            .collect();

        let mut shared_gpu: HashMap<(usize, usize), u32> = HashMap::new();
        // Only tiles that actually reached the atlas may be marked clean at the end. An upload
        // the atlas had no room for has to stay dirty, or it is skipped by `build_layer_draws`
        // (no slot) and never retried (not dirty) — a permanent hole in the layer, showing
        // through as bare paper until something happens to dirty that tile again.
        let mut uploaded: Vec<(usize, TileCoord)> = Vec::with_capacity(uploads.len());

        for ((layer_index, coord, key, skip_mips), payload) in uploads.iter().zip(payloads.iter()) {
            let key = *key;
            let skip_mips = *skip_mips;
            let Some((composited, mips)) = payload else {
                continue;
            };
            let baked = composited.is_some();
            let layer = &doc.layers[*layer_index];
            let Some(pixels) = layer.tiles().and_then(|g| g.get(*coord)) else {
                continue;
            };
            let base: &[u8] = composited.as_deref().unwrap_or(pixels.as_slice());
            if !baked {
                let ptr = Arc::as_ptr(pixels) as usize;
                if let Some(&array_layer) = shared_gpu.get(&(*layer_index, ptr)) {
                    self.tiles.insert(key, GpuTile { array_layer });
                    self.note_mip_state(key, skip_mips);
                    uploaded.push((*layer_index, *coord));
                    continue;
                }
            }

            if let Some(existing) = self.tiles.get(&key) {
                let slot = existing.array_layer;
                self.atlas.write(&self.queue, slot, base, mips);
                if !baked {
                    let ptr = Arc::as_ptr(pixels) as usize;
                    shared_gpu.insert((*layer_index, ptr), slot);
                }
                self.note_mip_state(key, skip_mips);
                uploaded.push((*layer_index, *coord));
                continue;
            }

            let shared = SharedBindings {
                layout: &self.tile_shared_bgl,
                camera: &self.tile_camera_buf,
                layers: &self.layer_data_buf,
                samplers: &self.samplers,
            };
            let array_layer = match self.atlas.allocate(&self.device, &self.queue, &shared) {
                Some(slot) => slot,
                None => {
                    let victim = evictable
                        .pop()
                        .or_else(|| live.iter().copied().find(|key| !visible_keys.contains(key)));
                    let Some(victim) = victim else {
                        continue;
                    };
                    if let Some(freed) = self.tiles.remove(&victim) {
                        self.atlas.free(freed.array_layer);
                    }
                    let Some(slot) = self.atlas.allocate(&self.device, &self.queue, &shared) else {
                        continue;
                    };
                    slot
                }
            };
            self.atlas.write(&self.queue, array_layer, base, mips);
            self.tiles.insert(key, GpuTile { array_layer });
            if !baked {
                let ptr = Arc::as_ptr(pixels) as usize;
                shared_gpu.insert((*layer_index, ptr), array_layer);
            }
            self.note_mip_state(key, skip_mips);
            uploaded.push((*layer_index, *coord));
        }

        // Anything no longer live (scrolled entirely out of the retention margin, or its
        // layer was removed) frees its atlas slot for reuse. Tiles evicted above under
        // capacity pressure are already gone from `self.tiles`, so this does not double-free.
        let dropped: Vec<u32> = self
            .tiles
            .iter()
            .filter(|(k, _)| !live.contains(*k))
            .map(|(_, gpu)| gpu.array_layer)
            .collect();
        for slot in dropped {
            self.atlas.free(slot);
        }
        self.tiles.retain(|k, _| live.contains(k));
        self.base_only_tiles.retain(|k| live.contains(k));
        let live_layers: FxHashSet<&str> = doc.layers.iter().map(|l| l.id.as_str()).collect();
        self.layer_slots
            .retain(|id, _| live_layers.contains(id.as_str()));

        for (layer_index, coord) in uploaded {
            if let Some(grid) = doc.layers.get_mut(layer_index).and_then(|l| l.tiles_mut()) {
                grid.clear_dirty_tile(DirtyChannel::Render, coord);
            }
        }
        // Eviction under capacity pressure happens above, so this is the other half of the
        // memo's invalidation: residency changed, ask again next frame.
        self.visible_upload_needed = None;
    }

    /// The whole layer stack as one ordered draw list, filling the instance buffers as it
    /// goes. A document layer's visible tiles become one range in `tiles` — one instanced
    /// draw call regardless of how many tiles that is. A vector layer is one item, so it is
    /// one `LayerDraw::Vector` and nothing in the layer coalesces.
    fn build_layer_draws(
        &mut self,
        doc: &Document,
        tiles: &mut Vec<TileInstance>,
        strokes: &mut Vec<StrokeInstance>,
        shapes: &mut Vec<VectorShapeInstance>,
    ) -> Vec<LayerDraw> {
        let Some(visible) = doc.visible_rect() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (layer_index, layer) in doc.layers.iter().enumerate() {
            let layer_index = layer_index as u32;
            if !layer.visible {
                continue;
            }
            if let Some(item) = layer.content.item() {
                let placement = vector_placement(layer);
                if !item_visible(item, placement, visible) {
                    continue;
                }
                match item {
                    VectorItem::Shape(shape) => {
                        let start = shapes.len() as u32;
                        shapes.push(shape_instance(shape, placement));
                        out.push(LayerDraw::Vector(
                            VectorRun::Shapes,
                            start..shapes.len() as u32,
                        ));
                    }
                    VectorItem::Path(path) => {
                        let start = strokes.len() as u32;
                        push_path_instances(path, placement, strokes);
                        if strokes.len() as u32 > start {
                            out.push(LayerDraw::Vector(
                                VectorRun::Paths,
                                start..strokes.len() as u32,
                            ));
                        }
                    }
                }
                continue;
            }
            let Some(grid) = layer.tiles() else {
                continue;
            };
            let Some(slot) = self.layer_slots.get(&layer.id).copied() else {
                continue;
            };
            if layer.is_paper() && grid.whole_tiles_share_one_arc() {
                // `write_layer_data` already put this layer's atlas slot in its table row; the
                // draw only has to name the row.
                if self.tiles.contains_key(&(slot, 0, 0)) {
                    out.push(LayerDraw::Solid(layer.blend_mode, layer_index));
                }
                continue;
            }
            let visible_grid = layer.doc_rect_to_grid(visible);
            let start = tiles.len() as u32;
            for coord in grid.coords_intersecting(visible_grid) {
                let key: TileKey = (slot, coord.x, coord.y);
                let Some(gpu) = self.tiles.get(&key) else {
                    continue;
                };
                let (ox, oy) = coord.origin();
                tiles.push(TileInstance {
                    origin: [ox as f32, oy as f32],
                    slot: gpu.array_layer,
                    layer_index,
                });
            }
            if tiles.len() as u32 > start {
                out.push(LayerDraw::Tiles(
                    layer.blend_mode,
                    start..tiles.len() as u32,
                ));
            }
        }
        out
    }

    /// Replays `cached_draws` into whatever color attachment `pass` targets — the shared body
    /// behind both a full content redraw (the whole visible tile/vector set, into a fresh
    /// `PanCache` reference) and a blit-frame's exposed-strip repair (the same draws, scissored
    /// down to just the strip). Positions are document-space in the instance buffer, so the
    /// same buffers and draw calls reproduce correctly at any camera state — nothing here reads
    /// `doc` directly.
    fn draw_cached_content<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        for draw in &self.cached_draws {
            match draw {
                LayerDraw::Tiles(mode, range) => {
                    pass.set_pipeline(self.tile_pipeline(*mode));
                    pass.set_bind_group(0, self.atlas.bind_group(), &[]);
                    pass.set_vertex_buffer(0, self.tile_instance_buf.slice(..));
                    pass.draw(0..6, range.clone());
                }
                LayerDraw::Solid(mode, layer_index) => {
                    pass.set_pipeline(self.solid_pipeline(*mode));
                    pass.set_bind_group(0, self.atlas.bind_group(), &[]);
                    // The instance range *is* the argument: `vs_doc_quad` reads its layer row
                    // from `instance_index`, so a one-instance draw at `layer_index` says which.
                    pass.draw(0..6, *layer_index..*layer_index + 1);
                }
                LayerDraw::Vector(kind, range) => {
                    let (pipeline, buf) = match kind {
                        VectorRun::Shapes => (&self.vector_shape_pipeline, &self.vector_shape_buf),
                        VectorRun::Paths => (&self.stroke_pipeline, &self.stroke_buf),
                    };
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(0, &self.preview_bg, &[]);
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw(0..6, range.clone());
                }
            }
        }
    }

    /// Draws the whole visible stack fresh into the `PanCache` reference texture, scissored to
    /// the current paper rect, and commits it as the new blit baseline. This is the "content
    /// pass" side of `ChunkDraw` in the plan's terms — a full redraw, just retargeted from the
    /// swapchain to an offscreen texture so a later camera-only frame has something to shift.
    fn redraw_pan_cache_reference(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        pan: (f32, f32),
        zoom: f32,
        dpr: f32,
        scissor: PxRect,
    ) {
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pan-cache-full"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: self.pan_cache.reference_view(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                ..Default::default()
            });
            pass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
            self.draw_cached_content(&mut pass);
        }
        self.pan_cache.commit_reference(pan, zoom, dpr, scissor);
    }

    /// Copies the previous frame's content shifted by this frame's pan delta into the
    /// `PanCache` working texture, patches the strips the copy could not have populated
    /// (`framebuffer::exposed_rects`) by replaying `cached_draws` scissored to just those
    /// rects, then promotes the result to be the next frame's reference. Each strip is cleared
    /// to transparent first — `LoadOp::Load` preserves the freshly copied region, so without an
    /// explicit clear a semi-transparent stroke in the strip would blend against whatever this
    /// texture held two frames ago instead of nothing.
    ///
    /// The promotion at the end is what keeps the strips thin: measured against the previous
    /// frame the exposed band is one frame's worth of travel, a few pixels on a normal drag.
    /// Measured against a reference frozen at the last full redraw — the way this used to work
    /// — it grew with the whole gesture. See `PanCache`'s own note.
    fn patch_pan_cache_working(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        plan: framebuffer::BlitPlan,
        dpr: f32,
        scissor: PxRect,
    ) {
        let framebuffer::BlitPlan { src, dst, shift } = plan;
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: self.pan_cache.reference_texture(),
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: src.0,
                    y: src.1,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: self.pan_cache.working_texture(),
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: dst.0,
                    y: dst.1,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: src.2,
                height: src.3,
                depth_or_array_layers: 1,
            },
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("pan-cache-patch"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: self.pan_cache.working_view(),
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            ..Default::default()
        });
        {
            for strip in framebuffer::exposed_rects(scissor, dst)
                .into_iter()
                .flatten()
            {
                pass.set_scissor_rect(strip.0, strip.1, strip.2, strip.3);
                pass.set_pipeline(self.pan_cache.clear_pipeline());
                pass.draw(0..3, 0..1);
                self.draw_cached_content(&mut pass);
            }
        }
        drop(pass);
        self.pan_cache.commit_shift(shift, dpr, scissor);
    }

    pub fn render(&mut self, doc: &mut Document) {
        // The caret is the whole of `has_animated_overlay`, and it is a square wave: parking the
        // cursor in a text layer used to run the full board pass at display rate to service a
        // signal that changes state twice a second. Comparing the phase against the frame that
        // was actually drawn turns that into two frames a second — and the same comparison
        // catches the caret going away, which needs one frame to erase it.
        let caret_phase = doc
            .has_animated_overlay()
            .then(|| text_caret_visible(self.started.elapsed().as_secs_f32()));
        if self.frame_dirty == FrameDirty::Clean
            && !doc.has_live_preview()
            && caret_phase == self.drawn_caret_phase
        {
            return;
        }

        let (dw, dh) = doc.camera.device_size();
        self.resize(dw, dh);
        self.pan_cache
            .resize(&self.device, self.config.width, self.config.height);

        let viewport = [
            (self.config.width as f32).max(1.0),
            (self.config.height as f32).max(1.0),
        ];

        let tile_draw_count =
            if matches!(self.frame_dirty, FrameDirty::Camera | FrameDirty::Overlay) {
                self.cached_tile_draw_count
                    .unwrap_or_else(|| self.visible_tile_draw_count(doc))
            } else {
                let count = self.visible_tile_draw_count(doc);
                self.cached_tile_draw_count = Some(count);
                count
            };
        let use_overview = self
            .overview
            .should_use(tile_draw_count, doc.has_live_preview());

        let need_tile_sync = !use_overview
            && (self.frame_dirty == FrameDirty::Content
                || Self::retained_span(doc, self.budget.retention_margin_tiles())
                    != self.cached_retained_span
                || self.visible_upload_needed(doc));
        let need_draw_rebuild = !use_overview
            && (need_tile_sync || Self::visible_span(doc) != self.cached_visible_span);
        let camera_only =
            self.frame_dirty == FrameDirty::Camera && !doc.has_live_preview() && !use_overview;

        if use_overview {
            if self.frame_dirty == FrameDirty::Content {
                self.overview.mark_dirty();
            }
            self.overview.sync(doc, &self.device, &self.queue);
            self.overview.write_camera(&self.queue, doc, viewport);
        } else {
            self.overview.prewarm(doc, &self.device, &self.queue);
            if need_tile_sync {
                self.sync_tiles(doc);
            }
            if need_draw_rebuild {
                // After `sync_tiles`, because solid Paper's row carries an atlas slot that only
                // exists once its tile is resident, and before the draw list, which indexes
                // these rows.
                self.write_layer_data(doc);
                self.rebuild_layer_cache(doc);
            }
        }

        let desk = calumma_core::DeskMetrics::DEFAULT;
        if self
            .desk_lattice
            .ensure(&self.device, &self.queue, doc.camera.dpr)
        {
            self.paper_bg = paper_bind_group(
                &self.device,
                &self.paper_bgl,
                &self.paper_buf,
                &self.desk_lattice,
            );
        }
        let paper = PaperUniforms {
            pan: [doc.camera.pan_x, doc.camera.pan_y],
            zoom: doc.camera.zoom,
            dpr: doc.camera.dpr,
            doc_size: [doc.width as f32, doc.height as f32],
            viewport,
            dark: if doc.dark_theme { 1.0 } else { 0.0 },
            lattice_side: self.desk_lattice.shader_side(),
            _pad1: 0.0,
            _pad2: 0.0,
            desk_metrics: [
                desk.cell,
                desk.line_width,
                desk.cross_arm,
                desk.cross_line_width,
            ],
            desk: rgba_unit(doc.board_colors.desk),
            grid: rgba_unit(doc.board_colors.grid),
            paper_border: rgba_unit(doc.board_colors.paper_border),
        };
        self.queue
            .write_buffer(&self.paper_buf, 0, bytemuck::bytes_of(&paper));

        let tile_camera = TileCamera {
            pan: [doc.camera.pan_x, doc.camera.pan_y],
            zoom: doc.camera.zoom,
            dpr: doc.camera.dpr,
            viewport,
            doc_size: [doc.width as f32, doc.height as f32],
            crisp: f32::from(u8::from(doc.camera.zoom >= CRISP_PIXEL_ZOOM)),
            _pad: [0.0; 3],
        };
        self.queue
            .write_buffer(&self.tile_camera_buf, 0, bytemuck::bytes_of(&tile_camera));

        let scissor: Option<PxRect> = doc.camera.paper_scissor(
            doc.width as f32,
            doc.height as f32,
            self.config.width,
            self.config.height,
        );
        let pan = (doc.camera.pan_x, doc.camera.pan_y);
        // The pan cache holds this frame's content already when nothing it depends on has
        // moved: no tile resync, no draw-list rebuild, and the same camera it was captured at.
        // That is every overlay-only frame — a pen stroke between pointer-down and pointer-up,
        // a shape being dragged out, a blinking caret — and it means the content pass is
        // skipped entirely rather than recompositing the visible stack behind an overlay that
        // is the only thing that changed.
        let reuse_reference = !use_overview
            && !need_tile_sync
            && !need_draw_rebuild
            && scissor.is_some_and(|s| {
                self.pan_cache
                    .reference_matches(pan, doc.camera.zoom, doc.camera.dpr, s)
            });
        let blit_plan = if !use_overview && camera_only && !need_draw_rebuild && !reuse_reference {
            scissor.and_then(|s| self.pan_cache.plan(pan, doc.camera.zoom, doc.camera.dpr, s))
        } else {
            None
        };

        let preview_shape = doc.preview_shape();
        let ink = doc.ink_rgba();
        let color = [
            ink[0] as f32 / 255.0,
            ink[1] as f32 / 255.0,
            ink[2] as f32 / 255.0,
            ink[3] as f32 / 255.0,
        ];
        let (p0, p1, tool, half_width, fill, shape_stroke, shape_color, shape_stroke_color) =
            match preview_shape {
                Some(s) => {
                    let (fill_ink, stroke_ink) = doc.shape_paint(s.tool);
                    (
                        [s.start.0, s.start.1],
                        [s.end.0, s.end.1],
                        s.tool as u32 as f32,
                        s.half_width,
                        f32::from(u8::from(s.fill)),
                        f32::from(u8::from(s.stroke)),
                        rgba_unit(fill_ink),
                        rgba_unit(stroke_ink),
                    )
                }
                None => match selection_rect_or_ellipse(doc) {
                    // The marquee is an outline and nothing else, so it rides in on the stroke
                    // half of the same uniform the shape preview uses.
                    Some((p0, p1, sel_tool)) => (
                        p0,
                        p1,
                        sel_tool as u32 as f32,
                        SELECTION_OUTLINE_WIDTH,
                        0.0,
                        1.0,
                        SELECTION_OUTLINE_COLOR,
                        SELECTION_OUTLINE_COLOR,
                    ),
                    None => ([0.0, 0.0], [0.0, 0.0], 0.0, 0.0, 0.0, 0.0, color, color),
                },
            };
        // Written every frame, not only on the ones that build an overlay: the guide pass reads
        // the camera out of this buffer, and guides are board furniture that has to keep up with
        // a pan the overlay sits out.
        let preview = PreviewUniforms {
            pan: [doc.camera.pan_x, doc.camera.pan_y],
            zoom: doc.camera.zoom,
            dpr: doc.camera.dpr,
            viewport,
            _align_color: [0.0, 0.0],
            color: shape_color,
            p0,
            p1,
            half_width,
            tool,
            fill,
            shape_stroke,
            stroke_ink: rgba_unit(doc.stroke_ink()),
            shape_stroke_color,
        };
        self.queue
            .write_buffer(&self.preview_buf, 0, bytemuck::bytes_of(&preview));

        let guide_count = self.write_guides(doc);
        let mut overlay_range = 0u32..0u32;
        let mut screen_overlay_range = 0u32..0u32;
        // `brush_range` is the segments to *union into* the coverage target this frame, which is
        // empty on any frame the pointer did not move; `brush_active` is whether there is a live
        // brush stroke to composite onto the board at all. They used to be the same question,
        // because the target was rebuilt from the first point every frame.
        let mut brush_range = 0u32..0u32;
        let mut brush_active = false;
        let mut brush_restart = false;
        if !camera_only {
            let radius = doc.effective_brush_size() * 0.5;
            let stroke_color = if doc.tool == Tool::Eraser {
                ERASER_PREVIEW_COLOR
            } else {
                color
            };
            let mut brush_instances: Vec<StrokeInstance> = Vec::new();
            // The stroke buffer is a vector-path prefix (owned by `cached_draws`' ranges)
            // followed by this frame's overlay. Only the overlay changes on an overlay frame,
            // so it is built into a reused scratch buffer and written at the prefix's offset —
            // cloning `cached_strokes` every frame just to rewrite a suffix put a full copy of
            // every vector path in the document on the hot path.
            //
            // The overlay itself splits in two, by which pass measures it: ink-shaped previews
            // stay in document units on `stroke_pipeline`, while chrome — the transform and
            // item frames, the text session's box and caret, the hover outline — is measured in
            // screen pixels on `overlay_pipeline`. Both are contiguous ranges of the same
            // buffer, so the split costs a second `draw`, not a second upload.
            let prefix_len = if self.camera_motion {
                0
            } else {
                self.cached_strokes.len()
            };
            let mut instances = std::mem::take(&mut self.overlay_scratch);
            instances.clear();
            let mut screen_instances = std::mem::take(&mut self.screen_overlay_scratch);
            screen_instances.clear();
            let overlay_start = prefix_len as u32;
            if doc.text_editing() {
                screen_instances.extend(text_overlay_instances(
                    doc,
                    self.started.elapsed().as_secs_f32(),
                ));
            } else if doc.previews_brush_stroke() {
                brush_active = true;
                let profile = doc.active_brush_profile();
                // Ahead of the append decision rather than after the ranges are built: a target
                // the surface resize just recreated is empty, and only `ensure` knows that.
                let recreated = self.stroke_coverage.ensure(
                    &self.device,
                    self.config.width,
                    self.config.height,
                );
                let progress = CoverageProgress {
                    generation: doc.stroke_generation(),
                    points: doc.stroke_points.len(),
                    pan,
                    zoom: doc.camera.zoom,
                    dpr: doc.camera.dpr,
                    brush: brush_params(radius, &profile),
                    color: stroke_color,
                };
                let first_segment = match self.coverage_progress {
                    Some(prev) if !recreated && prev.appendable(&progress) => {
                        stroke_segment_count(prev.points)
                    }
                    _ => 0,
                };
                brush_restart = first_segment == 0;
                brush_instances = stroke_instances_from(
                    &doc.stroke_points,
                    first_segment,
                    radius,
                    stroke_color,
                    &profile,
                );
                self.coverage_progress = Some(progress);
            } else if !doc.stroke_points.is_empty() && doc.tool.previews_stroke() {
                instances.extend(stroke_instances(
                    &doc.stroke_points,
                    radius,
                    stroke_color,
                    &BrushProfile::HARD,
                ));
            } else if let Some(handles) = doc.transform_handles() {
                screen_instances.extend(transform_overlay_instances(handles));
            } else if let Some(points) = selection_lasso_points(doc) {
                instances.extend(stroke_instances(
                    &points,
                    SELECTION_OUTLINE_WIDTH,
                    SELECTION_OUTLINE_COLOR,
                    &BrushProfile::HARD,
                ));
            } else if let Some(edges) =
                selection_mask_edges(doc, SELECTION_OUTLINE_WIDTH, SELECTION_OUTLINE_COLOR)
            {
                instances.extend(edges);
            }
            // Not part of the chain above: a selected item's frame is drawn under the Move
            // tool too, where none of those branches is the one that ran. It costs nothing
            // when nothing is selected, and `transform_handles` stands the layer frame down
            // while it is on screen, so the two can never both draw.
            screen_instances.extend(vector_selection_instances(doc));
            // Unconditional for the same reason: the engine decides whether there is a brush
            // cursor to draw, and answers with nothing when there is not.
            screen_instances.extend(brush_ring_instances(doc));
            if let Some((index, corners)) = doc.layer_highlight() {
                let covered = doc
                    .transform_handles()
                    .is_some_and(|(handle_index, _, _)| handle_index == index);
                if !covered {
                    screen_instances.extend(layer_highlight_instances(
                        corners,
                        self.started.elapsed().as_secs_f32(),
                        doc.camera.zoom,
                    ));
                }
            }
            overlay_range = overlay_start..overlay_start + instances.len() as u32;
            let screen_start = overlay_range.end;
            instances.append(&mut screen_instances);
            screen_overlay_range = screen_start..prefix_len as u32 + instances.len() as u32;
            let brush_start = screen_overlay_range.end;
            instances.append(&mut brush_instances);
            brush_range = brush_start..prefix_len as u32 + instances.len() as u32;
            let total = prefix_len + instances.len();
            let grew = self.ensure_stroke_capacity(total);
            let stride = std::mem::size_of::<StrokeInstance>() as u64;
            if (grew || need_draw_rebuild) && prefix_len > 0 {
                self.queue.write_buffer(
                    &self.stroke_buf,
                    0,
                    bytemuck::cast_slice(&self.cached_strokes),
                );
            }
            if !instances.is_empty() {
                self.queue.write_buffer(
                    &self.stroke_buf,
                    prefix_len as u64 * stride,
                    bytemuck::cast_slice(&instances),
                );
            }
            self.overlay_scratch = instances;
            self.screen_overlay_scratch = screen_instances;
            if need_draw_rebuild && !self.cached_shapes.is_empty() {
                self.ensure_vector_shape_capacity(self.cached_shapes.len());
                self.queue.write_buffer(
                    &self.vector_shape_buf,
                    0,
                    bytemuck::cast_slice(&self.cached_shapes),
                );
            }
        }

        if need_draw_rebuild && !self.cached_tile_instances.is_empty() {
            self.ensure_tile_instance_capacity(self.cached_tile_instances.len());
            self.queue.write_buffer(
                &self.tile_instance_buf,
                0,
                bytemuck::cast_slice(&self.cached_tile_instances),
            );
        }

        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            _ => return,
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });

        // The content pass, in one of three modes: reuse what the `PanCache` reference already
        // holds, shift it by this frame's pan and patch the strips that exposes, or redraw the
        // visible stack into it from scratch. All three leave the frame's content in the
        // reference texture, so the board pass below only ever draws a single textured quad for
        // the content, not the tile/vector instance list.
        let has_content = !use_overview
            && scissor
                .map(|s| {
                    if reuse_reference {
                        return;
                    }
                    if let Some(plan) = blit_plan {
                        self.patch_pan_cache_working(&mut encoder, plan, doc.camera.dpr, s);
                    } else {
                        self.redraw_pan_cache_reference(
                            &mut encoder,
                            pan,
                            doc.camera.zoom,
                            doc.camera.dpr,
                            s,
                        );
                    }
                })
                .is_some();

        if brush_active {
            // `accumulate` no-ops on an empty range that is not a restart, which is the frame
            // where the pointer has not moved far enough to add a segment — the target already
            // holds the whole stroke and the board pass below still composites it.
            self.stroke_coverage.accumulate(
                &mut encoder,
                &self.preview_bg,
                &self.stroke_buf,
                brush_range.clone(),
                scissor,
                brush_restart,
            );
        }

        // One render pass for the whole frame. These four stages used to be four separate
        // passes chained with LoadOp::Load — correct, but every begin/end pair is a real
        // boundary on tile-based GPUs (Apple Silicon among them), forcing a tile-memory
        // flush each time. They draw into the same attachment in the same order regardless,
        // so a single pass produces an identical image for a fraction of the pass overhead.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("board"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                ..Default::default()
            });

            pass.set_pipeline(&self.paper_pipeline);
            pass.set_bind_group(0, &self.paper_bg, &[]);
            pass.draw(0..3, 0..1);

            if let Some((x, y, w, h)) = scissor {
                pass.set_scissor_rect(x, y, w, h);

                if use_overview {
                    self.overview.draw(&mut pass);
                } else if has_content {
                    pass.set_pipeline(self.pan_cache.blit_pipeline());
                    pass.set_bind_group(0, self.pan_cache.bind_group(), &[]);
                    pass.draw(0..3, 0..1);
                }
            }

            // Over the artwork, under the transform box and the marching ants: a guide is
            // something the picture is aligned against, not something drawn on it. It is the one
            // pass *outside* the paper scissor, because a guide is measured against the view —
            // it runs edge to edge, meeting the ruler it was pulled from, rather than stopping
            // where the paper does (`guide_instances`). Drawn even with the paper fully off
            // screen, which is why it does not sit inside the `if let` either.
            if guide_count > 0 {
                pass.set_scissor_rect(0, 0, self.config.width, self.config.height);
                pass.set_pipeline(&self.guide_pipeline);
                pass.set_bind_group(0, &self.preview_bg, &[]);
                pass.set_vertex_buffer(0, self.guide_buf.slice(..));
                pass.draw(0..6, 0..guide_count);
            }

            if let Some((x, y, w, h)) = scissor {
                pass.set_scissor_rect(x, y, w, h);

                if !overlay_range.is_empty() {
                    pass.set_pipeline(&self.stroke_pipeline);
                    pass.set_bind_group(0, &self.preview_bg, &[]);
                    pass.set_vertex_buffer(0, self.stroke_buf.slice(..));
                    pass.draw(0..6, overlay_range.clone());
                }

                if !screen_overlay_range.is_empty() {
                    pass.set_pipeline(&self.overlay_pipeline);
                    pass.set_bind_group(0, &self.preview_bg, &[]);
                    pass.set_vertex_buffer(0, self.stroke_buf.slice(..));
                    pass.draw(0..6, screen_overlay_range.clone());
                }

                if preview_shape.is_some() {
                    pass.set_pipeline(&self.shape_pipeline);
                    pass.set_bind_group(0, &self.preview_bg, &[]);
                    pass.draw(0..3, 0..1);
                }
            }

            // The brush ring is a cursor, so it goes on top of everything and — like the guides —
            // outside the paper scissor. `Document::brush_ring` has already decided there is a
            // stamp to promise, and on a pasted layer that overflows the paper that stamp can
            // land out over the desk; clipping the ring to the paper drew nothing there while
            // the shell had already hidden its own cursor, so the pointer disappeared.
            if brush_active {
                pass.set_scissor_rect(0, 0, self.config.width, self.config.height);
                self.stroke_coverage.composite(&mut pass, &self.preview_bg);
            }
        }

        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
        self.tick_camera_motion();
        // A gesture in flight asks for another frame, but only an *overlay* one: the pointer
        // events that move it already invalidate at the right level — `Content` for anything
        // that touched tiles, vectors or a transform, `Overlay` for a preview that is drawn on
        // top of content nobody changed. Pinning `Content` here instead re-synced every tile
        // and recomposited the whole stack on every frame of every stroke, for a stroke that
        // lays no pixels down until pointer-up.
        // A caret no longer pins `Overlay` — the phase comparison at the top of the frame is what
        // asks for its next one, and pinning `Overlay` here would defeat that by making the
        // early-out unreachable for as long as a text session was open.
        self.frame_dirty = if doc.has_live_preview() {
            FrameDirty::Overlay
        } else {
            FrameDirty::Clean
        };
        // Recorded here rather than at the top, so a frame abandoned on a lost surface leaves the
        // caret asking to be drawn instead of counting as drawn.
        self.drawn_caret_phase = caret_phase;
    }
}

const ERASER_PREVIEW_COLOR: [f32; 4] = [0.5, 0.5, 0.5, 0.5];
const SELECTION_OUTLINE_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 0.9];
const SELECTION_OUTLINE_WIDTH: f32 = 1.5;
/// Group 0 for every tile and solid draw: the per-frame camera, the shared atlas with its two
/// samplers, and the layer table. Bound once for the whole board — there is no per-layer group,
/// which is the point of the table.
///
/// A function rather than an inline descriptor so the GPU tests below build their pipelines
/// against the *same* layout the app does; a shader/layout disagreement then fails a test
/// instead of only the running app.
fn tile_shared_bgl(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("tile-shared-bgl"),
        entries: &[
            uniform_entry(0, std::mem::size_of::<TileCamera>()),
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            // The layer table. `VERTEX_FRAGMENT` since plan 23: `fs_tile`/`fs_solid_tile` now
            // read `opacity`/`lut_mode`/`tone`/`saturation`/`vibrance` off the same row
            // `vs_tile`/`vs_doc_quad` already read for the transform, evaluating the adjustment
            // LUT per pixel instead of the CPU baking it into tile bytes before upload.
            //
            // `Limits::default()` guarantees 8 storage buffers per stage, and Metal offers
            // far more; the binding is not near any adapter limit. What is finite is the
            // *row count*, and that is a plain buffer size the renderer grows.
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    })
}

/// Group 0 for `fs_paper`: the desk uniforms and the baked lattice period it reads its grid
/// coverage out of. Rebuilt whenever the lattice is rebaked, which is a backing-scale change and
/// nothing else — a bind group holds the view it was built from, so without this the shader
/// would keep reading the texture for the old `dpr`.
pub(crate) fn paper_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniforms: &wgpu::Buffer,
    lattice: &DeskLattice,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("paper-bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(lattice.view()),
            },
        ],
    })
}

/// The adapter, reduced to the one thing [`DeviceTier::classify`] reads. `core` owns the tier
/// table and is kept free of platform dependencies, so the mapping from wgpu's own enum lives
/// here — the render crate is the only place that has an adapter to ask.
fn gpu_kind(device_type: wgpu::DeviceType) -> GpuKind {
    match device_type {
        wgpu::DeviceType::DiscreteGpu => GpuKind::Discrete,
        wgpu::DeviceType::IntegratedGpu | wgpu::DeviceType::VirtualGpu => GpuKind::Integrated,
        wgpu::DeviceType::Cpu => GpuKind::Software,
        wgpu::DeviceType::Other => GpuKind::Other,
    }
}

fn uniform_entry(binding: u32, size: usize) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: NonZeroU64::new(size as u64),
        },
        count: None,
    }
}

fn replace_target(format: wgpu::TextureFormat) -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format,
        blend: Some(wgpu::BlendState::REPLACE),
        write_mask: wgpu::ColorWrites::ALL,
    }
}

pub(crate) const PREMULTIPLIED_ALPHA_COMPONENT: wgpu::BlendComponent = wgpu::BlendComponent {
    src_factor: wgpu::BlendFactor::One,
    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
    operation: wgpu::BlendOperation::Add,
};

fn premultiplied_target(format: wgpu::TextureFormat) -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format,
        blend: Some(wgpu::BlendState {
            color: PREMULTIPLIED_ALPHA_COMPONENT,
            alpha: PREMULTIPLIED_ALPHA_COMPONENT,
        }),
        write_mask: wgpu::ColorWrites::ALL,
    }
}

fn multiply_target(format: wgpu::TextureFormat) -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format,
        blend: Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Dst,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: PREMULTIPLIED_ALPHA_COMPONENT,
        }),
        write_mask: wgpu::ColorWrites::ALL,
    }
}

fn screen_target(format: wgpu::TextureFormat) -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format,
        blend: Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrc,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: PREMULTIPLIED_ALPHA_COMPONENT,
        }),
        write_mask: wgpu::ColorWrites::ALL,
    }
}

fn alpha_target(format: wgpu::TextureFormat) -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format,
        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
        write_mask: wgpu::ColorWrites::ALL,
    }
}

const TILE_INSTANCE_ATTRS: &[wgpu::VertexAttribute] = &[
    wgpu::VertexAttribute {
        offset: 0,
        shader_location: 0,
        format: wgpu::VertexFormat::Float32x2,
    },
    wgpu::VertexAttribute {
        offset: 8,
        shader_location: 1,
        format: wgpu::VertexFormat::Uint32,
    },
    wgpu::VertexAttribute {
        offset: 12,
        shader_location: 2,
        format: wgpu::VertexFormat::Uint32,
    },
];

pub(crate) const STROKE_ATTRS: &[wgpu::VertexAttribute] = &[
    wgpu::VertexAttribute {
        offset: 0,
        shader_location: 0,
        format: wgpu::VertexFormat::Float32x4,
    },
    wgpu::VertexAttribute {
        offset: 16,
        shader_location: 1,
        format: wgpu::VertexFormat::Float32x4,
    },
    wgpu::VertexAttribute {
        offset: 32,
        shader_location: 2,
        format: wgpu::VertexFormat::Float32x4,
    },
];

const GUIDE_ATTRS: &[wgpu::VertexAttribute] = &[
    wgpu::VertexAttribute {
        offset: 0,
        shader_location: 0,
        format: wgpu::VertexFormat::Float32x4,
    },
    wgpu::VertexAttribute {
        offset: 16,
        shader_location: 1,
        format: wgpu::VertexFormat::Float32x4,
    },
];

const VECTOR_SHAPE_ATTRS: &[wgpu::VertexAttribute] = &[
    wgpu::VertexAttribute {
        offset: 0,
        shader_location: 0,
        format: wgpu::VertexFormat::Float32x2,
    },
    wgpu::VertexAttribute {
        offset: 8,
        shader_location: 1,
        format: wgpu::VertexFormat::Float32x2,
    },
    wgpu::VertexAttribute {
        offset: 16,
        shader_location: 2,
        format: wgpu::VertexFormat::Float32x4,
    },
    wgpu::VertexAttribute {
        offset: 32,
        shader_location: 3,
        format: wgpu::VertexFormat::Float32x4,
    },
    wgpu::VertexAttribute {
        offset: 48,
        shader_location: 4,
        format: wgpu::VertexFormat::Float32,
    },
    wgpu::VertexAttribute {
        offset: 52,
        shader_location: 5,
        format: wgpu::VertexFormat::Float32,
    },
    wgpu::VertexAttribute {
        offset: 56,
        shader_location: 6,
        format: wgpu::VertexFormat::Float32,
    },
    wgpu::VertexAttribute {
        offset: 60,
        shader_location: 7,
        format: wgpu::VertexFormat::Float32,
    },
];

/// Everything a `Renderer` does needs a `wgpu::Surface`, which needs a window, so what can be
/// tested here without one is what the renderer *decides* rather than what it draws: the blend
/// state each layer blend mode compiles to, the bind-group and vertex layouts the pipelines are
/// declared with, and the tile spans that drive upload and eviction. The drawing itself is
/// covered where it can be — `stroke_coverage`, `tile_atlas`, `framebuffer` and `overview` all
/// run against a headless device.
#[cfg(test)]
mod tests {
    use super::*;
    use calumma_core::tile::TILE_SIZE;

    fn blend(target: wgpu::ColorTargetState) -> wgpu::BlendState {
        target.blend.expect("every board target blends")
    }

    fn doc_with_viewport() -> Document {
        let mut doc = Document::new("p".into(), "t", 2048, 2048);
        doc.resize_viewport(800.0, 600.0, 1.0);
        doc.fit_to_view();
        doc
    }

    #[test]
    fn every_target_keeps_its_format_and_writes_every_channel() {
        let format = wgpu::TextureFormat::Bgra8UnormSrgb;
        for target in [
            replace_target(format),
            premultiplied_target(format),
            multiply_target(format),
            screen_target(format),
            alpha_target(format),
        ] {
            assert_eq!(target.format, format);
            assert_eq!(target.write_mask, wgpu::ColorWrites::ALL);
        }
    }

    /// Normal is a premultiplied source-over: the tile arrives with its color already scaled by
    /// its alpha, so the source factor is `One` and only the destination is attenuated. `Src`
    /// there instead would double-apply alpha and darken every edge.
    #[test]
    fn normal_blends_as_premultiplied_source_over() {
        let state = blend(premultiplied_target(wgpu::TextureFormat::Bgra8UnormSrgb));

        assert_eq!(state.color.src_factor, wgpu::BlendFactor::One);
        assert_eq!(state.color.dst_factor, wgpu::BlendFactor::OneMinusSrcAlpha);
        assert_eq!(state.color.operation, wgpu::BlendOperation::Add);
        assert_eq!(state.alpha, state.color, "alpha rides the same component");
    }

    /// Multiply is `src * dst` expressed as a fixed-function factor: the source is scaled *by
    /// the destination* on the way in. Alpha must not be multiplied too — coverage still
    /// composites normally, or a multiplied layer would eat the alpha underneath it.
    #[test]
    fn multiply_scales_the_source_by_the_destination_but_leaves_alpha_alone() {
        let state = blend(multiply_target(wgpu::TextureFormat::Bgra8UnormSrgb));

        assert_eq!(state.color.src_factor, wgpu::BlendFactor::Dst);
        assert_eq!(state.color.dst_factor, wgpu::BlendFactor::OneMinusSrcAlpha);
        assert_eq!(state.alpha, PREMULTIPLIED_ALPHA_COMPONENT);
    }

    /// Screen is `1 - (1 - src)(1 - dst)`, which factors to `src + dst * (1 - src)` — hence a
    /// destination factor keyed on the source *color*, not its alpha. This is the one place the
    /// two differ, and swapping them silently turns Screen back into Normal.
    #[test]
    fn screen_attenuates_the_destination_by_the_source_color() {
        let state = blend(screen_target(wgpu::TextureFormat::Bgra8UnormSrgb));

        assert_eq!(state.color.src_factor, wgpu::BlendFactor::One);
        assert_eq!(state.color.dst_factor, wgpu::BlendFactor::OneMinusSrc);
        assert_ne!(state.color.dst_factor, state.alpha.dst_factor);
        assert_eq!(state.alpha, PREMULTIPLIED_ALPHA_COMPONENT);
    }

    #[test]
    fn the_three_blend_modes_compile_to_three_different_states() {
        let format = wgpu::TextureFormat::Bgra8UnormSrgb;
        let normal = blend(premultiplied_target(format));
        let multiply = blend(multiply_target(format));
        let screen = blend(screen_target(format));

        assert_ne!(normal.color, multiply.color);
        assert_ne!(normal.color, screen.color);
        assert_ne!(multiply.color, screen.color);
        assert_eq!(blend(replace_target(format)), wgpu::BlendState::REPLACE);
        assert_eq!(
            blend(alpha_target(format)),
            wgpu::BlendState::ALPHA_BLENDING
        );
    }

    /// `min_binding_size` is how a uniform that has grown in Rust but not in WGSL (or the other
    /// way round) is caught at pipeline creation instead of read as garbage at 120 Hz.
    #[test]
    fn a_uniform_entry_declares_the_size_of_the_struct_it_carries() {
        let entry = uniform_entry(2, std::mem::size_of::<TileCamera>());

        assert_eq!(entry.binding, 2);
        assert_eq!(entry.count, None);
        let wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset,
            min_binding_size,
        } = entry.ty
        else {
            panic!("a uniform entry has to be a buffer");
        };
        assert_eq!(ty, wgpu::BufferBindingType::Uniform);
        assert!(!has_dynamic_offset);
        assert_eq!(
            min_binding_size.map(|n| n.get()),
            Some(std::mem::size_of::<TileCamera>() as u64)
        );
    }

    /// Instance attributes are hand-written offsets into a `#[repr(C)]` struct. Nothing in the
    /// type system ties the two together, so a field inserted into the struct without the table
    /// being updated makes every instance read the wrong bytes — with no error anywhere.
    #[test]
    fn every_instance_attribute_table_matches_the_struct_it_reads() {
        let cases: [(&[wgpu::VertexAttribute], usize, &str); 4] = [
            (
                TILE_INSTANCE_ATTRS,
                std::mem::size_of::<TileInstance>(),
                "tile",
            ),
            (
                STROKE_ATTRS,
                std::mem::size_of::<StrokeInstance>(),
                "stroke",
            ),
            (GUIDE_ATTRS, std::mem::size_of::<GuideInstance>(), "guide"),
            (
                VECTOR_SHAPE_ATTRS,
                std::mem::size_of::<VectorShapeInstance>(),
                "vector shape",
            ),
        ];

        for (attrs, stride, name) in cases {
            let mut cursor = 0u64;
            for (i, attr) in attrs.iter().enumerate() {
                assert_eq!(
                    attr.offset, cursor,
                    "{name} attribute {i} is not packed against the one before it"
                );
                assert_eq!(attr.shader_location, i as u32, "{name} location {i}");
                cursor += attr.format.size();
            }
            assert!(
                cursor <= stride as u64,
                "{name} attributes read {cursor} bytes past a {stride}-byte instance"
            );
        }
    }

    #[test]
    fn a_document_with_no_viewport_has_nothing_visible_to_upload_or_retain() {
        let doc = Document::new("p".into(), "t", 512, 512);

        assert_eq!(Renderer::visible_span(&doc), None);
        assert_eq!(
            Renderer::retained_span(&doc, MemoryPressureLevel::Normal.retention_margin_tiles()),
            None
        );
    }

    /// The retained span is the visible one plus a margin, and that margin is the whole
    /// eviction policy: tiles just outside the viewport are kept so a pan does not have to
    /// re-upload the row it is about to reach.
    #[test]
    fn the_retained_span_is_the_visible_span_grown_by_exactly_the_margin() {
        let doc = doc_with_viewport();

        let margin = MemoryPressureLevel::Normal.retention_margin_tiles();
        let (vx0, vy0, vx1, vy1) = Renderer::visible_span(&doc).expect("visible");
        let (rx0, ry0, rx1, ry1) = Renderer::retained_span(&doc, margin).expect("retained");

        assert_eq!((rx0, ry0), (vx0 - margin, vy0 - margin));
        assert_eq!((rx1, ry1), (vx1 + margin, vy1 + margin));
    }

    /// Zoomed in far enough that the paper is larger than the viewport, panning slides the
    /// span across the document — which is what decides the tiles the next frame has to have
    /// resident.
    #[test]
    fn panning_slides_the_visible_span_across_the_document() {
        let mut doc = doc_with_viewport();
        doc.camera.zoom = 2.0;
        doc.camera.pan_x = 0.0;
        doc.camera.pan_y = 0.0;
        let before = Renderer::visible_span(&doc).expect("visible");

        doc.camera.pan_x -= (TILE_SIZE as f32) * 4.0;
        let after = Renderer::visible_span(&doc).expect("visible");

        assert!(after.0 > before.0, "{before:?} -> {after:?}");
        assert!(after.2 > before.2, "{before:?} -> {after:?}");
        assert_eq!(after.1, before.1, "a horizontal pan leaves the rows alone");
        assert_eq!(after.3, before.3);

        doc.camera.pan_x += (TILE_SIZE as f32) * 4.0;
        assert_eq!(
            Renderer::visible_span(&doc),
            Some(before),
            "panning back lands on the same tiles, so a pan gesture uploads nothing new"
        );
    }

    /// Apple Silicon reports `IntegratedGpu`, so the mapping has to keep it distinguishable
    /// from a software adapter — `DeviceTier::classify` separates the two by limits, and it can
    /// only do that if this does not flatten them into the same kind first.
    #[test]
    fn every_adapter_kind_maps_to_the_one_the_tier_table_reads() {
        assert_eq!(gpu_kind(wgpu::DeviceType::DiscreteGpu), GpuKind::Discrete);
        assert_eq!(
            gpu_kind(wgpu::DeviceType::IntegratedGpu),
            GpuKind::Integrated
        );
        assert_eq!(gpu_kind(wgpu::DeviceType::VirtualGpu), GpuKind::Integrated);
        assert_eq!(gpu_kind(wgpu::DeviceType::Cpu), GpuKind::Software);
        assert_eq!(gpu_kind(wgpu::DeviceType::Other), GpuKind::Other);
    }

    fn progress(generation: u64, points: usize) -> CoverageProgress {
        CoverageProgress {
            generation,
            points,
            pan: (0.0, 0.0),
            zoom: 1.0,
            dpr: 2.0,
            brush: [8.0, 1.0, 0.0, 1.0],
            color: [0.1, 0.1, 0.1, 1.0],
        }
    }

    /// The ordinary frame: the pointer travelled, the camera did not, so the segments already
    /// unioned into the coverage target stay and only the new ones are drawn.
    #[test]
    fn a_stroke_that_only_grew_is_appended_to() {
        assert!(progress(7, 40).appendable(&progress(7, 41)));
        assert!(progress(7, 40).appendable(&progress(7, 40)));
    }

    /// `stroke_segment_count` maps one point to one segment — the degenerate capsule behind a
    /// tap's dot — and segment 0 replaces it rather than following it, so the first real capsule
    /// would never be drawn if this appended.
    #[test]
    fn the_one_point_dot_restarts_rather_than_being_appended_to() {
        assert!(!progress(7, 1).appendable(&progress(7, 2)));
        assert!(!progress(7, 0).appendable(&progress(7, 1)));
    }

    /// Two guards on the same failure, because `Max` blending cannot take a capsule back out of
    /// the target. `Document::push_stroke_point` bumps the generation when a Shift-held straight
    /// segment rewinds the list; the shorter-point-count test is what catches a rewind that
    /// forgot to.
    #[test]
    fn a_rewound_or_restarted_stroke_is_not_appendable() {
        assert!(!progress(7, 40).appendable(&progress(8, 41)));
        assert!(!progress(7, 40).appendable(&progress(7, 39)));
    }

    /// The coverage pass rasterizes in device pixels off the preview uniform, so pixels
    /// accumulated at one camera are in the wrong place at the next one — and a capsule carries
    /// the width and color it was drawn at.
    #[test]
    fn moving_the_camera_or_the_brush_invalidates_what_was_accumulated() {
        let base = progress(7, 40);
        let mut panned = progress(7, 41);
        panned.pan = (4.0, 0.0);
        assert!(!base.appendable(&panned));

        let mut zoomed = progress(7, 41);
        zoomed.zoom = 1.5;
        assert!(!base.appendable(&zoomed));

        let mut rescaled = progress(7, 41);
        rescaled.dpr = 1.0;
        assert!(!base.appendable(&rescaled));

        let mut resized = progress(7, 41);
        resized.brush[0] = 12.0;
        assert!(!base.appendable(&resized));

        let mut recolored = progress(7, 41);
        recolored.color = [1.0, 0.0, 0.0, 1.0];
        assert!(!base.appendable(&recolored));
    }
}

/// The layer table, exercised on a real device.
///
/// These build the tile and solid pipelines against the *same* `tile_shared_bgl` and the same
/// shader the app uses, so a disagreement between `LayerData` in Rust and `LayerData` in WGSL —
/// a field added on one side, a stride that stopped matching — fails here rather than showing up
/// as geometry in the wrong place on someone's board.
#[cfg(test)]
mod layer_table_tests {
    use super::*;
    use crate::test_gpu::{gpu, read_texture_layer, Gpu};
    use calumma_core::filters::{AdjustmentLut, Adjustments};
    use calumma_core::tile::{TILE_BYTES, TILE_SIZE};

    const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
    const RED: [u8; 4] = [255, 0, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];

    struct Fixture {
        bgl: wgpu::BindGroupLayout,
        camera: wgpu::Buffer,
        layers: wgpu::Buffer,
        samplers: TileSamplers,
        atlas: TileAtlas,
        target: wgpu::Texture,
    }

    impl Fixture {
        /// A slot in the atlas holding one flat colour, mip chain included so the sampler's
        /// choice of level cannot change what the test reads back.
        fn solid_slot(&mut self, gpu: &Gpu, rgba: [u8; 4]) -> u32 {
            let shared = SharedBindings {
                layout: &self.bgl,
                camera: &self.camera,
                layers: &self.layers,
                samplers: &self.samplers,
            };
            let slot = self
                .atlas
                .allocate(&gpu.device, &gpu.queue, &shared)
                .expect("slot");
            let base = rgba.repeat(TILE_BYTES / 4);
            let mut mips = Vec::new();
            let mut side = TILE_SIZE / 2;
            while side >= 1 {
                mips.push(rgba.repeat((side * side) as usize));
                if side == 1 {
                    break;
                }
                side /= 2;
            }
            self.atlas.write(&gpu.queue, slot, &base, &mips);
            slot
        }

        fn write_rows(&self, gpu: &Gpu, rows: &[LayerData]) {
            gpu.queue
                .write_buffer(&self.layers, 0, bytemuck::cast_slice(rows));
        }
    }

    /// One tile's worth of board, drawn 1:1 into a `TILE_SIZE` target: document pixel *n* lands
    /// on target pixel *n*, so a readback coordinate is a document coordinate and the mip level
    /// is 0 everywhere.
    fn fixture(gpu: &Gpu) -> Fixture {
        let bgl = tile_shared_bgl(&gpu.device);
        let camera = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tile-camera"),
            size: std::mem::size_of::<TileCamera>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let side = TILE_SIZE as f32;
        gpu.queue.write_buffer(
            &camera,
            0,
            bytemuck::bytes_of(&TileCamera {
                pan: [0.0, 0.0],
                zoom: 1.0,
                dpr: 1.0,
                viewport: [side, side],
                doc_size: [side, side],
                crisp: 0.0,
                _pad: [0.0; 3],
            }),
        );
        let layers = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("layer-data"),
            size: (LAYER_DATA_CAPACITY * std::mem::size_of::<LayerData>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let samplers = TileSamplers::new(&gpu.device);
        let atlas = TileAtlas::new(
            &gpu.device,
            &SharedBindings {
                layout: &bgl,
                camera: &camera,
                layers: &layers,
                samplers: &samplers,
            },
            8,
        );
        let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("layer-table-target"),
            size: wgpu::Extent3d {
                width: TILE_SIZE,
                height: TILE_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        Fixture {
            bgl,
            camera,
            layers,
            samplers,
            atlas,
            target,
        }
    }

    fn pipeline(
        gpu: &Gpu,
        f: &Fixture,
        vs: &str,
        fs: &str,
        instanced: bool,
    ) -> wgpu::RenderPipeline {
        let layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("tile-pl"),
                bind_group_layouts: &[Some(&f.bgl)],
                ..Default::default()
            });
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TileInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: TILE_INSTANCE_ATTRS,
        };
        let buffers: &[Option<wgpu::VertexBufferLayout>] = if instanced {
            &[Some(instance_layout)]
        } else {
            &[]
        };
        gpu.device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("layer-table-test"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &gpu.shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers,
                },
                fragment: Some(wgpu::FragmentState {
                    module: &gpu.shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(premultiplied_target(TARGET_FORMAT))],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
    }

    /// Runs one draw against a cleared target and hands back the rendered pixels.
    fn draw(
        gpu: &Gpu,
        f: &Fixture,
        pipeline: &wgpu::RenderPipeline,
        instances: &[TileInstance],
        range: std::ops::Range<u32>,
    ) -> Vec<u8> {
        let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tile-instances"),
            size: ((instances.len().max(1)) * std::mem::size_of::<TileInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !instances.is_empty() {
            gpu.queue
                .write_buffer(&buf, 0, bytemuck::cast_slice(instances));
        }
        let view = f
            .target
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("layer-table-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, f.atlas.bind_group(), &[]);
            if !instances.is_empty() {
                pass.set_vertex_buffer(0, buf.slice(..));
            }
            pass.draw(0..6, range);
        }
        gpu.queue.submit(Some(encoder.finish()));
        read_texture_layer(&gpu.device, &gpu.queue, &f.target, 0, TILE_SIZE)
    }

    fn pixel(image: &[u8], x: u32, y: u32) -> [u8; 4] {
        let i = ((y * TILE_SIZE + x) * 4) as usize;
        [image[i], image[i + 1], image[i + 2], image[i + 3]]
    }

    /// The whole point of the table: two tiles in **one** instanced draw, transformed
    /// differently, because each instance names its own row. Under the per-layer uniform this
    /// needed two draws with a bind group swap between them, and a single draw could only ever
    /// place every tile with one transform.
    #[test]
    fn an_instance_is_transformed_by_the_row_its_layer_index_names() {
        let Some(gpu) = gpu() else { return };
        let mut f = fixture(gpu);
        let red = f.solid_slot(gpu, RED);
        let blue = f.solid_slot(gpu, BLUE);
        let shift = (TILE_SIZE / 2) as f32;
        f.write_rows(
            gpu,
            &[
                LayerData::default(),
                LayerData {
                    offset: [shift, 0.0],
                    ..LayerData::default()
                },
            ],
        );
        let pipe = pipeline(gpu, &f, "vs_tile", "fs_tile", true);

        let image = draw(
            gpu,
            &f,
            &pipe,
            &[
                TileInstance {
                    origin: [0.0, 0.0],
                    slot: red,
                    layer_index: 0,
                },
                TileInstance {
                    origin: [0.0, 0.0],
                    slot: blue,
                    layer_index: 1,
                },
            ],
            0..2,
        );

        assert_eq!(
            pixel(&image, 8, 8),
            RED,
            "row 0 is identity, so the red tile sits where its origin says"
        );
        assert_eq!(
            pixel(&image, TILE_SIZE - 8, 8),
            BLUE,
            "row 1 offsets by half a tile, so the blue tile covers the right half — same draw, \
             same origin, different row"
        );
    }

    /// Both tiles carry the *same* origin and the same row; nothing should move. Guards against
    /// a shader that reads a row by something other than the index it was handed — an instance
    /// counter, say — which the test above alone would not catch.
    #[test]
    fn two_instances_sharing_a_row_land_in_the_same_place() {
        let Some(gpu) = gpu() else { return };
        let mut f = fixture(gpu);
        let red = f.solid_slot(gpu, RED);
        let blue = f.solid_slot(gpu, BLUE);
        f.write_rows(
            gpu,
            &[
                LayerData::default(),
                LayerData {
                    offset: [TILE_SIZE as f32, 0.0],
                    ..LayerData::default()
                },
            ],
        );
        let pipe = pipeline(gpu, &f, "vs_tile", "fs_tile", true);

        let image = draw(
            gpu,
            &f,
            &pipe,
            &[
                TileInstance {
                    origin: [0.0, 0.0],
                    slot: red,
                    layer_index: 0,
                },
                TileInstance {
                    origin: [0.0, 0.0],
                    slot: blue,
                    layer_index: 0,
                },
            ],
            0..2,
        );

        assert_eq!(
            pixel(&image, TILE_SIZE - 8, 8),
            BLUE,
            "the second instance read row 0 like the first, so it covers the first everywhere"
        );
        assert_eq!(pixel(&image, 8, 8), BLUE);
    }

    /// A layer's transform reaches the shader through the row, not through a bind group: scale
    /// about the pivot has to survive the move to the table.
    #[test]
    fn a_rows_scale_and_pivot_still_place_the_tile() {
        let Some(gpu) = gpu() else { return };
        let mut f = fixture(gpu);
        let red = f.solid_slot(gpu, RED);
        let centre = (TILE_SIZE / 2) as f32;
        f.write_rows(
            gpu,
            &[LayerData {
                pivot: [centre, centre],
                scale: [0.5, 0.5],
                ..LayerData::default()
            }],
        );
        let pipe = pipeline(gpu, &f, "vs_tile", "fs_tile", true);

        let image = draw(
            gpu,
            &f,
            &pipe,
            &[TileInstance {
                origin: [0.0, 0.0],
                slot: red,
                layer_index: 0,
            }],
            0..1,
        );

        assert_eq!(
            pixel(&image, centre as u32, centre as u32),
            RED,
            "half scale about the centre keeps the middle covered"
        );
        assert_eq!(
            pixel(&image, 8, 8),
            [0, 0, 0, 0],
            "and pulls the corner in, leaving it clear"
        );
    }

    /// Solid Paper has no instance buffer to carry an atlas slot, so its row holds one and the
    /// draw names the row through its instance range. This is what replaced bitcasting the slot
    /// into `pivot.x` — two draw paths reading the same bytes as different types.
    #[test]
    fn the_solid_quad_reads_its_atlas_slot_from_the_row_the_draw_range_names() {
        let Some(gpu) = gpu() else { return };
        let mut f = fixture(gpu);
        let red = f.solid_slot(gpu, RED);
        let blue = f.solid_slot(gpu, BLUE);
        f.write_rows(
            gpu,
            &[
                LayerData {
                    atlas_slot: red,
                    ..LayerData::default()
                },
                LayerData {
                    atlas_slot: blue,
                    ..LayerData::default()
                },
            ],
        );
        let pipe = pipeline(gpu, &f, "vs_doc_quad", "fs_solid_tile", false);

        let image = draw(gpu, &f, &pipe, &[], 1..2);

        assert_eq!(
            pixel(&image, 8, 8),
            BLUE,
            "instance range 1..2 selects row 1, whose atlas slot is the blue tile"
        );
        assert_eq!(pixel(&image, TILE_SIZE - 8, TILE_SIZE - 8), BLUE);
    }

    /// The Rust row and the WGSL row have to agree byte for byte, and nothing in the type system
    /// enforces it. 1072 bytes is also what makes the WGSL array stride 1072 with no tail
    /// padding (the struct's own alignment is 8, from the three `vec2<f32>` fields, and 1072 is
    /// already a multiple of 8) — a mismatch here misaddresses every row past the first. Plan 23
    /// grew this from 32 to 1072 deliberately; see `LayerData`'s own doc comment for the layout.
    #[test]
    fn a_table_row_is_the_size_the_shader_strides_by() {
        assert_eq!(std::mem::size_of::<LayerData>(), 1072);
        assert_eq!(std::mem::align_of::<LayerData>(), 4);
        assert_eq!(
            std::mem::size_of::<TileInstance>(),
            16,
            "layer_index took the place of padding, so instances did not grow"
        );
    }

    /// Renders one tile of `combos.len()` distinct texels (row-major, one combo per texel)
    /// through `fs_tile` with `row` as its only `LayerData` entry, and hands back the target's
    /// pixels. The target is a *separate* sRGB texture, not `Fixture::TARGET_FORMAT`: `fs_tile`
    /// hands back linear light for correct blending (see the comment above `linear_to_srgb` in
    /// board.wgsl), and only an sRGB target's automatic re-encode on write turns that back into
    /// the same sRGB-encoded byte `AdjustmentLut::apply` computes on the CPU.
    fn render_byte_cube(gpu: &Gpu, f: &Fixture, slot: u32, row: LayerData) -> Vec<u8> {
        f.write_rows(gpu, &[row]);

        let srgb_format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("byte-cube-target"),
            size: wgpu::Extent3d {
                width: TILE_SIZE,
                height: TILE_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: srgb_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("byte-cube-pl"),
                bind_group_layouts: &[Some(&f.bgl)],
                ..Default::default()
            });
        let pipe = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("byte-cube-test"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &gpu.shader,
                    entry_point: Some("vs_tile"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<TileInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: TILE_INSTANCE_ATTRS,
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &gpu.shader,
                    entry_point: Some("fs_tile"),
                    compilation_options: Default::default(),
                    targets: &[Some(premultiplied_target(srgb_format))],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });

        let instance = TileInstance {
            origin: [0.0, 0.0],
            slot,
            layer_index: 0,
        };
        let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("byte-cube-instance"),
            size: std::mem::size_of::<TileInstance>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        gpu.queue
            .write_buffer(&buf, 0, bytemuck::bytes_of(&instance));

        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("byte-cube-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipe);
            pass.set_bind_group(0, f.atlas.bind_group(), &[]);
            pass.set_vertex_buffer(0, buf.slice(..));
            pass.draw(0..6, 0..1);
        }
        gpu.queue.submit(Some(encoder.finish()));
        read_texture_layer(&gpu.device, &gpu.queue, &target, 0, TILE_SIZE)
    }

    /// `fs_tile`'s `apply_adjustments` and `core::filters::AdjustmentLut::apply` are two
    /// independent implementations of the same math — one WGSL, one Rust — kept in step by
    /// hand. This is the test that actually enforces it, over a stratified sample of the byte
    /// cube covering both `LUT_MODE_TONE` (tone only) and `LUT_MODE_TONE_HSL` (tone + hue/sat).
    /// A 1-of-255 tolerance absorbs the sRGB round trip: `apply_adjustments` undoes the atlas
    /// texture's automatic sRGB decode in software (`linear_to_srgb`/`srgb_to_linear`) so the
    /// lookup lands on the same byte the CPU path would use, and redoes it in software before
    /// the GPU's own hardware re-encodes on write to the sRGB target — two curves computed two
    /// different ways, not required to be bit-identical.
    #[test]
    fn fs_tile_adjustments_agree_with_the_cpu_lut_over_a_byte_cube() {
        let Some(gpu) = gpu() else { return };
        let mut f = fixture(gpu);

        const STEPS: [u8; 9] = [0, 32, 64, 96, 128, 160, 192, 224, 255];
        let mut combos: Vec<[u8; 3]> = Vec::new();
        for &r in &STEPS {
            for &g in &STEPS {
                for &b in &STEPS {
                    combos.push([r, g, b]);
                }
            }
        }
        assert!(combos.len() <= (TILE_SIZE * TILE_SIZE) as usize);

        let mut base = vec![0u8; TILE_BYTES];
        for (i, rgb) in combos.iter().enumerate() {
            let px = i * 4;
            base[px] = rgb[0];
            base[px + 1] = rgb[1];
            base[px + 2] = rgb[2];
            base[px + 3] = 255;
        }
        let shared = SharedBindings {
            layout: &f.bgl,
            camera: &f.camera,
            layers: &f.layers,
            samplers: &f.samplers,
        };
        let slot = f
            .atlas
            .allocate(&gpu.device, &gpu.queue, &shared)
            .expect("slot");
        f.atlas.write(&gpu.queue, slot, &base, &[]);

        for adjustments in [
            // Tone only: saturation and vibrance neutral, so `write_layer_data` would pick
            // `LUT_MODE_TONE` and the shader never enters `hsl_stage`.
            Adjustments {
                brightness: 0.15,
                contrast: 0.2,
                vibrance: 0.0,
                saturation: 0.0,
                levels_gamma: 1.4,
            },
            // Tone + HSL: exercises `rgb_to_hsl` / `hue_to_rgb` / `hsl_to_rgb` too.
            Adjustments {
                brightness: 0.15,
                contrast: 0.2,
                vibrance: 0.3,
                saturation: -0.25,
                levels_gamma: 1.4,
            },
        ] {
            let lut = AdjustmentLut::new(&adjustments);
            let row = if lut.is_tone_only() {
                LayerData {
                    tone: *lut.tone_table(),
                    lut_mode: LUT_MODE_TONE,
                    ..LayerData::default()
                }
            } else {
                LayerData {
                    tone: *lut.tone_table(),
                    lut_mode: LUT_MODE_TONE_HSL,
                    saturation: adjustments.saturation,
                    vibrance: adjustments.vibrance,
                    ..LayerData::default()
                }
            };

            let image = render_byte_cube(gpu, &f, slot, row);

            let mut max_diff = 0i32;
            for (i, rgb) in combos.iter().enumerate() {
                let expected = lut.apply(*rgb);
                let got = pixel(&image, (i as u32) % TILE_SIZE, (i as u32) / TILE_SIZE);
                assert_eq!(got[3], 255, "alpha is untouched by adjustments");
                for c in 0..3 {
                    max_diff = max_diff.max((got[c] as i32 - expected[c] as i32).abs());
                }
            }
            assert!(
                max_diff <= 1,
                "GPU and CPU adjustments disagree by more than 1 of 255 somewhere (max {max_diff}, lut_mode {})",
                row.lut_mode
            );
        }
    }
}
