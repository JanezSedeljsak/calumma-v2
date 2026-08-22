pub type PxRect = (u32, u32, u32, u32);

/// Where a camera-only pan can read valid pixels from and where they land, given the on-screen
/// paper rect at the last full redraw (`reference`) and now (`current`), and the device-pixel
/// shift between them. `None` means the overlap is empty (a jump too large, or a corner case at
/// a viewport edge) — the caller falls back to a full redraw rather than blitting garbage.
pub fn shift_plan(
    reference: PxRect,
    current: PxRect,
    dx: i32,
    dy: i32,
    tex_w: u32,
    tex_h: u32,
) -> Option<(PxRect, PxRect)> {
    let (cx, cy, cw, ch) = current;
    let (rx, ry, rw, rh) = reference;

    let src_x0 = cx as i64 - dx as i64;
    let src_y0 = cy as i64 - dy as i64;
    let src_x1 = src_x0 + cw as i64;
    let src_y1 = src_y0 + ch as i64;

    let lo_x = src_x0.max(rx as i64).max(0);
    let lo_y = src_y0.max(ry as i64).max(0);
    let hi_x = src_x1.min(rx as i64 + rw as i64).min(tex_w as i64);
    let hi_y = src_y1.min(ry as i64 + rh as i64).min(tex_h as i64);
    if hi_x <= lo_x || hi_y <= lo_y {
        return None;
    }
    let src: PxRect = (
        lo_x as u32,
        lo_y as u32,
        (hi_x - lo_x) as u32,
        (hi_y - lo_y) as u32,
    );

    let dst_x0 = lo_x + dx as i64;
    let dst_y0 = lo_y + dy as i64;
    if dst_x0 < 0
        || dst_y0 < 0
        || dst_x0 + src.2 as i64 > tex_w as i64
        || dst_y0 + src.3 as i64 > tex_h as i64
    {
        return None;
    }
    let dst: PxRect = (dst_x0 as u32, dst_y0 as u32, src.2, src.3);
    Some((src, dst))
}

/// `outer \ inner` as up to four non-overlapping bands (top, bottom, left, right), assuming
/// `inner` is fully contained in `outer` — which `shift_plan`'s `dst` always is, by
/// construction. This is what has to be redrawn after a shift: everything the copy could not
/// have populated, because it was never part of the previous frame's paper rect.
pub fn exposed_rects(outer: PxRect, inner: PxRect) -> [Option<PxRect>; 4] {
    let (ox, oy, ow, oh) = outer;
    let (ix, iy, iw, ih) = inner;

    let top = (iy > oy).then_some((ox, oy, ow, iy - oy));
    let bottom_y = iy + ih;
    let bottom = (bottom_y < oy + oh).then_some((ox, bottom_y, ow, (oy + oh) - bottom_y));
    let left = (ix > ox).then_some((ox, iy, ix - ox, ih));
    let right_x = ix + iw;
    let right = (right_x < ox + ow).then_some((right_x, iy, (ox + ow) - right_x, ih));

    [top, bottom, left, right]
}

#[derive(Clone, Copy)]
pub(crate) enum PanCacheSlot {
    Reference,
    Working,
}

