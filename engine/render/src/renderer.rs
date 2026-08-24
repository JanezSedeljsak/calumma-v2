use crate::compose::{
    composited_tile_payload, guide_instances, layer_highlight_instances, rgba_unit,
    selection_lasso_points, selection_mask_edges, selection_rect_or_ellipse, stroke_instances,
    text_overlay_instances, tile_upload_mips, transform_overlay_instances, GuideInstance,
    StrokeInstance,
};
use crate::framebuffer::{self, PanCache, PxRect};
use crate::overview::OverviewPass;
use crate::stroke_coverage::StrokeCoverage;
use crate::tile_atlas::TileAtlas;
use crate::vector_draw::{
    item_visible, push_path_instances, shape_instance, vector_placement,
    vector_selection_instances, VectorShapeInstance,
};
use bytemuck::{Pod, Zeroable};
use calumma_core::filters::AdjustmentLut;
use calumma_core::limits::{
    CAMERA_MOTION_IDLE_FRAMES, GPU_TILE_RETENTION_MARGIN_TILES, GUIDES_LIMIT,
    STROKE_INSTANCE_CAPACITY, SURFACE_FRAME_LATENCY, TILE_ATLAS_MAX_CAPACITY,
    TILE_INSTANCE_CAPACITY, VECTOR_SHAPE_INSTANCE_CAPACITY,
};
use calumma_core::tile::{DirtyChannel, TileCoord, TileGrid};
use calumma_core::{BlendMode, BrushProfile, Document, Tool, VectorItem};
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
struct PaperUniforms {
    pan: [f32; 2],
    zoom: f32,
    dpr: f32,
    doc_size: [f32; 2],
    viewport: [f32; 2],
    dark: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    desk: [f32; 4],
    grid: [f32; 4],
    paper_border: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TileCamera {
    pan: [f32; 2],
    zoom: f32,
    dpr: f32,
    viewport: [f32; 2],
    doc_size: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SolidLayerUniform {
    slot: u32,
    _pad: [u32; 7],
}

/// One visible tile: where it sits in document space, and which array layer of the shared
/// atlas holds its pixels. This is the whole per-tile payload now — everything else a tile
/// draw needs (camera, the atlas itself, the layer's transform) is bound once per document
/// layer rather than once per tile.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TileInstance {
    origin: [f32; 2],
    slot: u32,
    _pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LayerXform {
    pivot: [f32; 2],
    offset: [f32; 2],
    scale: [f32; 2],
    rotation: f32,
    _pad: f32,
}

impl Default for LayerXform {
    fn default() -> Self {
        Self {
            pivot: [0.0, 0.0],
            offset: [0.0, 0.0],
            scale: [1.0, 1.0],
            rotation: 0.0,
            _pad: 0.0,
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
    /// buffer — one instanced draw regardless of how many tiles that is. The layer id looks up
    /// its transform bind group at draw time.
    Tiles(BlendMode, String, std::ops::Range<u32>),
    Solid(BlendMode, String),
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
    guide_pipeline: wgpu::RenderPipeline,
    stroke_coverage: StrokeCoverage,
    shape_pipeline: wgpu::RenderPipeline,
    vector_shape_pipeline: wgpu::RenderPipeline,
    paper_buf: wgpu::Buffer,
    paper_bg: wgpu::BindGroup,
    tile_shared_bgl: wgpu::BindGroupLayout,
    tile_layer_bgl: wgpu::BindGroupLayout,
    tile_camera_buf: wgpu::Buffer,
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
    sampler: wgpu::Sampler,
    atlas: TileAtlas,
    tiles: HashMap<TileKey, GpuTile>,
    layer_xforms: HashMap<String, (wgpu::Buffer, wgpu::BindGroup)>,
    layer_slots: HashMap<String, u32>,
    next_layer_slot: u32,
    started: Instant,
    frame_dirty: FrameDirty,
    cached_retained_span: Option<(i32, i32, i32, i32)>,
    cached_visible_span: Option<(i32, i32, i32, i32)>,
    cached_tile_instances: Vec<TileInstance>,
    cached_strokes: Vec<StrokeInstance>,
    overlay_scratch: Vec<StrokeInstance>,
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
}

impl Renderer {
    pub fn from_surface(
        surface: wgpu::Surface<'static>,
        instance: &wgpu::Instance,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
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
        let atlas_max_capacity = adapter
            .limits()
            .max_texture_array_layers
            .min(TILE_ATLAS_MAX_CAPACITY);

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("calumma-render"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits {
                max_texture_array_layers: atlas_max_capacity,
                ..wgpu::Limits::default()
            },
            memory_hints: wgpu::MemoryHints::Performance,
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
            entries: &[uniform_entry(0, std::mem::size_of::<PaperUniforms>())],
        });
        let paper_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("paper-uniform"),
            size: std::mem::size_of::<PaperUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let paper_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("paper-bg"),
            layout: &paper_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: paper_buf.as_entire_binding(),
            }],
        });
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

