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

/// One camera-only frame's shift, in whole device pixels, plus where it reads and lands. The
/// shift travels with the rects because the caller has to commit it back into the reference
/// afterwards — see [`PanCache::commit_shift`].
#[derive(Clone, Copy)]
pub(crate) struct BlitPlan {
    pub(crate) src: PxRect,
    pub(crate) dst: PxRect,
    pub(crate) shift: (i32, i32),
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

/// Two offscreen color targets that alternate roles: `reference` holds the content the last
/// frame left behind, `working` is where the next shift lands, and the two swap once the shift
/// is done so the freshest pixels are always the ones the next frame reads from.
///
/// Freezing `reference` at the last *full redraw* instead — which is what this used to do —
/// looks like it avoids rounding drift, and it does, but at a price that only shows up mid
/// gesture: the shift is then measured from a point that recedes further with every frame, so
/// the overlap shrinks, the strips `exposed_rects` hands back grow linearly with how far the
/// camera has travelled since that redraw, and the whole draw list is replayed into an
/// ever-widening band until the overlap empties and a full redraw restarts the ramp. Pan cost
/// sawtoothed across a gesture rather than staying flat.
///
/// Chaining frame to frame costs nothing in accuracy as long as the reference pan is advanced
/// by the *rounded* delta that was actually blitted (`commit_shift`) rather than by the raw
/// camera pan. The reference then describes exactly where the pixels sit, every blit is an
/// integer-pixel copy, and the error is structurally zero instead of merely bounded.
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

    /// The texture the board pass samples. Always `reference`: a full redraw writes it
    /// directly, and a shift writes `working` and then swaps it into place, so whichever
    /// route the content pass took, the current frame's pixels are here.
    pub(crate) fn bind_group(&self) -> &wgpu::BindGroup {
        &self
            .reference
            .as_ref()
            .expect("PanCache not sized")
            .bind_group
    }

    pub(crate) fn blit_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.blit_pipeline
    }

    pub(crate) fn clear_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.clear_pipeline
    }

    /// How far this frame's camera has moved from the reference, rounded to whole device
    /// pixels. Whole pixels because that is the only shift a `copy_texture_to_texture` can
    /// express; the sub-pixel remainder is what `commit_shift` deliberately does *not* fold
    /// into the reference pan.
    fn shift_px(&self, pan: (f32, f32), dpr: f32) -> (i32, i32) {
        (
            ((pan.0 - self.reference_pan.0) * dpr).round() as i32,
            ((pan.1 - self.reference_pan.1) * dpr).round() as i32,
        )
    }

    /// Whether the reference texture already holds this frame's content — same scale, same
    /// paper rect, and a camera that has not moved by a whole device pixel. When it does there
    /// is nothing to shift and nothing to redraw: the board pass samples `reference` directly
    /// and the content pass is skipped outright. This is what makes an overlay-only frame (a
    /// pen stroke's preview, a blinking caret) cost one instance-buffer write instead of a
    /// full recomposite.
    ///
    /// The camera test is "no whole-pixel shift" rather than exact float equality because
    /// `commit_shift` leaves the reference pan on a device-pixel grid: after a blit the two
    /// differ by up to half a pixel, which is a shift the copy could not have expressed
    /// anyway. Demanding equality there would send every post-blit frame down the blit path
    /// to perform a zero-distance full-viewport copy.
    pub(crate) fn reference_matches(
        &self,
        pan: (f32, f32),
        zoom: f32,
        dpr: f32,
        scissor: PxRect,
    ) -> bool {
        self.has_reference
            && self.reference_zoom == zoom
            && self.reference_dpr == dpr
            && self.reference_scissor == scissor
            && self.shift_px(pan, dpr) == (0, 0)
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
    ) -> Option<BlitPlan> {
        if !self.has_reference || zoom != self.reference_zoom || dpr != self.reference_dpr {
            return None;
        }
        let (dx, dy) = self.shift_px(pan, dpr);
        let (src, dst) = shift_plan(
            self.reference_scissor,
            scissor,
            dx,
            dy,
            self.width,
            self.height,
        )?;
        Some(BlitPlan {
            src,
            dst,
            shift: (dx, dy),
        })
    }

    /// Promotes the just-shifted `working` texture to be the reference the next frame measures
    /// from, advancing the reference pan by the shift that was *actually* blitted rather than
    /// by the raw camera pan. Whole device pixels in, whole device pixels out — nothing is left
    /// over to accumulate, so a pan of any length stays exact without ever re-freezing the
    /// baseline and paying the widening-strip ramp that costs.
    pub(crate) fn commit_shift(&mut self, shift: (i32, i32), dpr: f32, scissor: PxRect) {
        let dpr = if dpr > 0.0 { dpr } else { 1.0 };
        self.reference_pan = (
            self.reference_pan.0 + shift.0 as f32 / dpr,
            self.reference_pan.1 + shift.1 as f32 / dpr,
        );
        self.reference_scissor = scissor;
        std::mem::swap(&mut self.reference, &mut self.working);
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