struct Slot {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

fn make_slot(
    device: &wgpu::Device,
    bgl: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> Slot {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pan-cache"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("pan-cache-bg"),
        layout: bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    Slot {
        texture,
        view,
        bind_group,
    }
}

/// Two fixed-role offscreen colour targets, not a true alternating ping-pong: `reference`
/// holds the last full content redraw and is only ever replaced by another full redraw;
/// `working` is rebuilt from `reference` every camera-only frame. Blitting always measures the
/// pixel shift from the same frozen `reference` pan rather than chaining frame-to-frame, so
/// per-frame rounding to whole device pixels cannot accumulate into visible drift over a long
/// pan gesture.
pub(crate) struct PanCache {
    bgl: wgpu::BindGroupLayout,
    blit_pipeline: wgpu::RenderPipeline,
    clear_pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    format: wgpu::TextureFormat,
    reference: Option<Slot>,
    working: Option<Slot>,
    width: u32,
    height: u32,
    has_reference: bool,
    reference_pan: (f32, f32),
    reference_zoom: f32,
    reference_dpr: f32,
    reference_scissor: PxRect,
}

impl PanCache {
    pub(crate) fn new(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        format: wgpu::TextureFormat,
    ) -> Self {
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pan-cache-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("pan-cache-sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pan-cache-pl"),
            bind_group_layouts: &[Some(&bgl)],
            ..Default::default()
        });
        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pan-cache-blit"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_blit"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_blit"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState {
                        color: crate::renderer::PREMULTIPLIED_ALPHA_COMPONENT,
                        alpha: crate::renderer::PREMULTIPLIED_ALPHA_COMPONENT,
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
        let clear_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pan-cache-clear-pl"),
            bind_group_layouts: &[],
            ..Default::default()
        });
        let clear_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pan-cache-clear"),
            layout: Some(&clear_pl),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_clear_transparent"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
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
            blit_pipeline,
            clear_pipeline,
            sampler,
            format,
            reference: None,
            working: None,
            width: 0,
            height: 0,
            has_reference: false,
            reference_pan: (0.0, 0.0),
            reference_zoom: 1.0,
            reference_dpr: 1.0,
            reference_scissor: (0, 0, 0, 0),
        }
    }

    pub(crate) fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.width == width && self.height == height && self.reference.is_some() {
            return;
        }
        self.width = width;
        self.height = height;
        self.reference = Some(make_slot(
            device,
            &self.bgl,
            &self.sampler,
            self.format,
            width,
            height,
        ));
        self.working = Some(make_slot(
            device,
            &self.bgl,
            &self.sampler,
            self.format,
            width,
            height,
        ));
        self.has_reference = false;
    }

    pub(crate) fn invalidate(&mut self) {
        self.has_reference = false;
    }

    pub(crate) fn reference_texture(&self) -> &wgpu::Texture {
        &self.reference.as_ref().expect("PanCache not sized").texture
    }

    pub(crate) fn reference_view(&self) -> &wgpu::TextureView {
        &self.reference.as_ref().expect("PanCache not sized").view
    }

    pub(crate) fn working_texture(&self) -> &wgpu::Texture {
        &self.working.as_ref().expect("PanCache not sized").texture
    }

    pub(crate) fn working_view(&self) -> &wgpu::TextureView {
        &self.working.as_ref().expect("PanCache not sized").view
    }

    pub(crate) fn bind_group(&self, slot: PanCacheSlot) -> &wgpu::BindGroup {
        let slot = match slot {
            PanCacheSlot::Reference => self.reference.as_ref(),
            PanCacheSlot::Working => self.working.as_ref(),
        };
        &slot.expect("PanCache not sized").bind_group
    }

    pub(crate) fn blit_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.blit_pipeline
    }

    pub(crate) fn clear_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.clear_pipeline
    }

    /// Whether the reference texture already holds exactly this frame's content — same camera,
    /// same scale, same paper rect. When it does there is nothing to shift and nothing to
    /// redraw: the board pass samples `reference` directly and the content pass is skipped
    /// outright. This is what makes an overlay-only frame (a pen stroke's preview, a blinking
    /// caret) cost one instance-buffer write instead of a full recomposite.
    pub(crate) fn reference_matches(
        &self,
        pan: (f32, f32),
        zoom: f32,
        dpr: f32,
        scissor: PxRect,
    ) -> bool {
        self.has_reference
            && self.reference_pan == pan
            && self.reference_zoom == zoom
            && self.reference_dpr == dpr
            && self.reference_scissor == scissor
    }

    /// Shift + destination rect for this frame's blit, in device pixels, or `None` when the
    /// camera moved too far (or zoomed/rescaled) for a straight pixel copy to apply — the
    /// caller falls back to a full redraw either way.
    pub(crate) fn plan(
        &self,
        pan: (f32, f32),
        zoom: f32,
        dpr: f32,
        scissor: PxRect,
    ) -> Option<(PxRect, PxRect)> {
        if !self.has_reference || zoom != self.reference_zoom || dpr != self.reference_dpr {
            return None;
        }
        let dx = ((pan.0 - self.reference_pan.0) * dpr).round() as i32;
        let dy = ((pan.1 - self.reference_pan.1) * dpr).round() as i32;
        shift_plan(
            self.reference_scissor,
            scissor,
            dx,
            dy,
            self.width,
            self.height,
        )
    }

    pub(crate) fn commit_reference(
        &mut self,
        pan: (f32, f32),
        zoom: f32,
        dpr: f32,
        scissor: PxRect,
    ) {
        self.reference_pan = pan;
        self.reference_zoom = zoom;
        self.reference_dpr = dpr;
        self.reference_scissor = scissor;
        self.has_reference = true;
    }
}
