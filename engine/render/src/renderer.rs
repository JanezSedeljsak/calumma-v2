use crate::compose::{
    composited_tile_payload, layer_highlight_instances, rgba_unit, selection_lasso_points,
    selection_rect_or_ellipse, stroke_instances, text_overlay_instances, tile_mip_chain,
    transform_overlay_instances, StrokeInstance,
};
use crate::tile_atlas::TileAtlas;
use crate::vector_draw::{
    item_visible, push_path_instances, shape_instance, vector_placement,
    vector_selection_instances, VectorShapeInstance,
};
use bytemuck::{Pod, Zeroable};
use calumma_core::filters::AdjustmentLut;
use calumma_core::limits::{
    GPU_TILE_RETENTION_MARGIN_TILES, STROKE_INSTANCE_CAPACITY, SURFACE_FRAME_LATENCY,
    TILE_ATLAS_MAX_CAPACITY, TILE_INSTANCE_CAPACITY, VECTOR_SHAPE_INSTANCE_CAPACITY,
};
use calumma_core::tile::{DirtyChannel, TileCoord, TileGrid};
use calumma_core::{BlendMode, Document, Tool, VectorItem};
use rayon::prelude::*;
use rustc_hash::FxHashSet;
use std::collections::HashMap;
use std::num::NonZeroU64;
use std::time::Instant;

type TileKey = (u32, i32, i32);

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
    _pad: [f32; 2],
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
struct PreviewUniforms {
    pan: [f32; 2],
    zoom: f32,
    dpr: f32,
    viewport: [f32; 2],
    _align_color: [f32; 2],
    color: [f32; 4],
    p0: [f32; 2],
    p1: [f32; 2],
    half_width: f32,
    tool: f32,
    fill: f32,
    _pad: f32,
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
    Vector(VectorRun, std::ops::Range<u32>),
}

