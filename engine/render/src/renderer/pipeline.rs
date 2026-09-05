use super::*;

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
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
            ..Default::default()
        }))
        .ok()?;
        let adapter_array_layers = adapter.limits().max_texture_array_layers;
        let atlas_max_capacity = adapter_array_layers.min(TILE_ATLAS_MAX_CAPACITY);
        let budget = GpuBudget::new(DeviceTier::classify(
            gpu_kind(adapter.get_info().device_type),
            adapter_array_layers,
        ));
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("calumma-render-headless"),
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
        .ok()?;
        let format = wgpu::TextureFormat::Bgra8UnormSrgb;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
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
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/board.wgsl").into()),
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

        Self {
            device,
            queue,
            output,
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
        }
    }

    pub(super) fn tile_pipeline(&self, mode: BlendMode) -> &wgpu::RenderPipeline {
        match mode {
            BlendMode::Normal => &self.tile_pipeline_normal,
            BlendMode::Multiply => &self.tile_pipeline_multiply,
            BlendMode::Screen => &self.tile_pipeline_screen,
        }
    }

    pub(super) fn solid_pipeline(&self, mode: BlendMode) -> &wgpu::RenderPipeline {
        match mode {
            BlendMode::Normal => &self.solid_pipeline_normal,
            BlendMode::Multiply => &self.solid_pipeline_multiply,
            BlendMode::Screen => &self.solid_pipeline_screen,
        }
    }
}

/// samplers, and the layer table. Bound once for the whole board — there is no per-layer group,
/// which is the point of the table.
///
/// A function rather than an inline descriptor so the GPU tests below build their pipelines
/// against the *same* layout the app does; a shader/layout disagreement then fails a test
/// instead of only the running app.
pub(crate) fn tile_shared_bgl(device: &wgpu::Device) -> wgpu::BindGroupLayout {
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
pub(super) fn gpu_kind(device_type: wgpu::DeviceType) -> GpuKind {
    match device_type {
        wgpu::DeviceType::DiscreteGpu => GpuKind::Discrete,
        wgpu::DeviceType::IntegratedGpu | wgpu::DeviceType::VirtualGpu => GpuKind::Integrated,
        wgpu::DeviceType::Cpu => GpuKind::Software,
        wgpu::DeviceType::Other => GpuKind::Other,
    }
}

pub(super) fn uniform_entry(binding: u32, size: usize) -> wgpu::BindGroupLayoutEntry {
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

pub(super) fn replace_target(format: wgpu::TextureFormat) -> wgpu::ColorTargetState {
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

pub(crate) fn premultiplied_target(format: wgpu::TextureFormat) -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format,
        blend: Some(wgpu::BlendState {
            color: PREMULTIPLIED_ALPHA_COMPONENT,
            alpha: PREMULTIPLIED_ALPHA_COMPONENT,
        }),
        write_mask: wgpu::ColorWrites::ALL,
    }
}

pub(super) fn multiply_target(format: wgpu::TextureFormat) -> wgpu::ColorTargetState {
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

pub(super) fn screen_target(format: wgpu::TextureFormat) -> wgpu::ColorTargetState {
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

pub(super) fn alpha_target(format: wgpu::TextureFormat) -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format,
        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
        write_mask: wgpu::ColorWrites::ALL,
    }
}

pub(crate) const TILE_INSTANCE_ATTRS: &[wgpu::VertexAttribute] = &[
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

pub(super) const GUIDE_ATTRS: &[wgpu::VertexAttribute] = &[
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

pub(super) const VECTOR_SHAPE_ATTRS: &[wgpu::VertexAttribute] = &[
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

    fn blend(target: wgpu::ColorTargetState) -> wgpu::BlendState {
        target.blend.expect("every board target blends")
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
}