        // Group 0: the shared tile atlas plus the per-frame camera, bound once regardless of
        // how many document layers draw this frame. Group 1: the one thing that still varies
        // per layer, its transform.
        let tile_shared_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
            ],
        });
        let tile_layer_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tile-layer-bgl"),
            entries: &[uniform_entry(0, std::mem::size_of::<LayerXform>())],
        });
        let tile_camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tile-camera"),
            size: std::mem::size_of::<TileCamera>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("tile-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            // Tiles carry a full mip chain (`TileAtlas`/`compose::tile_mip_chain`) precisely so
            // this can blend between levels: `fs_tile` samples with automatic LOD, and without
            // this the GPU would still pick the right mip but snap to it instead of blending,
            // which shows up as visible seams sweeping across the board while zooming.
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let atlas = TileAtlas::new(
            &device,
            &tile_shared_bgl,
            &tile_camera_buf,
            &sampler,
            atlas_max_capacity,
        );
        let tile_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tile-pl"),
            bind_group_layouts: &[Some(&tile_shared_bgl), Some(&tile_layer_bgl)],
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
            guide_pipeline,
            stroke_coverage,
            shape_pipeline,
            vector_shape_pipeline,
            paper_buf,
            paper_bg,
            tile_shared_bgl,
            tile_layer_bgl,
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
            sampler,
            atlas,
            tiles: HashMap::new(),
            layer_xforms: HashMap::new(),
            layer_slots: HashMap::new(),
            next_layer_slot: 0,
            started: Instant::now(),
            frame_dirty: FrameDirty::Content,
            cached_retained_span: None,
            cached_visible_span: None,
            cached_tile_instances: Vec::new(),
            cached_strokes: Vec::new(),
            overlay_scratch: Vec::new(),
            cached_shapes: Vec::new(),
            cached_draws: Vec::new(),
            overview,
            camera_motion: false,
            motion_idle_frames: 0,
            cached_tile_draw_count: None,
            base_only_tiles: FxHashSet::default(),
            pan_cache,
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

    fn write_solid_layer_slot(&self, layer_id: &str, slot: u32) {
        let Some((buf, _)) = self.layer_xforms.get(layer_id) else {
            return;
        };
        let uniform = SolidLayerUniform { slot, _pad: [0; 7] };
        self.queue
            .write_buffer(buf, 0, bytemuck::bytes_of(&uniform));
    }

    fn visible_span(doc: &Document) -> Option<(i32, i32, i32, i32)> {
        doc.visible_rect().map(|visible| visible.tile_span())
    }

    pub fn request_overview_prewarm(&mut self) {
        self.overview.request_prewarm();
    }

    fn retained_span(doc: &Document) -> Option<(i32, i32, i32, i32)> {
        doc.visible_rect().map(|visible| {
            visible
                .expanded_by_tiles(GPU_TILE_RETENTION_MARGIN_TILES)
                .tile_span()
        })
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
        self.cached_retained_span = Self::retained_span(doc);
        self.cached_visible_span = Self::visible_span(doc);
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
            for coord in grid.coords_intersecting(visible) {
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
            count += grid.coords_intersecting(visible).count();
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

    fn ensure_layer_xform(&mut self, layer_id: &str) {
        if self.layer_xforms.contains_key(layer_id) {
            return;
        }
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("layer-transform"),
            size: std::mem::size_of::<LayerXform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue
            .write_buffer(&buf, 0, bytemuck::bytes_of(&LayerXform::default()));
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tile-layer-bg"),
            layout: &self.tile_layer_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buf.as_entire_binding(),
            }],
        });
        self.layer_xforms
            .insert(layer_id.to_string(), (buf, bind_group));
    }

    /// Every layer `sync_tiles` uploads needs its uniform written, text included — the
    /// buffer is created zeroed, and a zeroed `LayerXform` scales the tile to nothing.
    fn write_layer_transforms(&mut self, doc: &Document) {
        for layer in &doc.layers {
            if layer.tiles().is_none() {
                continue;
            }
            if layer.is_paper() && layer.tiles().is_some_and(|g| g.whole_tiles_share_one_arc()) {
                continue;
            }
            self.ensure_layer_xform(&layer.id);
            let Some((buf, _)) = self.layer_xforms.get(&layer.id) else {
                continue;
            };
            let xform = match (layer.transform, layer.content_bounds()) {
                (Some(t), Some(bounds)) => LayerXform {
                    pivot: [(bounds.0 + bounds.2) * 0.5, (bounds.1 + bounds.3) * 0.5],
                    offset: [t.offset_x, t.offset_y],
                    scale: [t.scale_x, t.scale_y],
                    rotation: t.rotation,
                    _pad: 0.0,
                },
                _ => LayerXform::default(),
            };
            self.queue.write_buffer(buf, 0, bytemuck::bytes_of(&xform));
        }
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

    /// Hand back everything that belonged to the document being closed — the atlas's slots and
    /// the per-layer uniform buffers keyed by its layer ids. Eviction otherwise only happens
    /// inside `sync_tiles`, which needs a document to run, so a closed project's textures
    /// would sit in VRAM until some *other* project was opened and drawn.
    pub fn release_document(&mut self) {
        self.tiles.clear();
        self.base_only_tiles.clear();
        self.atlas.clear();
        self.layer_xforms.clear();
        self.layer_slots.clear();
        self.next_layer_slot = 0;
        self.clear_layer_cache();
        self.overview.clear();
        self.pan_cache.invalidate();
        self.stroke_coverage.release();
        self.frame_dirty = FrameDirty::Content;
    }

    pub fn invalidate(&mut self) {
        self.frame_dirty = FrameDirty::Content;
        self.cached_retained_span = None;
        self.cached_visible_span = None;
        self.cached_tile_draw_count = None;
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
        let retained = visible.expanded_by_tiles(GPU_TILE_RETENTION_MARGIN_TILES);
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
            self.ensure_layer_xform(&layer.id);
            let dirty = grid.dirty_tiles(DirtyChannel::Render);

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

            for coord in grid.coords_intersecting(retained) {
                let cell = TileGrid::tile_rect(coord);
                let key: TileKey = (slot, coord.x, coord.y);
                live.insert(key);
                if !cell.intersects(visible) {
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

        // Bake mask/adjustments/opacity for every dirty tile up front and in parallel —
        // dragging a filter slider re-composites the whole visible tile set each frame,
        // which is the one CPU cost that scales with viewport size at 60fps. The wgpu
        // calls below stay sequential; only the pixel math goes wide.
        //
        // Building an `AdjustmentLut` walks 256 tone entries per layer, so it's scoped to
        // just the layers that actually have tiles in `uploads` this frame — panning or
        // zooming re-enters `render()` every frame via `dirty` without marking any tile
        // dirty, and rebuilding every adjusted layer's LUT on each of those frames for no
        // reason was pure waste.
        let needed_layers: FxHashSet<usize> = uploads.iter().map(|(li, _, _, _)| *li).collect();
        let luts: HashMap<usize, Option<AdjustmentLut>> = needed_layers
            .into_iter()
            .map(|li| (li, doc.layers[li].adjustments.map(|a| a.lut())))
            .collect();
        // The mip chain is built here too, alongside the mask/adjustment bake — both are pure
        // pixel math that scales with tile count, so both go through rayon rather than running
        // sequentially on the frame thread once the wgpu upload loop below gets to them.
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
                let lut = luts.get(layer_index).and_then(|l| l.as_ref());
                let composited = composited_tile_payload(pixels, *coord, layer, lut, doc_width);
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

            let array_layer = match self.atlas.allocate(
                &self.device,
                &self.queue,
                &self.tile_shared_bgl,
                &self.tile_camera_buf,
                &self.sampler,
            ) {
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
                    let Some(slot) = self.atlas.allocate(
                        &self.device,
                        &self.queue,
                        &self.tile_shared_bgl,
                        &self.tile_camera_buf,
                        &self.sampler,
                    ) else {
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
        self.layer_xforms
            .retain(|id, _| live_layers.contains(id.as_str()));

        for (layer_index, coord) in uploaded {
            if let Some(grid) = doc.layers.get_mut(layer_index).and_then(|l| l.tiles_mut()) {
                grid.clear_dirty_tile(DirtyChannel::Render, coord);
            }
        }
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
        for layer in &doc.layers {
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
                let key: TileKey = (slot, 0, 0);
                if self.tiles.contains_key(&key) {
                    self.ensure_layer_xform(&layer.id);
                    if let Some(gpu) = self.tiles.get(&key) {
                        self.write_solid_layer_slot(&layer.id, gpu.array_layer);
                        out.push(LayerDraw::Solid(layer.blend_mode, layer.id.clone()));
                    }
                }
                continue;
            }
            let start = tiles.len() as u32;
            for coord in grid.coords_intersecting(visible) {
                let key: TileKey = (slot, coord.x, coord.y);
                let Some(gpu) = self.tiles.get(&key) else {
                    continue;
                };
                let (ox, oy) = coord.origin();
                tiles.push(TileInstance {
                    origin: [ox as f32, oy as f32],
                    slot: gpu.array_layer,
                    _pad: 0,
                });
            }
            if tiles.len() as u32 > start {
                out.push(LayerDraw::Tiles(
                    layer.blend_mode,
                    layer.id.clone(),
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
                LayerDraw::Tiles(mode, layer_id, range) => {
                    let Some((_, xform_bg)) = self.layer_xforms.get(layer_id) else {
                        continue;
                    };
                    pass.set_pipeline(self.tile_pipeline(*mode));
                    pass.set_bind_group(0, self.atlas.bind_group(), &[]);
                    pass.set_bind_group(1, xform_bg, &[]);
                    pass.set_vertex_buffer(0, self.tile_instance_buf.slice(..));
                    pass.draw(0..6, range.clone());
                }
                LayerDraw::Solid(mode, layer_id) => {
                    let Some((_, xform_bg)) = self.layer_xforms.get(layer_id) else {
                        continue;
                    };
                    pass.set_pipeline(self.solid_pipeline(*mode));
                    pass.set_bind_group(0, self.atlas.bind_group(), &[]);
                    pass.set_bind_group(1, xform_bg, &[]);
                    pass.draw(0..6, 0..1);
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
        if self.frame_dirty == FrameDirty::Clean
            && !doc.has_live_preview()
            && !doc.has_animated_overlay()
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
                || Self::retained_span(doc) != self.cached_retained_span
                || self.visible_needs_gpu_upload(doc));
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
                self.write_layer_transforms(doc);
                self.sync_tiles(doc);
            }
            if need_draw_rebuild {
                self.rebuild_layer_cache(doc);
            }
        }

        let paper = PaperUniforms {
            pan: [doc.camera.pan_x, doc.camera.pan_y],
            zoom: doc.camera.zoom,
            dpr: doc.camera.dpr,
            doc_size: [doc.width as f32, doc.height as f32],
            viewport,
            dark: if doc.dark_theme { 1.0 } else { 0.0 },
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
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
        let mut brush_range = 0u32..0u32;
        if !camera_only {
            let radius = doc.brush_size * 0.5;
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
            let prefix_len = if self.camera_motion {
                0
            } else {
                self.cached_strokes.len()
            };
            let mut instances = std::mem::take(&mut self.overlay_scratch);
            instances.clear();
            let overlay_start = prefix_len as u32;
            if doc.text_editing() {
                instances.extend(text_overlay_instances(
                    doc,
                    self.started.elapsed().as_secs_f32(),
                ));
            } else if doc.previews_brush_stroke() {
                brush_instances = stroke_instances(
                    &doc.stroke_points,
                    radius,
                    stroke_color,
                    &doc.active_brush_profile(),
                );
            } else if !doc.stroke_points.is_empty() && doc.tool.previews_stroke() {
                instances.extend(stroke_instances(
                    &doc.stroke_points,
                    radius,
                    stroke_color,
                    &BrushProfile::HARD,
                ));
            } else if let Some(handles) = doc.transform_handles() {
                instances.extend(transform_overlay_instances(handles));
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
            instances.extend(vector_selection_instances(doc));
            if let Some((index, corners)) = doc.layer_highlight() {
                let covered = doc
                    .transform_handles()
                    .is_some_and(|(handle_index, _, _)| handle_index == index);
                if !covered {
                    instances.extend(layer_highlight_instances(
                        corners,
                        self.started.elapsed().as_secs_f32(),
                    ));
                }
            }
            overlay_range = overlay_start..overlay_start + instances.len() as u32;
            let brush_start = overlay_range.end;
            instances.append(&mut brush_instances);
            brush_range = brush_start..prefix_len as u32 + instances.len() as u32;
            if !brush_range.is_empty() {
                self.stroke_coverage
                    .ensure(&self.device, self.config.width, self.config.height);
            }
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

        if !brush_range.is_empty() {
            // TODO(F4): pass the segments added since the last frame and `restart: false`.
            // `accumulate` already appends correctly; the renderer still has to track how much
            // of the stroke is in the target and when the camera/brush invalidates it.
            self.stroke_coverage.accumulate(
                &mut encoder,
                &self.preview_bg,
                &self.stroke_buf,
                brush_range.clone(),
                scissor,
                true,
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

                // Under the transform box and the marching ants, over the artwork: a guide is
                // something the picture is aligned against, not something drawn on it.
                if guide_count > 0 {
                    pass.set_pipeline(&self.guide_pipeline);
                    pass.set_bind_group(0, &self.preview_bg, &[]);
                    pass.set_vertex_buffer(0, self.guide_buf.slice(..));
                    pass.draw(0..6, 0..guide_count);
                }

                if !overlay_range.is_empty() {
                    pass.set_pipeline(&self.stroke_pipeline);
                    pass.set_bind_group(0, &self.preview_bg, &[]);
                    pass.set_vertex_buffer(0, self.stroke_buf.slice(..));
                    pass.draw(0..6, overlay_range.clone());
                }

                if !brush_range.is_empty() {
                    self.stroke_coverage.composite(&mut pass, &self.preview_bg);
                }

                if preview_shape.is_some() {
                    pass.set_pipeline(&self.shape_pipeline);
                    pass.set_bind_group(0, &self.preview_bg, &[]);
                    pass.draw(0..3, 0..1);
                }
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
        self.frame_dirty = if doc.has_live_preview() || doc.has_animated_overlay() {
            FrameDirty::Overlay
        } else {
            FrameDirty::Clean
        };
    }
}

const ERASER_PREVIEW_COLOR: [f32; 4] = [0.5, 0.5, 0.5, 0.5];
const SELECTION_OUTLINE_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 0.9];
const SELECTION_OUTLINE_WIDTH: f32 = 1.5;
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
