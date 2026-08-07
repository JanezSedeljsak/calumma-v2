use bytemuck::{Pod, Zeroable};
use calumma_core::limits::{
    GPU_TILE_RETENTION_MARGIN_TILES, STROKE_INSTANCE_CAPACITY, SURFACE_FRAME_LATENCY,
};
use calumma_core::tile::{DirtyChannel, TileCoord, TileGrid, TILE_BYTES, TILE_SIZE};
use calumma_core::{Document, StrokePoint, Tool};
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

fn rgba_unit(rgba: [u8; 4]) -> [f32; 4] {
    [
        rgba[0] as f32 / 255.0,
        rgba[1] as f32 / 255.0,
        rgba[2] as f32 / 255.0,
        rgba[3] as f32 / 255.0,
    ]
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

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct StrokeInstance {
    segment: [f32; 4],
    color: [f32; 4],
    radius: f32,
    _pad: [f32; 3],
}

struct GpuTile {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    paper_pipeline: wgpu::RenderPipeline,
    tile_pipeline: wgpu::RenderPipeline,
    stroke_pipeline: wgpu::RenderPipeline,
    shape_pipeline: wgpu::RenderPipeline,
    paper_buf: wgpu::Buffer,
    paper_bg: wgpu::BindGroup,
    tile_bgl: wgpu::BindGroupLayout,
    tile_camera_buf: wgpu::Buffer,
    preview_buf: wgpu::Buffer,
    preview_bg: wgpu::BindGroup,
    stroke_buf: wgpu::Buffer,
    stroke_capacity: usize,
    sampler: wgpu::Sampler,
    tiles: HashMap<TileKey, GpuTile>,
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
        let tile_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("tile"),
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
                targets: &[Some(alpha_target(format))],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

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

        let stroke_capacity = STROKE_INSTANCE_CAPACITY;
        let stroke_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stroke-instances"),
            size: (stroke_capacity * std::mem::size_of::<StrokeInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            device,
            queue,
            surface,
            config,
            paper_pipeline,
            tile_pipeline,
            stroke_pipeline,
            shape_pipeline,
            paper_buf,
            paper_bg,
            tile_bgl,
            tile_camera_buf,
            preview_buf,
            preview_bg,
            stroke_buf,
            stroke_capacity,
            sampler,
            tiles: HashMap::new(),
            layer_slots: HashMap::new(),
            next_layer_slot: 0,
            started: Instant::now(),
            dirty: true,
        })
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

    pub fn cached_tile_count(&self) -> usize {
        self.tiles.len()
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

    fn sync_tiles(&mut self, doc: &mut Document) {
        let Some(visible) = doc.visible_rect() else {
            return;
        };
        let retained = visible.expanded_by_tiles(GPU_TILE_RETENTION_MARGIN_TILES);
        let doc_width = doc.width;

        let mut live: HashSet<TileKey> = HashSet::new();
        let mut uploads: Vec<(usize, TileCoord)> = Vec::new();
        let mut scratch = Vec::new();

        for layer_index in 0..doc.layers.len() {
            let layer = &doc.layers[layer_index];
            if !layer.visible {
                continue;
            }
            let Some(grid) = layer.tiles() else {
                continue;
            };
            let slot = self.layer_slot(&layer.id);
            let dirty = grid.dirty_tiles(DirtyChannel::Render);
            let mask = layer.mask();

            for (coord, pixels) in grid.iter() {
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
                uploads.push((layer_index, coord));

                if let Some(existing) = self.tiles.get(&key) {
                    let upload = masked_tile_bytes(pixels, coord, mask, doc_width, &mut scratch);
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
                let upload = masked_tile_bytes(pixels, coord, mask, doc_width, &mut scratch);
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
        }

        self.tiles.retain(|k, _| live.contains(k));
        let live_layers: HashSet<&str> = doc.layers.iter().map(|l| l.id.as_str()).collect();
        self.layer_slots
            .retain(|id, _| live_layers.contains(id.as_str()));

        for (layer_index, coord) in uploads {
            if let Some(grid) = doc.layers.get_mut(layer_index).and_then(|l| l.tiles_mut()) {
                grid.clear_dirty_tile(DirtyChannel::Render, coord);
            }
        }
    }

    fn visible_tile_keys(&mut self, doc: &Document) -> Vec<TileKey> {
        let Some(visible) = doc.visible_rect() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for layer in &doc.layers {
            if !layer.visible {
                continue;
            }
            let Some(grid) = layer.tiles() else {
                continue;
            };
            let Some(slot) = self.layer_slots.get(&layer.id).copied() else {
                continue;
            };
            for coord in grid.coords() {
                if TileGrid::tile_rect(coord).intersects(visible) {
                    out.push((slot, coord.x, coord.y));
                }
            }
        }
        out
    }

    pub fn render(&mut self, doc: &mut Document) {
        if !self.dirty && !doc.has_live_preview() {
            return;
        }

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
        let (p0, p1, tool, half_width, fill) = match doc.preview_shape {
            Some(s) => (
                [s.start.0, s.start.1],
                [s.end.0, s.end.1],
                s.tool as u32 as f32,
                s.half_width,
                if s.fill { 1.0 } else { 0.0 },
            ),
            None => ([0.0, 0.0], [0.0, 0.0], 0.0, 0.0, 0.0),
        };
        let preview = PreviewUniforms {
            pan: [doc.camera.pan_x, doc.camera.pan_y],
            zoom: doc.camera.zoom,
            dpr: doc.camera.dpr,
            viewport,
            _align_color: [0.0, 0.0],
            color,
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
        let instances = stroke_instances(&doc.stroke_points, radius, stroke_color);
        let stroke_count = instances.len();
        if stroke_count > 0 {
            self.ensure_stroke_capacity(stroke_count);
            self.queue
                .write_buffer(&self.stroke_buf, 0, bytemuck::cast_slice(&instances));
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

        for layer in &doc.layers {
            if !layer.visible {
                continue;
            }
            let Some(paths) = layer.content.paths() else {
                continue;
            };
            for path in paths {
                if !(path.fill && path.closed) || path.points.len() < 2 {
                    continue;
                }
                let mut min_x = f32::INFINITY;
                let mut min_y = f32::INFINITY;
                let mut max_x = f32::NEG_INFINITY;
                let mut max_y = f32::NEG_INFINITY;
                for &(x, y) in &path.points {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
                if !min_x.is_finite() {
                    continue;
                }
                let color = [
                    path.color[0] as f32 / 255.0,
                    path.color[1] as f32 / 255.0,
                    path.color[2] as f32 / 255.0,
                    path.color[3] as f32 / 255.0,
                ];
                let vector = PreviewUniforms {
                    pan: [doc.camera.pan_x, doc.camera.pan_y],
                    zoom: doc.camera.zoom,
                    dpr: doc.camera.dpr,
                    viewport,
                    _align_color: [0.0, 0.0],
                    color,
                    p0: [min_x, min_y],
                    p1: [max_x, max_y],
                    half_width: 0.0,
                    tool: Tool::Rect as u32 as f32,
                    fill: 1.0,
                    _pad: 0.0,
                };
                self.queue
                    .write_buffer(&self.preview_buf, 0, bytemuck::bytes_of(&vector));
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("vector"),
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
        }

        let draws = self.visible_tile_keys(doc);
        if !draws.is_empty() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("tiles"),
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
            pass.set_pipeline(&self.tile_pipeline);
            for key in draws {
                let Some(gpu) = self.tiles.get(&key) else {
                    continue;
                };
                pass.set_bind_group(0, &gpu.bind_group, &[]);
                pass.draw(0..6, 0..1);
            }
        }

        if stroke_count > 0 {
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
            pass.draw(0..6, 0..stroke_count as u32);
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

fn stroke_instances(points: &[StrokePoint], radius: f32, color: [f32; 4]) -> Vec<StrokeInstance> {
    if points.is_empty() {
        return Vec::new();
    }
    let instance = |a: &StrokePoint, b: &StrokePoint| StrokeInstance {
        segment: [a.x, a.y, b.x, b.y],
        color,
        radius,
        _pad: [0.0; 3],
    };
    if points.len() == 1 {
        return vec![instance(&points[0], &points[0])];
    }
    points.windows(2).map(|p| instance(&p[0], &p[1])).collect()
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

fn masked_tile_bytes<'a>(
    pixels: &'a [u8],
    coord: TileCoord,
    mask: Option<&[u8]>,
    doc_width: u32,
    scratch: &'a mut Vec<u8>,
) -> &'a [u8] {
    let Some(mask) = mask else {
        return pixels;
    };
    scratch.clear();
    scratch.extend_from_slice(pixels);
    if scratch.len() < TILE_BYTES {
        scratch.resize(TILE_BYTES, 0);
    }
    let (ox, oy) = coord.origin();
    for ty in 0..TILE_SIZE {
        for tx in 0..TILE_SIZE {
            let x = ox + tx as i32;
            let y = oy + ty as i32;
            if x < 0 || y < 0 {
                continue;
            }
            let mi = (y as u32)
                .saturating_mul(doc_width)
                .saturating_add(x as u32) as usize;
            if mi >= mask.len() {
                continue;
            }
            let i = ((ty * TILE_SIZE + tx) * 4) as usize;
            let a = scratch[i + 3] as u16 * mask[mi] as u16 / 255;
            scratch[i + 3] = a as u8;
        }
    }
    scratch.as_slice()
}

fn replace_target(format: wgpu::TextureFormat) -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format,
        blend: Some(wgpu::BlendState::REPLACE),
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
