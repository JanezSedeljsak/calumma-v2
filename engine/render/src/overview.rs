use crate::overview_lod::{
    chunk_tex_rect, needed_side, overview_chunks, pick_level, pyramid_sides, stack_stamp,
};
use bytemuck::{Pod, Zeroable};
use calumma_core::limits::{
    OVERVIEW_ENTER_TILE_THRESHOLD, OVERVIEW_EXIT_TILE_THRESHOLD, OVERVIEW_LEVELS,
};
use calumma_core::tile::DirtyChannel;
use calumma_core::{Document, GpuBudget, MemoryPressureLevel};
use rustc_hash::FxHashSet;

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

struct Level {
    max_side: u32,
    tex_width: u32,
    tex_height: u32,
    texture: Option<wgpu::Texture>,
    bind_group: Option<wgpu::BindGroup>,
    full_dirty: bool,
    dirty_chunks: FxHashSet<(i32, i32)>,
}

pub struct OverviewPass {
    bgl: wgpu::BindGroupLayout,
    camera_buf: wgpu::Buffer,
    sampler: wgpu::Sampler,
    pipeline: wgpu::RenderPipeline,
    levels: Vec<Level>,
    displayed: usize,
    tex_width: u32,
    tex_height: u32,
    doc_width: u32,
    doc_height: u32,
    dirty: bool,
    active: bool,
    prewarm_pending: bool,
    stamp: u64,
    allocations: u32,
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
            levels: Vec::new(),
            displayed: 0,
            tex_width: 0,
            tex_height: 0,
            doc_width: 0,
            doc_height: 0,
            dirty: true,
            active: false,
            prewarm_pending: false,
            stamp: 0,
            allocations: 0,
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
        self.levels.clear();
        self.displayed = 0;
        self.tex_width = 0;
        self.tex_height = 0;
        self.doc_width = 0;
        self.doc_height = 0;
        self.dirty = true;
        self.active = false;
        self.prewarm_pending = false;
        self.stamp = 0;
    }

    /// `busiest_layer_tiles` is the *busiest single layer's* visible tile count
    /// (`Renderer::busiest_layer_tile_count`), not a sum across the stack — a document with
    /// many sparse layers should not be charged as if it were one layer painted edge to edge,
    /// and previously could never leave the overview once enough layers pushed the summed
    /// count past the exit threshold even though no individual layer needed it.
    pub fn should_use(&mut self, busiest_layer_tiles: usize, live_editing: bool) -> bool {
        if live_editing {
            self.active = false;
            return false;
        }
        if self.active {
            self.active = busiest_layer_tiles > OVERVIEW_EXIT_TILE_THRESHOLD;
        } else {
            self.active = busiest_layer_tiles >= OVERVIEW_ENTER_TILE_THRESHOLD;
        }
        self.active
    }

    pub fn prewarm(
        &mut self,
        doc: &mut Document,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        budget: &GpuBudget,
    ) {
        if !self.prewarm_pending {
            return;
        }
        self.refresh(doc, device, queue, budget);
        self.prewarm_pending = false;
    }

    pub fn sync(
        &mut self,
        doc: &mut Document,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        budget: &GpuBudget,
    ) {
        if !self.active {
            return;
        }
        self.refresh(doc, device, queue, budget);
    }

    fn refresh(
        &mut self,
        doc: &mut Document,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        budget: &GpuBudget,
    ) {
        self.ensure_pyramid(doc, budget);
        if self.levels.is_empty() {
            return;
        }
        let stamp = stack_stamp(doc);
        if stamp != self.stamp {
            for level in &mut self.levels {
                level.full_dirty = true;
                level.dirty_chunks.clear();
            }
            self.stamp = stamp;
        }
        let chunks = overview_chunks(doc);
        if !chunks.is_empty() {
            for level in &mut self.levels {
                if !level.full_dirty {
                    level.dirty_chunks.extend(&chunks);
                }
            }
        }
        let mut sides = [0u32; OVERVIEW_LEVELS];
        let n = self.levels.len().min(OVERVIEW_LEVELS);
        for (i, level) in self.levels.iter().take(n).enumerate() {
            sides[i] = level.max_side;
        }
        let index = pick_level(&sides[..n], needed_side(doc));
        self.ensure_level(index, doc, device, queue);
        if budget.pressure() >= MemoryPressureLevel::Warn {
            for (i, level) in self.levels.iter_mut().enumerate() {
                if i != index {
                    level.texture = None;
                    level.bind_group = None;
                    level.full_dirty = true;
                    level.dirty_chunks.clear();
                }
            }
        }
        self.displayed = index;
        if let Some(level) = self.levels.get(index) {
            self.tex_width = level.tex_width;
            self.tex_height = level.tex_height;
        }
        self.doc_width = doc.width;
        self.doc_height = doc.height;
        self.dirty = false;
        doc.clear_layer_dirty(DirtyChannel::Overview);
    }

    fn ensure_pyramid(&mut self, doc: &Document, budget: &GpuBudget) {
        let dw = doc.width;
        let dh = doc.height;
        let sides = pyramid_sides(dw, dh, budget.overview_finest_side());
        let same = self.levels.len() == sides.len()
            && self.levels.iter().zip(&sides).all(|(level, &side)| {
                level.max_side == side && self.doc_width == dw && self.doc_height == dh
            });
        if same {
            return;
        }
        self.levels = sides
            .into_iter()
            .map(|max_side| {
                let (tex_width, tex_height) = Document::overview_dimensions(dw, dh, max_side);
                Level {
                    max_side,
                    tex_width,
                    tex_height,
                    texture: None,
                    bind_group: None,
                    full_dirty: true,
                    dirty_chunks: FxHashSet::default(),
                }
            })
            .collect();
        self.displayed = 0;
        self.stamp = 0;
    }

    fn ensure_level(
        &mut self,
        index: usize,
        doc: &Document,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let Some(level) = self.levels.get(index) else {
            return;
        };
        let max_side = level.max_side;
        let tw = level.tex_width.max(1);
        let th = level.tex_height.max(1);
        let missing = level.texture.is_none();
        let full = missing || level.full_dirty;
        let chunks: Vec<(i32, i32)> = if full {
            Vec::new()
        } else {
            level.dirty_chunks.iter().copied().collect()
        };
        if !full && chunks.is_empty() {
            return;
        }
        if full {
            let rgba = doc.composite_overview_rect(max_side, 0, 0, tw, th);
            if missing {
                self.allocate_level(index, device, tw, th, &rgba, queue);
            } else if let Some(level) = self.levels.get_mut(index) {
                if let Some(texture) = level.texture.as_ref() {
                    write_rect(queue, texture, 0, 0, tw, th, &rgba);
                }
                level.full_dirty = false;
                level.dirty_chunks.clear();
            }
            return;
        }
        let dw = doc.width.max(1);
        let dh = doc.height.max(1);
        if let Some(level) = self.levels.get_mut(index) {
            if let Some(texture) = level.texture.as_ref() {
                for (cx, cy) in chunks {
                    let Some((x, y, w, h)) = chunk_tex_rect(cx, cy, dw, dh, tw, th) else {
                        continue;
                    };
                    let rgba = doc.composite_overview_rect(max_side, x, y, w, h);
                    write_rect(queue, texture, x, y, w, h, &rgba);
                }
            }
            level.dirty_chunks.clear();
        }
    }

    fn allocate_level(
        &mut self,
        index: usize,
        device: &wgpu::Device,
        tw: u32,
        th: u32,
        rgba: &[u8],
        queue: &wgpu::Queue,
    ) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("overview"),
            size: wgpu::Extent3d {
                width: tw,
                height: th,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        write_rect(queue, &texture, 0, 0, tw, th, rgba);
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
        if let Some(level) = self.levels.get_mut(index) {
            level.texture = Some(texture);
            level.bind_group = Some(bind_group);
            level.full_dirty = false;
            level.dirty_chunks.clear();
        }
        self.allocations += 1;
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
        let Some(bg) = self
            .levels
            .get(self.displayed)
            .and_then(|level| level.bind_group.as_ref())
        else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bg, &[]);
        pass.draw(0..6, 0..1);
    }
}

