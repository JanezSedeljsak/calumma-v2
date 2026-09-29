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

mod pyramid;

use pyramid::Level;

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
    /// many sparse layers should not be charged as if it were one layer painted edge to edge.
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

#[cfg(test)]
mod tests;
