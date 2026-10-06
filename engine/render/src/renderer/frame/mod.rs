use super::*;

mod board_pass;
mod layer_draw;
mod overlays;
mod tile_sync;

use board_pass::BoardPass;

/// What `prepare_frame` decided, read by every later phase of the same frame.
#[derive(Clone, Copy)]
pub(super) struct FramePlan {
    pub(super) viewport: [f32; 2],
    pub(super) use_overview: bool,
    pub(super) need_tile_sync: bool,
    pub(super) need_draw_rebuild: bool,
    pub(super) camera_only: bool,
}

/// Where each overlay pass's instances sit in the stroke buffer this frame.
pub(super) struct OverlayRanges {
    pub(super) overlay: std::ops::Range<u32>,
    pub(super) screen: std::ops::Range<u32>,
    pub(super) brush_ring: std::ops::Range<u32>,
    pub(super) brush: std::ops::Range<u32>,
    pub(super) brush_active: bool,
    pub(super) brush_restart: bool,
}

impl Renderer {
    pub fn render(&mut self, doc: &mut Document) {
        // The caret is a square wave: comparing its phase against the frame actually drawn
        // costs two frames a second instead of a display-rate pass, and still catches the
        // caret going away. A background-removal sweep is continuous, so it is not part of
        // that comparison — while it runs, every presented frame redraws the overlay.
        let sweep = doc.background_removal_animating();
        let caret_phase = (!sweep && doc.text_editing())
            .then(|| text_caret_visible(self.started.elapsed().as_secs_f32()));
        if self.frame_dirty == FrameDirty::Clean
            && !doc.has_live_preview()
            && !sweep
            && caret_phase == self.drawn_caret_phase
        {
            return;
        }

        let plan = self.prepare_frame(doc);
        let FramePlan {
            viewport,
            use_overview,
            need_tile_sync,
            need_draw_rebuild,
            camera_only,
        } = plan;
        self.write_frame_uniforms(doc, viewport);

        let scissor: Option<PxRect> = doc.camera.paper_scissor(
            doc.width as f32,
            doc.height as f32,
            self.config.width,
            self.config.height,
        );
        let pan = (doc.camera.pan_x, doc.camera.pan_y);
        // The pan cache holds this frame's content already when nothing it depends on has
        // moved: no tile resync, no draw-list rebuild, and the same camera it was captured at.
        // That is every overlay-only frame — a pen stroke between pointer-down and pointer-up,
        // a shape being dragged out, a blinking caret — and it means the content pass is
        // skipped entirely rather than recompositing the visible stack behind an overlay that
        // is the only thing that changed.
        let reuse_reference = !use_overview
            && !need_tile_sync
            && !need_draw_rebuild
            && scissor.is_some_and(|s| {
                self.pan_cache
                    .reference_matches(pan, doc.camera.zoom, doc.camera.dpr, s)
            });
        let blit_plan = if !use_overview && camera_only && !need_draw_rebuild && !reuse_reference {
            scissor.and_then(|s| self.pan_cache.plan(pan, doc.camera.zoom, doc.camera.dpr, s))
        } else {
            None
        };

        let ink = doc.ink_rgba();
        let color = [
            ink[0] as f32 / 255.0,
            ink[1] as f32 / 255.0,
            ink[2] as f32 / 255.0,
            ink[3] as f32 / 255.0,
        ];
        let preview_shape = self.write_preview_uniforms(doc, viewport, color);

        let guide_count = self.write_guides(doc);
        let overlays = self.build_overlays(doc, plan, pan, color);
        self.upload_draw_caches(need_draw_rebuild);

        let (view, acquired) = match &mut self.output {
            FrameOutput::Surface(surface) => {
                let frame = match surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(t)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
                    wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                        surface.configure(&self.device, &self.config);
                        return;
                    }
                    _ => return,
                };
                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                (view, AcquiredFrame::Surface(frame))
            }
            #[cfg(test)]
            FrameOutput::Headless(texture) => {
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                (view, AcquiredFrame::Headless)
            }
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });

        // The content pass, in one of three modes: reuse what the `PanCache` reference already
        // holds, shift it by this frame's pan and patch the strips that exposes, or redraw the
        // visible stack into it from scratch. All three leave the frame's content in the
        // reference texture, so the board pass below only ever draws a single textured quad for
        // the content, not the tile/vector instance list.
        let has_content = !use_overview
            && scissor
                .map(|s| {
                    if reuse_reference {
                        return;
                    }
                    if let Some(plan) = blit_plan {
                        self.patch_pan_cache_working(&mut encoder, plan, doc.camera.dpr, s);
                    } else {
                        self.redraw_pan_cache_reference(
                            &mut encoder,
                            pan,
                            doc.camera.zoom,
                            doc.camera.dpr,
                            s,
                        );
                    }
                })
                .is_some();

        if overlays.brush_active {
            // `accumulate` no-ops on an empty range that is not a restart, which is the frame
            // where the pointer has not moved far enough to add a segment — the target already
            // holds the whole stroke and the board pass below still composites it.
            self.stroke_coverage.accumulate(
                &mut encoder,
                &self.preview_bg,
                &self.stroke_buf,
                overlays.brush.clone(),
                scissor,
                overlays.brush_restart,
            );
        }

        self.draw_board_pass(
            &mut encoder,
            &view,
            doc,
            BoardPass {
                scissor,
                use_overview,
                has_content,
                guide_count,
                overlays: &overlays,
                has_preview_shape: preview_shape.is_some(),
            },
        );

        self.queue.submit(Some(encoder.finish()));
        match acquired {
            AcquiredFrame::Surface(frame) => self.queue.present(frame),
            #[cfg(test)]
            AcquiredFrame::Headless => {}
        }
        self.tick_camera_motion();
        // A gesture in flight asks for another frame, but only an *overlay* one: the pointer
        // events that move it already invalidate at the right level — `Content` for anything
        // that touched tiles, vectors or a transform, `Overlay` for a preview that is drawn on
        // top of content nobody changed. Pinning `Content` here instead re-synced every tile
        // and recomposited the whole stack on every frame of every stroke, for a stroke that
        // lays no pixels down until pointer-up.
        // A caret no longer pins `Overlay` — the phase comparison at the top of the frame is what
        // asks for its next one, and pinning `Overlay` here would defeat that by making the
        // early-out unreachable for as long as a text session was open. The removal sweep does
        // pin it: the band has to move on every frame, and an overlay pin still skips the tiles.
        self.frame_dirty = if doc.has_live_preview() || doc.background_removal_animating() {
            FrameDirty::Overlay
        } else {
            FrameDirty::Clean
        };
        // Recorded here rather than at the top, so a frame abandoned on a lost surface leaves the
        // caret asking to be drawn instead of counting as drawn.
        self.drawn_caret_phase = caret_phase;
    }

    /// Everything the frame decides before it writes a byte: which path draws the content
    /// (the overview or the tiles), and what has to be re-synced or rebuilt for it.
    fn prepare_frame(&mut self, doc: &mut Document) -> FramePlan {
        let (dw, dh) = doc.camera.device_size();
        self.resize(dw, dh);
        self.pan_cache
            .resize(&self.device, self.config.width, self.config.height);
        self.ensure_backdrop(
            doc.layers
                .iter()
                .any(|layer| layer.visible && !layer.blend_mode.is_fixed_function()),
        );

        let viewport = [
            (self.config.width as f32).max(1.0),
            (self.config.height as f32).max(1.0),
        ];

        let busiest_layer_tiles =
            if matches!(self.frame_dirty, FrameDirty::Camera | FrameDirty::Overlay) {
                self.cached_tile_draw_count
                    .unwrap_or_else(|| self.busiest_layer_tile_count(doc))
            } else {
                let count = self.busiest_layer_tile_count(doc);
                self.cached_tile_draw_count = Some(count);
                count
            };
        let use_overview = self
            .overview
            .should_use(busiest_layer_tiles, doc.has_live_preview());

        let need_tile_sync = !use_overview
            && (self.frame_dirty == FrameDirty::Content
                || Self::retained_span(doc, self.budget.retention_margin_tiles())
                    != self.cached_retained_span
                || self.visible_upload_needed(doc));
        let need_draw_rebuild = !use_overview
            && (need_tile_sync || Self::visible_span(doc) != self.cached_visible_span);
        let camera_only =
            self.frame_dirty == FrameDirty::Camera && !doc.has_live_preview() && !use_overview;

        if use_overview {
            self.overview
                .sync(doc, &self.device, &self.queue, &self.budget);
            self.overview.write_camera(&self.queue, doc, viewport);
        } else {
            self.overview
                .prewarm(doc, &self.device, &self.queue, &self.budget);
            if need_tile_sync {
                self.sync_tiles(doc);
            }
            if need_draw_rebuild {
                // After `sync_tiles`, because solid Paper's row carries an atlas slot that only
                // exists once its tile is resident, and before the draw list, which indexes
                // these rows.
                self.write_layer_data(doc);
                self.rebuild_layer_cache(doc);
            }
        }

        FramePlan {
            viewport,
            use_overview,
            need_tile_sync,
            need_draw_rebuild,
            camera_only,
        }
    }

    fn write_frame_uniforms(&mut self, doc: &Document, viewport: [f32; 2]) {
        let desk = calumma_core::DeskMetrics::DEFAULT;
        if self
            .desk_lattice
            .ensure(&self.device, &self.queue, doc.camera.dpr)
        {
            self.paper_bg = paper_bind_group(
                &self.device,
                &self.paper_bgl,
                &self.paper_buf,
                &self.desk_lattice,
            );
        }
        let paper = PaperUniforms {
            pan: [doc.camera.pan_x, doc.camera.pan_y],
            zoom: doc.camera.zoom,
            dpr: doc.camera.dpr,
            doc_size: [doc.width as f32, doc.height as f32],
            viewport,
            dark: if doc.dark_theme { 1.0 } else { 0.0 },
            lattice_side: self.desk_lattice.shader_side(),
            _pad1: 0.0,
            _pad2: 0.0,
            desk_metrics: [
                desk.cell,
                desk.line_width,
                desk.cross_arm,
                desk.cross_line_width,
            ],
            desk: rgba_unit(doc.board_colors.desk),
            grid: rgba_unit(doc.board_colors.grid),
            paper_border: rgba_unit(doc.board_colors.paper_border),
        };
        self.queue
            .write_buffer(&self.paper_buf, 0, bytemuck::bytes_of(&paper));

        let tile_camera = TileCamera {
            pan: [doc.camera.pan_x, doc.camera.pan_y],
            zoom: doc.camera.zoom,
            dpr: doc.camera.dpr,
            viewport,
            doc_size: [doc.width as f32, doc.height as f32],
            crisp: f32::from(u8::from(doc.camera.zoom >= CRISP_PIXEL_ZOOM)),
            _pad: [0.0; 3],
        };
        self.queue
            .write_buffer(&self.tile_camera_buf, 0, bytemuck::bytes_of(&tile_camera));
    }
}

#[cfg(test)]
mod headless_tests;