/// Append an instance range, growing the previous entry instead when it is the same kind and
/// ends where this one starts. Item order survives — a shape between two paths still splits
/// the run — while the common layer, all of one kind, collapses to a single draw call.
fn extend_run(out: &mut Vec<LayerDraw>, kind: VectorRun, range: std::ops::Range<u32>) {
    if range.is_empty() {
        return;
    }
    if let Some(LayerDraw::Vector(prev_kind, prev)) = out.last_mut() {
        if *prev_kind == kind && prev.end == range.start {
            prev.end = range.end;
            return;
        }
    }
    out.push(LayerDraw::Vector(kind, range));
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
    stroke_pipeline: wgpu::RenderPipeline,
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
    dirty: bool,
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

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::default(),
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
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

        Ok(Self {
            device,
            queue,
            surface,
            config,
            paper_pipeline,
            tile_pipeline_normal,
            tile_pipeline_multiply,
            tile_pipeline_screen,
            stroke_pipeline,
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
            dirty: true,
        })
    }

    fn tile_pipeline(&self, mode: BlendMode) -> &wgpu::RenderPipeline {
        match mode {
            BlendMode::Normal => &self.tile_pipeline_normal,
            BlendMode::Multiply => &self.tile_pipeline_multiply,
            BlendMode::Screen => &self.tile_pipeline_screen,
        }
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
        self.atlas.clear();
        self.layer_xforms.clear();
        self.layer_slots.clear();
        self.next_layer_slot = 0;
        self.dirty = true;
    }

    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if self.config.width != width || self.config.height != height {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            self.dirty = true;
        }
    }

    fn ensure_stroke_capacity(&mut self, count: usize) {
        if count <= self.stroke_capacity {
            return;
        }
        let next = count.next_power_of_two().max(STROKE_INSTANCE_CAPACITY);
        self.stroke_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stroke-instances"),
            size: (next * std::mem::size_of::<StrokeInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.stroke_capacity = next;
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

    fn sync_tiles(&mut self, doc: &mut Document) {
        let Some(visible) = doc.visible_rect() else {
            return;
        };
        let retained = visible.expanded_by_tiles(GPU_TILE_RETENTION_MARGIN_TILES);
        let doc_width = doc.width;

        let mut live: FxHashSet<TileKey> = FxHashSet::default();
        let mut visible_keys: FxHashSet<TileKey> = FxHashSet::default();
        let mut uploads: Vec<(usize, TileCoord, TileKey)> = Vec::new();

        for layer_index in 0..doc.layers.len() {
            let layer = &doc.layers[layer_index];
            if !layer.visible {
                continue;
            }
            let Some(grid) = layer.tiles() else {
                continue;
            };
            let slot = self.layer_slot(&layer.id);
            self.ensure_layer_xform(&layer.id);
            let dirty = grid.dirty_tiles(DirtyChannel::Render);

            for (coord, _) in grid.iter() {
                let cell = TileGrid::tile_rect(coord);
                if !cell.intersects(retained) {
                    continue;
                }
                let key: TileKey = (slot, coord.x, coord.y);
                live.insert(key);
                if !cell.intersects(visible) {
                    continue;
                }
                visible_keys.insert(key);
                let known = self.tiles.contains_key(&key);
                if known && !dirty.contains(&coord) {
                    continue;
                }
                uploads.push((layer_index, coord, key));
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
        let needed_layers: FxHashSet<usize> = uploads.iter().map(|(li, _, _)| *li).collect();
        let luts: HashMap<usize, Option<AdjustmentLut>> = needed_layers
            .into_iter()
            .map(|li| (li, doc.layers[li].adjustments.map(|a| a.lut())))
            .collect();
        // The mip chain is built here too, alongside the mask/adjustment bake — both are pure
        // pixel math that scales with tile count, so both go through rayon rather than running
        // sequentially on the frame thread once the wgpu upload loop below gets to them.
        let payloads: Vec<Option<Vec<Vec<u8>>>> = uploads
            .par_iter()
            .map(|(layer_index, coord, _)| {
                let layer = doc.layers.get(*layer_index)?;
                let pixels = layer.tiles()?.get(*coord)?;
                let lut = luts.get(layer_index).and_then(|l| l.as_ref());
                let composited = composited_tile_payload(pixels, *coord, layer, lut, doc_width);
                let base: &[u8] = composited.as_deref().unwrap_or(pixels.as_slice());
                Some(tile_mip_chain(base))
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

        for ((_, _, key), payload) in uploads.iter().zip(payloads.iter()) {
            let key = *key;
            let Some(levels) = payload else {
                continue;
            };

            if let Some(existing) = self.tiles.get(&key) {
                self.atlas.write(&self.queue, existing.array_layer, levels);
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
                    // The atlas is at capacity with nothing free. Evict a prefetch-margin
                    // tile — one outside the viewport right now — to make room; if there is
                    // none left to sacrifice, every live tile is genuinely on screen at once
                    // and this upload is skipped for this frame rather than forcing growth
                    // past the ceiling.
                    let Some(victim) = evictable.pop() else {
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
            self.atlas.write(&self.queue, array_layer, levels);
            self.tiles.insert(key, GpuTile { array_layer });
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
        let live_layers: FxHashSet<&str> = doc.layers.iter().map(|l| l.id.as_str()).collect();
        self.layer_slots
            .retain(|id, _| live_layers.contains(id.as_str()));
        self.layer_xforms
            .retain(|id, _| live_layers.contains(id.as_str()));

        for (layer_index, coord, _) in uploads {
            if let Some(grid) = doc.layers.get_mut(layer_index).and_then(|l| l.tiles_mut()) {
                grid.clear_dirty_tile(DirtyChannel::Render, coord);
            }
        }
    }

    /// The whole layer stack as one ordered draw list, filling the instance buffers as it
    /// goes. A document layer's visible tiles become one range in `tiles` — one instanced
    /// draw call regardless of how many tiles that is. Within a vector layer, runs of the
    /// same kind coalesce into a single instanced draw while item order is preserved — so a
    /// shapes-only layer is one draw call, and a layer that alternates shapes and freehand
    /// still stacks in the order it was drawn in.
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
            if let Some(items) = layer.content.items() {
                let placement = vector_placement(layer);
                for item in items {
                    if !item_visible(item, placement, visible) {
                        continue;
                    }
                    match item {
                        VectorItem::Shape(shape) => {
                            let start = shapes.len() as u32;
                            shapes.push(shape_instance(shape, placement));
                            extend_run(&mut out, VectorRun::Shapes, start..shapes.len() as u32);
                        }
                        VectorItem::Path(path) => {
                            let start = strokes.len() as u32;
                            push_path_instances(path, placement, strokes);
                            extend_run(&mut out, VectorRun::Paths, start..strokes.len() as u32);
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
            let start = tiles.len() as u32;
            for coord in grid.coords() {
                if !TileGrid::tile_rect(coord).intersects(visible) {
                    continue;
                }
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

    pub fn render(&mut self, doc: &mut Document) {
        if !self.dirty && !doc.has_live_preview() {
            return;
        }

        self.write_layer_transforms(doc);
        self.sync_tiles(doc);

        let (dw, dh) = doc.camera.device_size();
        self.resize(dw, dh);

        let viewport = [
            (self.config.width as f32).max(1.0),
            (self.config.height as f32).max(1.0),
        ];

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

        let ink = doc.ink_rgba();
        let color = [
            ink[0] as f32 / 255.0,
            ink[1] as f32 / 255.0,
            ink[2] as f32 / 255.0,
            ink[3] as f32 / 255.0,
        ];
        let preview_shape = doc.preview_shape();
        let (p0, p1, tool, half_width, fill, shape_color) = match preview_shape {
            Some(s) => (
                [s.start.0, s.start.1],
                [s.end.0, s.end.1],
                s.tool as u32 as f32,
                s.half_width,
                if s.fill { 1.0 } else { 0.0 },
                color,
            ),
            None => match selection_rect_or_ellipse(doc) {
                Some((p0, p1, sel_tool)) => (
                    p0,
                    p1,
                    sel_tool as u32 as f32,
                    SELECTION_OUTLINE_WIDTH,
                    0.0,
                    SELECTION_OUTLINE_COLOR,
                ),
                None => ([0.0, 0.0], [0.0, 0.0], 0.0, 0.0, 0.0, color),
            },
        };
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
            _pad: 0.0,
        };
        self.queue
            .write_buffer(&self.preview_buf, 0, bytemuck::bytes_of(&preview));

        self.queue.write_buffer(
            &self.tile_camera_buf,
            0,
            bytemuck::bytes_of(&TileCamera {
                pan: [doc.camera.pan_x, doc.camera.pan_y],
                zoom: doc.camera.zoom,
                dpr: doc.camera.dpr,
                viewport,
                _pad: [0.0, 0.0],
            }),
        );

        let radius = doc.brush_size * 0.5;
        let stroke_color = if doc.tool == Tool::Eraser {
            ERASER_PREVIEW_COLOR
        } else {
            color
        };
        let mut tile_instances: Vec<TileInstance> = Vec::new();
        let mut instances: Vec<StrokeInstance> = Vec::new();
        let mut shape_instances: Vec<VectorShapeInstance> = Vec::new();
        let draws = self.build_layer_draws(
            doc,
            &mut tile_instances,
            &mut instances,
            &mut shape_instances,
        );

        let overlay_start = instances.len() as u32;
        if doc.text_editing() {
            instances.extend(text_overlay_instances(
                doc,
                self.started.elapsed().as_secs_f32(),
            ));
        } else if !doc.stroke_points.is_empty() {
            instances.extend(stroke_instances(&doc.stroke_points, radius, stroke_color));
        } else if let Some(handles) = doc.transform_handles() {
            instances.extend(transform_overlay_instances(handles));
            instances.extend(vector_selection_instances(doc));
        } else if let Some(points) = selection_lasso_points(doc) {
            instances.extend(stroke_instances(
                &points,
                SELECTION_OUTLINE_WIDTH,
                SELECTION_OUTLINE_COLOR,
            ));
        }
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
        let overlay_range = overlay_start..instances.len() as u32;

        if !tile_instances.is_empty() {
            self.ensure_tile_instance_capacity(tile_instances.len());
            self.queue.write_buffer(
                &self.tile_instance_buf,
                0,
                bytemuck::cast_slice(&tile_instances),
            );
        }
        if !instances.is_empty() {
            self.ensure_stroke_capacity(instances.len());
            self.queue
                .write_buffer(&self.stroke_buf, 0, bytemuck::cast_slice(&instances));
        }
        if !shape_instances.is_empty() {
            self.ensure_vector_shape_capacity(shape_instances.len());
            self.queue.write_buffer(
                &self.vector_shape_buf,
                0,
                bytemuck::cast_slice(&shape_instances),
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

            if let Some((x, y, w, h)) = doc.camera.paper_scissor(
                doc.width as f32,
                doc.height as f32,
                self.config.width,
                self.config.height,
            ) {
                pass.set_scissor_rect(x, y, w, h);

                for draw in &draws {
                    match draw {
                        LayerDraw::Tiles(mode, layer_id, range) => {
                            let Some((_, xform_bg)) = self.layer_xforms.get(layer_id) else {
                                continue;
                            };
                            // Group 0 (the atlas) and the vertex buffer are the same for every
                            // tile draw, but a vector draw in between overwrites both slots, so
                            // each tile run still has to rebind — there is no cross-run state
                            // to reuse once the layer stack interleaves tiles and vectors.
                            pass.set_pipeline(self.tile_pipeline(*mode));
                            pass.set_bind_group(0, self.atlas.bind_group(), &[]);
                            pass.set_bind_group(1, xform_bg, &[]);
                            pass.set_vertex_buffer(0, self.tile_instance_buf.slice(..));
                            pass.draw(0..6, range.clone());
                        }
                        LayerDraw::Vector(kind, range) => {
                            let (pipeline, buf) = match kind {
                                VectorRun::Shapes => {
                                    (&self.vector_shape_pipeline, &self.vector_shape_buf)
                                }
                                VectorRun::Paths => (&self.stroke_pipeline, &self.stroke_buf),
                            };
                            pass.set_pipeline(pipeline);
                            pass.set_bind_group(0, &self.preview_bg, &[]);
                            pass.set_vertex_buffer(0, buf.slice(..));
                            pass.draw(0..6, range.clone());
                        }
                    }
                }

                if !overlay_range.is_empty() {
                    pass.set_pipeline(&self.stroke_pipeline);
                    pass.set_bind_group(0, &self.preview_bg, &[]);
                    pass.set_vertex_buffer(0, self.stroke_buf.slice(..));
                    pass.draw(0..6, overlay_range.clone());
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
        self.dirty = doc.has_live_preview();
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

const PREMULTIPLIED_ALPHA_COMPONENT: wgpu::BlendComponent = wgpu::BlendComponent {
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

const STROKE_ATTRS: &[wgpu::VertexAttribute] = &[
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
        format: wgpu::VertexFormat::Float32,
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
        format: wgpu::VertexFormat::Float32,
    },
    wgpu::VertexAttribute {
        offset: 36,
        shader_location: 4,
        format: wgpu::VertexFormat::Float32,
    },
    wgpu::VertexAttribute {
        offset: 40,
        shader_location: 5,
        format: wgpu::VertexFormat::Float32,
    },
];