fn write_rect(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    rgba: &[u8],
) {
    if w == 0 || h == 0 || rgba.is_empty() {
        return;
    }
    let row = w * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded = row.div_ceil(align) * align;
    let layout = wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(padded),
        rows_per_image: Some(h),
    };
    let dest = wgpu::TexelCopyTextureInfo {
        texture,
        mip_level: 0,
        origin: wgpu::Origin3d { x, y, z: 0 },
        aspect: wgpu::TextureAspect::All,
    };
    let size = wgpu::Extent3d {
        width: w,
        height: h,
        depth_or_array_layers: 1,
    };
    if padded == row {
        queue.write_texture(dest, rgba, layout, size);
        return;
    }
    let mut packed = vec![0u8; padded as usize * h as usize];
    for y in 0..h as usize {
        let src = y * row as usize;
        let dst = y * padded as usize;
        packed[dst..dst + row as usize].copy_from_slice(&rgba[src..src + row as usize]);
    }
    queue.write_texture(dest, &packed, layout, size);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_gpu::{gpu, read_texture_2d, Gpu};
    use calumma_core::tile::DocRect;
    use calumma_core::vector::VectorShape;
    use calumma_core::{DeviceTier, MemoryPressureLevel, Shape, Tool};

    fn pass(gpu: &Gpu) -> OverviewPass {
        OverviewPass::new(
            &gpu.device,
            &gpu.shader,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        )
    }

    fn budget() -> GpuBudget {
        GpuBudget::new(DeviceTier::Standard)
    }

    fn doc(width: u32, height: u32) -> Document {
        Document::new("p".into(), "t", width, height)
    }

    fn sync(overview: &mut OverviewPass, doc: &mut Document, gpu: &Gpu) {
        overview.sync(doc, &gpu.device, &gpu.queue, &budget());
    }

    fn has_texture(overview: &OverviewPass) -> bool {
        overview
            .levels
            .get(overview.displayed)
            .is_some_and(|level| level.texture.is_some())
    }

    fn resident_textures(overview: &OverviewPass) -> usize {
        overview
            .levels
            .iter()
            .filter(|level| level.texture.is_some())
            .count()
    }

    fn paint(doc: &mut Document, rect: DocRect, rgba: [u8; 4]) {
        let layer = doc.active_layer;
        doc.layers[layer]
            .tiles_mut()
            .unwrap()
            .paint_rect(rect, |_, _, _| Some(rgba));
    }

    fn gpu_matches_flatten(overview: &OverviewPass, doc: &Document, gpu: &Gpu) {
        let level = overview
            .levels
            .get(overview.displayed)
            .and_then(|level| level.texture.as_ref().map(|texture| (level, texture)))
            .expect("displayed level");
        let (tw, th) = (level.0.tex_width, level.0.tex_height);
        let cpu = doc.composite_overview_rect(level.0.max_side, 0, 0, tw, th);
        let gpu_px = read_texture_2d(&gpu.device, &gpu.queue, level.1, tw, th);
        assert_eq!(gpu_px, cpu, "{}x{} flatten mismatch", tw, th);
    }

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
        let mut d = doc(512, 512);

        sync(&mut overview, &mut d, gpu);

        assert!(!has_texture(&overview), "no upload while inactive");
        assert!(overview.dirty);
    }

    #[test]
    fn an_active_pass_uploads_once_and_then_leaves_the_texture_alone() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);
        let mut d = doc(512, 256);
        overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);

        sync(&mut overview, &mut d, gpu);
        assert!(has_texture(&overview));
        assert!(!overview.dirty);
        assert_eq!((overview.doc_width, overview.doc_height), (512, 256));
        assert_eq!((overview.tex_width, overview.tex_height), (512, 256));

        let (w, h) = (overview.tex_width, overview.tex_height);
        sync(&mut overview, &mut d, gpu);
        assert_eq!((overview.tex_width, overview.tex_height), (w, h));
    }

    #[test]
    fn a_document_resize_re_uploads_without_anything_marking_it_dirty() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);
        overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
        sync(&mut overview, &mut doc(512, 256), gpu);

        sync(&mut overview, &mut doc(256, 512), gpu);

        assert_eq!((overview.doc_width, overview.doc_height), (256, 512));
        assert_eq!((overview.tex_width, overview.tex_height), (256, 512));
    }

    #[test]
    fn marking_dirty_makes_the_next_sync_upload_again() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);
        let mut d = doc(128, 128);
        overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
        sync(&mut overview, &mut d, gpu);

        overview.mark_dirty();
        assert!(overview.dirty);
        sync(&mut overview, &mut d, gpu);
        assert!(!overview.dirty);
    }

    #[test]
    fn re_uploading_at_the_same_size_keeps_the_texture_it_already_had() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);
        let mut d = doc(512, 256);
        overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
        sync(&mut overview, &mut d, gpu);
        assert_eq!(overview.allocations, 1);

        paint(&mut d, DocRect::new(10, 10, 20, 20), [255, 0, 0, 255]);
        sync(&mut overview, &mut d, gpu);

        assert_eq!(
            overview.allocations, 1,
            "a paint patches the texture it already had"
        );
        assert!(!overview.dirty);
        gpu_matches_flatten(&overview, &d, gpu);
    }

    #[test]
    fn a_resize_replaces_the_texture_and_its_bind_group() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);
        overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
        sync(&mut overview, &mut doc(512, 256), gpu);
        assert_eq!(overview.allocations, 1);

        sync(&mut overview, &mut doc(256, 512), gpu);

        assert_eq!(overview.allocations, 2);
        assert_eq!((overview.tex_width, overview.tex_height), (256, 512));
    }

    #[test]
    fn prewarm_uploads_while_inactive_and_only_once_per_request() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);
        let mut d = doc(128, 128);

        overview.prewarm(&mut d, &gpu.device, &gpu.queue, &budget());
        assert!(!has_texture(&overview), "nothing was requested yet");

        overview.request_prewarm();
        overview.prewarm(&mut d, &gpu.device, &gpu.queue, &budget());
        assert!(has_texture(&overview));
        assert!(!overview.prewarm_pending);
        assert!(!overview.active, "prewarming does not turn the overview on");

        overview.mark_dirty();
        overview.prewarm(&mut d, &gpu.device, &gpu.queue, &budget());
        assert!(overview.dirty, "a spent request does not upload again");
    }

    #[test]
    fn clearing_drops_the_texture_and_the_pass_falls_back_to_drawing_nothing() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);
        overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
        sync(&mut overview, &mut doc(128, 128), gpu);

        overview.clear();

        assert!(!has_texture(&overview));
        assert!(overview.levels.is_empty());
        assert!(!overview.active);
        assert!(overview.dirty);
        assert_eq!((overview.doc_width, overview.doc_height), (0, 0));
    }

    #[test]
    fn the_camera_uniform_stays_the_size_the_shader_declares() {
        let Some(gpu) = gpu() else { return };
        let overview = pass(gpu);
        let mut d = doc(128, 128);
        d.camera.pan_x = 12.0;

        overview.write_camera(&gpu.queue, &d, [800.0, 600.0]);

        assert_eq!(std::mem::size_of::<OverviewCamera>(), 10 * 4);
    }

    #[test]
    fn zooming_in_picks_a_finer_pyramid_level() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);
        let mut d = doc(512, 512);
        d.resize_viewport(64.0, 64.0, 1.0);
        d.fit_to_view();
        overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
        sync(&mut overview, &mut d, gpu);
        let far = overview.tex_width;

        d.camera.zoom = 1.0;
        sync(&mut overview, &mut d, gpu);

        assert!(
            overview.tex_width >= far,
            "a closer camera must not pick a coarser flatten ({far} -> {})",
            overview.tex_width
        );
        assert_eq!(overview.tex_width, 512);
    }

    #[test]
    fn painting_one_tile_reuses_the_texture_and_clears_overview_dirty() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);
        let mut d = doc(512, 512);
        overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
        sync(&mut overview, &mut d, gpu);
        assert_eq!(overview.allocations, 1);
        assert!(d.layers[0]
            .dirty_tiles(DirtyChannel::Overview)
            .is_some_and(|set| set.is_empty()));

        let layer = d.active_layer;
        d.layers[layer]
            .tiles_mut()
            .unwrap()
            .paint_rect(DocRect::new(10, 10, 20, 20), |_, _, _| {
                Some([255, 0, 0, 255])
            });
        assert!(d.layers[layer]
            .dirty_tiles(DirtyChannel::Overview)
            .is_some_and(|set| !set.is_empty()));

        sync(&mut overview, &mut d, gpu);
        assert_eq!(overview.allocations, 1);
        assert!(d.layers[layer]
            .dirty_tiles(DirtyChannel::Overview)
            .is_some_and(|set| set.is_empty()));
        assert!(!overview.levels[overview.displayed].full_dirty);
        gpu_matches_flatten(&overview, &d, gpu);
    }

    #[test]
    fn an_overview_flatten_does_not_clear_the_render_channel() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);
        let mut d = doc(128, 128);
        overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
        assert!(d.layers[0]
            .dirty_tiles(DirtyChannel::Render)
            .is_some_and(|set| !set.is_empty()));

        sync(&mut overview, &mut d, gpu);

        assert!(d.layers[0]
            .dirty_tiles(DirtyChannel::Overview)
            .is_some_and(|set| set.is_empty()));
        assert!(d.layers[0]
            .dirty_tiles(DirtyChannel::Render)
            .is_some_and(|set| !set.is_empty()));
    }

    #[test]
    fn an_odd_aspect_document_uploads_bytes_the_gpu_can_round_trip() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);
        let mut d = doc(200, 300);
        overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
        sync(&mut overview, &mut d, gpu);
        gpu_matches_flatten(&overview, &d, gpu);
    }

    #[test]
    fn zooming_out_allocates_a_coarser_level_and_zooming_in_reuses_the_fine_one() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);
        let mut d = doc(2048, 2048);
        overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
        d.camera.zoom = 1.0;
        d.camera.dpr = 1.0;
        sync(&mut overview, &mut d, gpu);
        assert_eq!(overview.tex_width, 2048);
        assert_eq!(overview.allocations, 1);

        d.camera.zoom = 0.1;
        sync(&mut overview, &mut d, gpu);
        assert!(
            overview.tex_width < 2048,
            "zoomed out picks a coarser level"
        );
        assert_eq!(overview.allocations, 2);
        assert_eq!(resident_textures(&overview), 2);

        d.camera.zoom = 1.0;
        sync(&mut overview, &mut d, gpu);
        assert_eq!(overview.tex_width, 2048);
        assert_eq!(overview.allocations, 2, "the fine level is still there");
        gpu_matches_flatten(&overview, &d, gpu);
    }

    #[test]
    fn warn_pressure_drops_levels_that_are_not_on_screen() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);
        let mut d = doc(2048, 2048);
        overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
        d.camera.zoom = 1.0;
        sync(&mut overview, &mut d, gpu);
        d.camera.zoom = 0.1;
        sync(&mut overview, &mut d, gpu);
        assert_eq!(resident_textures(&overview), 2);

        let mut warn = GpuBudget::new(DeviceTier::Standard);
        warn.report_pressure(MemoryPressureLevel::Warn);
        overview.sync(&mut d, &gpu.device, &gpu.queue, &warn);

        assert_eq!(resident_textures(&overview), 1);
        assert!(has_texture(&overview));
        gpu_matches_flatten(&overview, &d, gpu);
    }

    #[test]
    fn a_low_tier_budget_never_builds_a_level_past_its_cap() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);
        let mut d = doc(4096, 4096);
        overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
        d.camera.zoom = 1.0;
        let low = GpuBudget::new(DeviceTier::Low);
        overview.sync(&mut d, &gpu.device, &gpu.queue, &low);
        assert_eq!(overview.tex_width, 2048);
        gpu_matches_flatten(&overview, &d, gpu);
    }

    #[test]
    fn hiding_a_layer_rewrites_the_displayed_level_in_place() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);
        let mut d = doc(256, 256);
        overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
        paint(&mut d, DocRect::new(8, 8, 40, 40), [200, 30, 40, 255]);
        sync(&mut overview, &mut d, gpu);
        gpu_matches_flatten(&overview, &d, gpu);

        d.layers[1].visible = false;
        sync(&mut overview, &mut d, gpu);
        assert_eq!(overview.allocations, 1);
        gpu_matches_flatten(&overview, &d, gpu);
    }

    #[test]
    fn recoloring_a_vector_rebuilds_the_flatten_without_a_new_texture() {
        let Some(gpu) = gpu() else { return };
        let mut overview = pass(gpu);
        let mut d = doc(128, 128);
        d.add_vector_layer(
            "V",
            calumma_core::VectorItem::Shape(VectorShape {
                shape: Shape {
                    tool: Tool::Rect,
                    start: (16.0, 16.0),
                    end: (96.0, 96.0),
                    half_width: 1.0,
                    fill: true,
                    stroke: false,
                },
                color: [0, 90, 220, 255],
                stroke_color: [0, 90, 220, 255],
            }),
        );
        overview.should_use(OVERVIEW_ENTER_TILE_THRESHOLD, false);
        sync(&mut overview, &mut d, gpu);
        gpu_matches_flatten(&overview, &d, gpu);

        let Some(calumma_core::VectorItem::Shape(shape)) =
            d.layers.last_mut().unwrap().content.item_mut()
        else {
            panic!("vector layer");
        };
        shape.color = [220, 30, 30, 255];
        sync(&mut overview, &mut d, gpu);
        assert_eq!(overview.allocations, 1);
        gpu_matches_flatten(&overview, &d, gpu);
    }
}
