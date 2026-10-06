//! The offscreen buffer that lets a live brush stroke preview at any opacity without the
//! overlaps between its own segments compounding into a dark, beaded rope.
//!
//! A stroke is drawn as one capsule per recorded pair of points, and consecutive capsules
//! overlap almost entirely when the pointer moves slowly. Alpha-blending them straight onto
//! the board therefore composites the same ink over itself dozens of times per stroke, which
//! is invisible at full opacity and ruinous below it. So the capsules go into a single-channel
//! coverage target with `Max` blending — union, not sum — and the board gets one composite of
//! the finished shape. The CPU does the same thing at commit time in `coverage.rs`, which is
//! why the stroke does not change when the pointer comes up.
//!
//! The target is allocated the first time a brush stroke needs it and only ever resized to the
//! surface, so a session that never paints never pays for it.

use crate::framebuffer::PxRect;

#[cfg(test)]
mod tests;

const COVERAGE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

pub(crate) struct StrokeCoverage {
    bgl: wgpu::BindGroupLayout,
    coverage_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    target: Option<Target>,
    width: u32,
    height: u32,
}

struct Target {
    /// Held so the coverage the GPU produced can be copied back and checked against the
    /// engine's own `stroke_coverage`, which is the only way to know the shader still agrees
    /// with it. Nothing in a running app reads it.
    #[allow(dead_code)]
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

impl StrokeCoverage {
    pub(crate) fn new(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        preview_bgl: &wgpu::BindGroupLayout,
        stroke_layout: wgpu::VertexBufferLayout<'_>,
        format: wgpu::TextureFormat,
    ) -> Self {
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("stroke-coverage-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let coverage_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("stroke-coverage-pl"),
            bind_group_layouts: &[Some(preview_bgl)],
            ..Default::default()
        });
        let coverage_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("stroke-coverage"),
            layout: Some(&coverage_pl),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_stroke"),
                compilation_options: Default::default(),
                buffers: &[Some(stroke_layout)],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_stroke_coverage"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COVERAGE_FORMAT,
                    blend: Some(wgpu::BlendState {
                        color: MAX_BLEND,
                        alpha: MAX_BLEND,
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

        let composite_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("stroke-composite-pl"),
            bind_group_layouts: &[Some(preview_bgl), Some(&bgl)],
            ..Default::default()
        });
        let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("stroke-composite"),
            layout: Some(&composite_pl),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_shape_preview"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_stroke_composite"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
            coverage_pipeline,
            composite_pipeline,
            target: None,
            width: 0,
            height: 0,
        }
    }

    /// Makes sure a coverage target of this size exists, reporting whether it had to make a
    /// new one. A fresh texture has no accumulated coverage in it, so the caller has to start
    /// the current stroke over rather than appending to pixels that are no longer there.
    pub(crate) fn ensure(&mut self, device: &wgpu::Device, width: u32, height: u32) -> bool {
        let (width, height) = (width.max(1), height.max(1));
        if self.target.is_some() && self.width == width && self.height == height {
            return false;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("stroke-coverage"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COVERAGE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stroke-coverage-bg"),
            layout: &self.bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        });
        self.target = Some(Target {
            texture,
            view,
            bind_group,
        });
        self.width = width;
        self.height = height;
        true
    }

    pub(crate) fn release(&mut self) {
        self.target = None;
        self.width = 0;
        self.height = 0;
    }

    /// Rasterize stroke segments into the coverage target, unioned rather than summed.
    /// Scissored to the paper so a stroke that runs off the board cannot smear into the desk.
    ///
    /// `range` is the segments *added since the last call*, not the whole stroke, and `restart`
    /// says whether the target has to be wiped first. Appending is exact rather than an
    /// approximation: the blend op is `Max`, which is idempotent and order-independent, so
    /// unioning segment N into pixels that already hold the union of segments 0..N is the same
    /// value as unioning 0..N+1 from an empty target. So a frame costs the segments the pointer actually
    /// travelled, not the whole stroke.
    pub(crate) fn accumulate(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        preview_bg: &wgpu::BindGroup,
        instances: &wgpu::Buffer,
        range: std::ops::Range<u32>,
        scissor: Option<PxRect>,
        restart: bool,
    ) {
        let Some(target) = &self.target else {
            return;
        };
        if range.is_empty() && !restart {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("stroke-coverage"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    // Only a restart wipes the target. Every other frame loads what the
                    // previous frames accumulated and unions this frame's segments onto it.
                    load: if restart {
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            ..Default::default()
        });
        if let Some((x, y, w, h)) = scissor {
            pass.set_scissor_rect(x, y, w, h);
        }
        if range.is_empty() {
            return;
        }
        pass.set_pipeline(&self.coverage_pipeline);
        pass.set_bind_group(0, preview_bg, &[]);
        pass.set_vertex_buffer(0, instances.slice(..));
        pass.draw(0..6, range);
    }

    /// Lay the accumulated shape onto the board once, in the stroke's ink.
    pub(crate) fn composite(&self, pass: &mut wgpu::RenderPass<'_>, preview_bg: &wgpu::BindGroup) {
        let Some(target) = &self.target else {
            return;
        };
        pass.set_pipeline(&self.composite_pipeline);
        pass.set_bind_group(0, preview_bg, &[]);
        pass.set_bind_group(1, &target.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

const MAX_BLEND: wgpu::BlendComponent = wgpu::BlendComponent {
    src_factor: wgpu::BlendFactor::One,
    dst_factor: wgpu::BlendFactor::One,
    operation: wgpu::BlendOperation::Max,
};
