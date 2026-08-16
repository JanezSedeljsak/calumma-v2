use bytemuck::{Pod, Zeroable};
use calumma_core::limits::{
    OVERVIEW_ENTER_TILE_THRESHOLD, OVERVIEW_EXIT_TILE_THRESHOLD, OVERVIEW_MAX_SIDE,
};
use calumma_core::Document;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct OverviewCamera {
    pub pan: [f32; 2],
    pub zoom: f32,
    pub dpr: f32,
    pub viewport: [f32; 2],
    pub doc_size: [f32; 2],
    pub _pad: [f32; 2],
}

pub struct OverviewPass {
    bgl: wgpu::BindGroupLayout,
    camera_buf: wgpu::Buffer,
    sampler: wgpu::Sampler,
    pipeline: wgpu::RenderPipeline,
    texture: Option<wgpu::Texture>,
    bind_group: Option<wgpu::BindGroup>,
    tex_width: u32,
    tex_height: u32,
    doc_width: u32,
    doc_height: u32,
    dirty: bool,
    active: bool,
    prewarm_pending: bool,
}

impl OverviewPass {
    pub fn new(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        format: wgpu::TextureFormat,
    ) -> Self {
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("overview-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
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
            ],
        });
        let camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("overview-camera"),
            size: std::mem::size_of::<OverviewCamera>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("overview-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("overview-pl"),
            bind_group_layouts: &[Some(&bgl)],
            ..Default::default()
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("overview"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_overview"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_overview"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            bgl,
            camera_buf,
            sampler,
            pipeline,
            texture: None,
            bind_group: None,
            tex_width: 0,
            tex_height: 0,
            doc_width: 0,
            doc_height: 0,
            dirty: true,
            active: false,
            prewarm_pending: false,
        }
    }

    pub fn request_prewarm(&mut self) {
        self.prewarm_pending = true;
        self.dirty = true;
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn clear(&mut self) {
        self.texture = None;
        self.bind_group = None;
        self.tex_width = 0;
        self.tex_height = 0;
        self.doc_width = 0;
        self.doc_height = 0;
        self.dirty = true;
        self.active = false;
        self.prewarm_pending = false;
    }

    pub fn should_use(&mut self, tile_draw_count: usize, live_editing: bool) -> bool {
        if live_editing {
            self.active = false;
            return false;
        }
        if self.active {
            self.active = tile_draw_count > OVERVIEW_EXIT_TILE_THRESHOLD;
        } else {
            self.active = tile_draw_count >= OVERVIEW_ENTER_TILE_THRESHOLD;
        }
        self.active
    }

    pub fn prewarm(&mut self, doc: &Document, device: &wgpu::Device, queue: &wgpu::Queue) {
        if !self.prewarm_pending {
            return;
        }
        self.upload(doc, device, queue);
        self.prewarm_pending = false;
    }

    pub fn sync(&mut self, doc: &Document, device: &wgpu::Device, queue: &wgpu::Queue) {
        if !self.active {
            return;
        }
        let dw = doc.width;
        let dh = doc.height;
        if !self.dirty && self.doc_width == dw && self.doc_height == dh {
            return;
        }
        self.upload(doc, device, queue);
    }

    fn upload(&mut self, doc: &Document, device: &wgpu::Device, queue: &wgpu::Queue) {
        let dw = doc.width;
        let dh = doc.height;
        let (tw, th, rgba) = doc.composite_overview(OVERVIEW_MAX_SIDE);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("overview"),
            size: wgpu::Extent3d {
                width: tw.max(1),
                height: th.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(tw * 4),
                rows_per_image: Some(th),
            },
            wgpu::Extent3d {
                width: tw,
                height: th,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("overview-bg"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.camera_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.texture = Some(texture);
        self.bind_group = Some(bind_group);
        self.tex_width = tw;
        self.tex_height = th;
        self.doc_width = dw;
        self.doc_height = dh;
        self.dirty = false;
    }

    pub fn write_camera(&self, queue: &wgpu::Queue, doc: &Document, viewport: [f32; 2]) {
        let camera = OverviewCamera {
            pan: [doc.camera.pan_x, doc.camera.pan_y],
            zoom: doc.camera.zoom,
            dpr: doc.camera.dpr,
            viewport,
            doc_size: [doc.width as f32, doc.height as f32],
            _pad: [0.0, 0.0],
        };
        queue.write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(&camera));
    }

    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        let Some(bg) = self.bind_group.as_ref() else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bg, &[]);
        pass.draw(0..6, 0..1);
    }
}
