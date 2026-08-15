use crate::compose::{
    composited_tile_payload, rgba_unit, selection_lasso_points, selection_rect_or_ellipse,
    stroke_instances, text_overlay_instances, transform_overlay_instances, StrokeInstance,
};
use crate::vector_draw::{
    item_visible, push_path_instances, shape_instance, vector_placement,
    vector_selection_instances, VectorShapeInstance,
};
use bytemuck::{Pod, Zeroable};
use calumma_core::filters::AdjustmentLut;
use calumma_core::limits::{
    GPU_TILE_RETENTION_MARGIN_TILES, STROKE_INSTANCE_CAPACITY, SURFACE_FRAME_LATENCY,
    VECTOR_SHAPE_INSTANCE_CAPACITY,
};
use calumma_core::tile::{DirtyChannel, TileCoord, TileGrid, TILE_BYTES, TILE_SIZE};
use calumma_core::{BlendMode, Document, Tool, VectorItem};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
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
    time: f32,
    dark: f32,
    _align_hover: [f32; 2],
    hover_rect: [f32; 4],
    desk: [f32; 4],
    grid: [f32; 4],
    paper_border: [f32; 4],
    hover_enabled: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
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

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TilePlacement {
    origin: [f32; 2],
    tile_size: f32,
    _pad: f32,
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

struct GpuTile {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
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
    Tiles(BlendMode, Vec<TileKey>),
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
    tile_bgl: wgpu::BindGroupLayout,
    tile_camera_buf: wgpu::Buffer,
    preview_buf: wgpu::Buffer,
    preview_bg: wgpu::BindGroup,
    stroke_buf: wgpu::Buffer,
    stroke_capacity: usize,
    vector_shape_buf: wgpu::Buffer,
    vector_shape_capacity: usize,
    sampler: wgpu::Sampler,
    tiles: HashMap<TileKey, GpuTile>,
    layer_transform_bufs: HashMap<String, wgpu::Buffer>,
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

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("calumma-render"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
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

        let tile_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tile-bgl"),
            entries: &[
                uniform_entry(0, std::mem::size_of::<TileCamera>()),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                uniform_entry(3, std::mem::size_of::<TilePlacement>()),
                uniform_entry(4, std::mem::size_of::<LayerXform>()),
            ],
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
            ..Default::default()
        });
        let tile_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tile-pl"),
            bind_group_layouts: &[Some(&tile_bgl)],
            ..Default::default()
        });
        let tile_pipeline_for = |label: &str, target: wgpu::ColorTargetState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&tile_pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_tile"),
                    compilation_options: Default::default(),
                    buffers: &[],
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
            tile_bgl,
            tile_camera_buf,
            preview_buf,
            preview_bg,
            stroke_buf,
            stroke_capacity,
            vector_shape_buf,
            vector_shape_capacity,
            sampler,
            tiles: HashMap::new(),
            layer_transform_bufs: HashMap::new(),
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

    fn ensure_layer_transform_buf(&mut self, layer_id: &str) {
        if self.layer_transform_bufs.contains_key(layer_id) {
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
        self.layer_transform_bufs.insert(layer_id.to_string(), buf);
    }

    /// Every layer `sync_tiles` uploads needs its uniform written, text included — the
    /// buffer is created zeroed, and a zeroed `LayerXform` scales the tile to nothing.
    fn write_layer_transforms(&mut self, doc: &Document) {
        for layer in &doc.layers {
            if layer.tiles().is_none() {
                continue;
            }
            self.ensure_layer_transform_buf(&layer.id);
            let Some(buf) = self.layer_transform_bufs.get(&layer.id) else {
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

    /// GPU-side bytes held for the open document: one RGBA texture per cached tile.
    pub fn gpu_tile_bytes(&self) -> usize {
        self.tiles.len() * TILE_BYTES
    }

    /// Hand back everything that belonged to the document being closed — tile textures and
    /// the per-layer uniform buffers keyed by its layer ids. Eviction otherwise only happens
    /// inside `sync_tiles`, which needs a document to run, so a closed project's textures
    /// would sit in VRAM until some *other* project was opened and drawn.
    pub fn release_document(&mut self) {
        self.tiles.clear();
        self.layer_transform_bufs.clear();
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

    fn sync_tiles(&mut self, doc: &mut Document) {
        let Some(visible) = doc.visible_rect() else {
            return;
        };
        let retained = visible.expanded_by_tiles(GPU_TILE_RETENTION_MARGIN_TILES);
        let doc_width = doc.width;

        let mut live: HashSet<TileKey> = HashSet::new();
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
            self.ensure_layer_transform_buf(&layer.id);
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
        let luts: Vec<Option<AdjustmentLut>> = doc
            .layers
            .iter()
            .map(|l| l.adjustments.map(|a| a.lut()))
            .collect();
        let payloads: Vec<Option<Vec<u8>>> = uploads
            .par_iter()
            .map(|(layer_index, coord, _)| {
                let layer = doc.layers.get(*layer_index)?;
                let pixels = layer.tiles()?.get(*coord)?;
                composited_tile_payload(
                    pixels,
                    *coord,
                    layer,
                    luts[*layer_index].as_ref(),
                    doc_width,
                )
            })
            .collect();

        for ((layer_index, coord, key), payload) in uploads.iter().zip(payloads.iter()) {
            let (layer_index, coord, key) = (*layer_index, *coord, *key);
            let Some(layer) = doc.layers.get(layer_index) else {
                continue;
            };
            let Some(pixels) = layer.tiles().and_then(|g| g.get(coord)) else {
                continue;
            };
            let upload: &[u8] = payload.as_deref().unwrap_or(pixels.as_slice());

            if let Some(existing) = self.tiles.get(&key) {
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &existing.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    upload,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(TILE_SIZE * 4),
                        rows_per_image: Some(TILE_SIZE),
                    },
                    wgpu::Extent3d {
                        width: TILE_SIZE,
                        height: TILE_SIZE,
                        depth_or_array_layers: 1,
                    },
                );
                continue;
            }
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("tile"),
                size: wgpu::Extent3d {
                    width: TILE_SIZE,
                    height: TILE_SIZE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                upload,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(TILE_SIZE * 4),
                    rows_per_image: Some(TILE_SIZE),
                },
                wgpu::Extent3d {
                    width: TILE_SIZE,
                    height: TILE_SIZE,
                    depth_or_array_layers: 1,
                },
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let (ox, oy) = coord.origin();
            let placement_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("tile-placement"),
                size: std::mem::size_of::<TilePlacement>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue.write_buffer(
                &placement_buf,
                0,
                bytemuck::bytes_of(&TilePlacement {
                    origin: [ox as f32, oy as f32],
                    tile_size: TILE_SIZE as f32,
                    _pad: 0.0,
                }),
            );
            let xform_buf = self
                .layer_transform_bufs
                .get(&layer.id)
                .expect("layer transform buffer ensured by caller");
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("tile-bg"),
                layout: &self.tile_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.tile_camera_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: placement_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: xform_buf.as_entire_binding(),
                    },
                ],
            });
            self.tiles.insert(
                key,
                GpuTile {
                    texture,
                    bind_group,
                },
            );
        }

        self.tiles.retain(|k, _| live.contains(k));
        let live_layers: HashSet<&str> = doc.layers.iter().map(|l| l.id.as_str()).collect();
        self.layer_slots
            .retain(|id, _| live_layers.contains(id.as_str()));
        self.layer_transform_bufs
            .retain(|id, _| live_layers.contains(id.as_str()));

        for (layer_index, coord, _) in uploads {
            if let Some(grid) = doc.layers.get_mut(layer_index).and_then(|l| l.tiles_mut()) {
                grid.clear_dirty_tile(DirtyChannel::Render, coord);
            }
        }
    }

    /// The whole layer stack as one ordered draw list, filling the two instance buffers as it
    /// goes. Within a vector layer, runs of the same kind coalesce into a single instanced
    /// draw while item order is preserved — so a shapes-only layer is one draw call, and a
    /// layer that alternates shapes and freehand still stacks in the order it was drawn in.
    fn build_layer_draws(
        &mut self,
        doc: &Document,
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
            let mut keys = Vec::new();
            for coord in grid.coords() {
                if TileGrid::tile_rect(coord).intersects(visible) {
                    keys.push((slot, coord.x, coord.y));
                }
            }
            if !keys.is_empty() {
                out.push(LayerDraw::Tiles(layer.blend_mode, keys));
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

        let hover = doc
            .hover_layer
            .and_then(|i| doc.layers.get(i))
            .and_then(|l| l.content_bounds())
            .map(|(x0, y0, x1, y1)| [x0, y0, x1, y1])
            .unwrap_or([0.0; 4]);

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
            time: self.started.elapsed().as_secs_f32(),
            dark: if doc.dark_theme { 1.0 } else { 0.0 },
            _align_hover: [0.0, 0.0],
            hover_rect: hover,
            desk: rgba_unit(doc.board_colors.desk),
            grid: rgba_unit(doc.board_colors.grid),
            paper_border: rgba_unit(doc.board_colors.paper_border),
            hover_enabled: if doc.hover_layer.is_some() && hover != [0.0; 4] {
                1.0
            } else {
                0.0
            },
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
        };
        self.queue
            .write_buffer(&self.paper_buf, 0, bytemuck::bytes_of(&paper));

        let color = [
            doc.color[0] as f32 / 255.0,
            doc.color[1] as f32 / 255.0,
            doc.color[2] as f32 / 255.0,
            doc.color[3] as f32 / 255.0,
        ];
        let (p0, p1, tool, half_width, fill, shape_color) = match doc.preview_shape {
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
        let mut instances: Vec<StrokeInstance> = Vec::new();
        let mut shape_instances: Vec<VectorShapeInstance> = Vec::new();
        let draws = self.build_layer_draws(doc, &mut instances, &mut shape_instances);

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
        let overlay_range = overlay_start..instances.len() as u32;

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

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("paper"),
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
        }

        if !draws.is_empty() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("layers"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
            for draw in &draws {
                match draw {
                    LayerDraw::Tiles(mode, keys) => {
                        pass.set_pipeline(self.tile_pipeline(*mode));
                        for key in keys {
                            let Some(gpu) = self.tiles.get(key) else {
                                continue;
                            };
                            pass.set_bind_group(0, &gpu.bind_group, &[]);
                            pass.draw(0..6, 0..1);
                        }
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
        }

        if !overlay_range.is_empty() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("stroke-preview"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
            pass.set_pipeline(&self.stroke_pipeline);
            pass.set_bind_group(0, &self.preview_bg, &[]);
            pass.set_vertex_buffer(0, self.stroke_buf.slice(..));
            pass.draw(0..6, overlay_range.clone());
        }

        if doc.preview_shape.is_some() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shape-preview"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
            pass.set_pipeline(&self.shape_pipeline);
            pass.set_bind_group(0, &self.preview_bg, &[]);
            pass.draw(0..3, 0..1);
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
