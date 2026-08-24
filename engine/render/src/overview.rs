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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_gpu::{gpu, Gpu};

    fn pass(gpu: &Gpu) -> OverviewPass {
        OverviewPass::new(
            &gpu.device,
            &gpu.shader,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        )
    }

    fn doc(width: u32, height: u32) -> Document {
        Document::new("p".into(), "t", width, height)
    }

    /// Entering and leaving use different thresholds on purpose: a tile count sitting on one
    /// number would flip the whole board between two rendering paths every frame.
    #[test]
    fn the_overview_switches_on_and_off_with_hysteresis() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);

        assert!(!overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD - 1, false));
        assert!(overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false));
        assert!(
            overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD - 1, false),
            "once on, it stays on well below the entry threshold"
        );
        assert!(overview.should_use(OVERVIEW_EXIT_TILE_THRESHOLD + 1, false));
        assert!(!overview.should_use(OVERVIEW_EXIT_TILE_THRESHOLD, false));
    }

    #[test]
    fn live_editing_takes_the_overview_off_whatever_the_tile_count() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);
        assert!(overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false));

        assert!(!overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD * 10, true));
        assert!(
            !overview.should_use(OVERVIEW_EXIT_TILE_THRESHOLD + 1, false),
            "coming back off a stroke re-enters through the entry threshold, not the exit one"
        );
    }

    #[test]
    fn an_inactive_pass_never_composites_the_document() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);

        overview.sync(&doc(512, 512), &gpu.device, &gpu.queue);

        assert!(overview.texture.is_none(), "no upload while inactive");
        assert!(overview.dirty);
    }

    #[test]
    fn an_active_pass_uploads_once_and_then_leaves_the_texture_alone() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);
        let doc = doc(512, 256);
        overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);

        overview.sync(&doc, &gpu.device, &gpu.queue);
        assert!(overview.texture.is_some());
        assert!(!overview.dirty);
        assert_eq!((overview.doc_width, overview.doc_height), (512, 256));
        assert_eq!((overview.tex_width, overview.tex_height), (512, 256));

        let (w, h) = (overview.tex_width, overview.tex_height);
        overview.sync(&doc, &gpu.device, &gpu.queue);
        assert_eq!((overview.tex_width, overview.tex_height), (w, h));
    }

    /// A resize changes what the overview *is*, so it re-uploads even though nothing marked it
    /// dirty — the document dimensions are part of the cache key, not just a payload.
    #[test]
    fn a_document_resize_re_uploads_without_anything_marking_it_dirty() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);
        overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
        overview.sync(&doc(512, 256), &gpu.device, &gpu.queue);

        overview.sync(&doc(256, 512), &gpu.device, &gpu.queue);

        assert_eq!((overview.doc_width, overview.doc_height), (256, 512));
        assert_eq!((overview.tex_width, overview.tex_height), (256, 512));
    }

    #[test]
    fn marking_dirty_makes_the_next_sync_upload_again() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);
        let doc = doc(128, 128);
        overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
        overview.sync(&doc, &gpu.device, &gpu.queue);

        overview.mark_dirty();
        assert!(overview.dirty);
        overview.sync(&doc, &gpu.device, &gpu.queue);
        assert!(!overview.dirty);
    }

    /// Prewarm is the one path that uploads while the pass is *inactive*: it pays the
    /// composite before the zoom-out that needs it, so the first overview frame is not the
    /// slow one.
    #[test]
    fn prewarm_uploads_while_inactive_and_only_once_per_request() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);
        let doc = doc(128, 128);

        overview.prewarm(&doc, &gpu.device, &gpu.queue);
        assert!(overview.texture.is_none(), "nothing was requested yet");

        overview.request_prewarm();
        overview.prewarm(&doc, &gpu.device, &gpu.queue);
        assert!(overview.texture.is_some());
        assert!(!overview.prewarm_pending);
        assert!(!overview.active, "prewarming does not turn the overview on");

        overview.mark_dirty();
        overview.prewarm(&doc, &gpu.device, &gpu.queue);
        assert!(overview.dirty, "a spent request does not upload again");
    }

    #[test]
    fn clearing_drops_the_texture_and_the_pass_falls_back_to_drawing_nothing() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);
        overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
        overview.sync(&doc(128, 128), &gpu.device, &gpu.queue);

        overview.clear();

        assert!(overview.texture.is_none());
        assert!(overview.bind_group.is_none());
        assert!(!overview.active);
        assert!(overview.dirty);
        assert_eq!((overview.doc_width, overview.doc_height), (0, 0));
    }

    /// `_pad` exists to keep this struct the exact size of `board.wgsl`'s `OverviewCamera`
    /// (ten floats: pan, zoom, dpr, viewport, doc_size, pad). A field added on either side
    /// without the other reads the uniform off by however many bytes it grew.
    #[test]
    fn the_camera_uniform_stays_the_size_the_shader_declares() {
        let Some(gpu) = gpu() else { return };
        let overview = pass(gpu);
        let mut doc = doc(128, 128);
        doc.camera.pan_x = 12.0;

        overview.write_camera(&gpu.queue, &doc, [800.0, 600.0]);

        assert_eq!(std::mem::size_of::<OverviewCamera>(), 10 * 4);
    }
}
