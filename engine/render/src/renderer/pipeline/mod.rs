use super::*;

mod builders;
mod layouts;
mod targets;

pub(crate) use layouts::{
    backdrop_bgl, paper_bind_group, tile_shared_bgl, STROKE_ATTRS, TILE_INSTANCE_ATTRS,
};
pub(crate) use targets::{premultiplied_target, PREMULTIPLIED_ALPHA_COMPONENT};

use builders::{paper_pipeline, preview_pipelines, tile_pipelines, vector_fill_pipeline};
pub(super) use layouts::{
    fill_bind_group, fill_edge_buffer, gpu_kind, uniform_entry, vector_fill_buffer, GUIDE_ATTRS,
    VECTOR_FILL_ATTRS, VECTOR_SHAPE_ATTRS,
};
pub(super) use targets::{alpha_target, multiply_target, replace_target, screen_target};

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
        let (device, queue, budget, atlas_max_capacity) =
            Self::request_device(&adapter, "calumma-render").map_err(|e| e.to_string())?;

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
        let output = FrameOutput::Surface(surface);

        Ok(Self::assemble(
            device,
            queue,
            format,
            config,
            output,
            budget,
            atlas_max_capacity,
        ))
    }

    #[cfg(test)]
    pub fn new_headless(width: u32, height: u32) -> Option<Self> {
        let instance = crate::test_gpu::headless_instance();
        let adapter = crate::test_gpu::request_test_adapter(&instance)?;
        let (device, queue, budget, atlas_max_capacity) =
            Self::request_device(&adapter, "calumma-render-headless").ok()?;
        let format = wgpu::TextureFormat::Bgra8UnormSrgb;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            format,
            color_space: wgpu::SurfaceColorSpace::default(),
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: SURFACE_FRAME_LATENCY,
        };
        let output = FrameOutput::Headless(FrameOutput::headless_texture(&device, &config));
        Some(Self::assemble(
            device,
            queue,
            format,
            config,
            output,
            budget,
            atlas_max_capacity,
        ))
    }

    fn request_device(
        adapter: &wgpu::Adapter,
        label: &'static str,
    ) -> Result<(wgpu::Device, wgpu::Queue, GpuBudget, u32), wgpu::RequestDeviceError> {
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
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some(label),
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
            }))?;
        Ok((device, queue, budget, atlas_max_capacity))
    }

    pub(super) fn assemble(
        device: wgpu::Device,
        queue: wgpu::Queue,
        format: wgpu::TextureFormat,
        config: wgpu::SurfaceConfiguration,
        output: FrameOutput,
        budget: GpuBudget,
        atlas_max_capacity: u32,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("board"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/board.wgsl").into()),
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
        let paper_pipeline = paper_pipeline(&device, &shader, &paper_bgl, format);

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
        let backdrop_bgl = backdrop_bgl(&device);
        let tiles = tile_pipelines(&device, &shader, &tile_shared_bgl, &backdrop_bgl, format);

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
        let previews = preview_pipelines(&device, &shader, &preview_bgl, format);

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

        let fill_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vector-fill-bgl"),
            entries: &[
                uniform_entry(0, std::mem::size_of::<PreviewUniforms>()),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let vector_fill_pipeline = vector_fill_pipeline(&device, &shader, &fill_bgl, format);
        let vector_fill_capacity = crate::vector_draw::FILL_INSTANCE_CAPACITY;
        let vector_fill_buf = vector_fill_buffer(&device, vector_fill_capacity);
        let fill_edge_capacity = crate::vector_draw::FILL_EDGE_CAPACITY;
        let fill_edge_buf = fill_edge_buffer(&device, fill_edge_capacity);
        let fill_bg = fill_bind_group(&device, &fill_bgl, &preview_buf, &fill_edge_buf);

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

        Self {
            device,
            queue,
            output,
            config,
            paper_pipeline,
            tile_pipeline_normal: tiles.normal,
            tile_pipeline_multiply: tiles.multiply,
            tile_pipeline_screen: tiles.screen,
            solid_pipeline_normal: tiles.solid_normal,
            solid_pipeline_multiply: tiles.solid_multiply,
            solid_pipeline_screen: tiles.solid_screen,
            tile_blend_pipeline: tiles.tile_blend,
            solid_blend_pipeline: tiles.solid_blend,
            backdrop_bgl,
            backdrop: None,
            stroke_pipeline: previews.stroke,
            overlay_pipeline: previews.overlay,
            guide_pipeline: previews.guide,
            stroke_coverage,
            shape_pipeline: previews.shape,
            vector_shape_pipeline: previews.vector_shape,
            vector_fill_pipeline,
            fill_bgl,
            fill_bg,
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
            vector_fill_buf,
            vector_fill_capacity,
            fill_edge_buf,
            fill_edge_capacity,
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
            last_overlay_range: 0..0,
            screen_overlay_start: 0,
            cached_shapes: Vec::new(),
            cached_fills: Vec::new(),
            cached_fill_edges: Vec::new(),
            cached_draws: Vec::new(),
            overview,
            camera_motion: false,
            motion_idle_frames: 0,
            cached_tile_draw_count: None,
            base_only_tiles: FxHashSet::default(),
            pan_cache,
            coverage_progress: None,
            drawn_caret_phase: None,
            layer_transform_stamp: Vec::new(),
        }
    }

    pub(super) fn tile_pipeline(&self, mode: BlendMode) -> &wgpu::RenderPipeline {
        match mode {
            BlendMode::Multiply => &self.tile_pipeline_multiply,
            BlendMode::Screen => &self.tile_pipeline_screen,
            _ => &self.tile_pipeline_normal,
        }
    }

    pub(super) fn solid_pipeline(&self, mode: BlendMode) -> &wgpu::RenderPipeline {
        match mode {
            BlendMode::Multiply => &self.solid_pipeline_multiply,
            BlendMode::Screen => &self.solid_pipeline_screen,
            _ => &self.solid_pipeline_normal,
        }
    }
}

#[cfg(test)]
mod tests;
