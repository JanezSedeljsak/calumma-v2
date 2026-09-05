use super::*;

const ERASER_PREVIEW_COLOR: [f32; 4] = [0.5, 0.5, 0.5, 0.5];
const SELECTION_OUTLINE_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 0.9];
const SELECTION_OUTLINE_WIDTH: f32 = 1.5;

impl Renderer {
    pub(super) fn sync_tiles(&mut self, doc: &mut Document) {
        let Some(visible) = doc.visible_rect() else {
            return;
        };
        let retained = visible.expanded_by_tiles(self.budget.retention_margin_tiles());
        let doc_width = doc.width;

        let mut live: FxHashSet<TileKey> = FxHashSet::default();
        let mut visible_keys: FxHashSet<TileKey> = FxHashSet::default();
        let mut uploads: Vec<(usize, TileCoord, TileKey, bool)> = Vec::new();

        for layer_index in 0..doc.layers.len() {
            let layer = &doc.layers[layer_index];
            if !layer.visible {
                // A hidden layer keeps whatever it already has in the atlas. Dropping it would
                // make the eye icon cost a full re-upload — every tile recomposited and
                // re-mipped — on the way back, which on a document with many layers is seconds
                // of stalled main thread per click. Its tiles stay out of `visible_keys`, so
                // they are the first thing `evictable` gives up when the atlas runs short.
                if let Some(&slot) = self.layer_slots.get(&layer.id) {
                    let retain: Vec<TileKey> = self
                        .tiles
                        .keys()
                        .filter(|(s, _, _)| *s == slot)
                        .copied()
                        .collect();
                    live.extend(retain);
                }
                continue;
            }
            let Some(grid) = layer.tiles() else {
                continue;
            };
            let slot = self.layer_slot(&layer.id);
            let dirty = grid.dirty_tiles(DirtyChannel::Render);
            let visible_grid = layer.doc_rect_to_grid(visible);
            let retained_grid = layer.doc_rect_to_grid(retained);

            if layer.is_paper() && grid.whole_tiles_share_one_arc() {
                let coord = TileCoord { x: 0, y: 0 };
                let key: TileKey = (slot, 0, 0);
                live.insert(key);
                visible_keys.insert(key);
                let known = self.tiles.contains_key(&key);
                if !known || dirty.contains(&coord) || self.needs_full_mips(&key) {
                    uploads.push((layer_index, coord, key, self.may_skip_mips(&key)));
                }
                continue;
            }

            for coord in grid.coords_intersecting(retained_grid) {
                let cell = TileGrid::tile_rect(coord);
                let key: TileKey = (slot, coord.x, coord.y);
                live.insert(key);
                if !cell.intersects(visible_grid) {
                    continue;
                }
                visible_keys.insert(key);
                let known = self.tiles.contains_key(&key);
                if known && !dirty.contains(&coord) && !self.needs_full_mips(&key) {
                    continue;
                }
                uploads.push((layer_index, coord, key, self.may_skip_mips(&key)));
            }
        }

        // Bake the mask for every dirty tile up front and in parallel, alongside the mip chain
        // every upload needs regardless — both are pure pixel math that scales with tile count,
        // so both go through rayon rather than running sequentially on the frame thread once the
        // wgpu upload loop below gets to them. Adjustments and opacity no longer bake here at
        // all: `write_layer_data` puts them in the `LayerData` row and `fs_tile` evaluates them
        // per pixel at draw time, so a filter slider drag reaches this loop only if it also
        // painted — the LUT itself never re-walks a tile.
        //
        // Whether the tile had to be baked travels back with its levels. The upload loop needs
        // that answer to decide if the tile may share an atlas slot with its siblings, and
        // re-deriving it there meant compositing every dirty tile a second time, sequentially,
        // on the frame thread — exactly doubling the cost of the one path that already
        // dominates a heavy frame.
        // The baked base level is only carried when there *was* something to bake. Otherwise it
        // stays `None` and the upload reads the tile's own `Arc` where it already lives, which
        // is also what tells the loop below the tile may share an atlas slot with its siblings.
        let payloads: Vec<Option<TilePayload>> = uploads
            .par_iter()
            .map(|(layer_index, coord, _, skip_mips)| {
                let layer = doc.layers.get(*layer_index)?;
                let pixels = layer.tiles()?.get(*coord)?;
                let composited = composited_tile_payload(pixels, *coord, layer, doc_width);
                let base: &[u8] = composited.as_deref().unwrap_or(pixels.as_slice());
                let mips = tile_upload_mips(base, *skip_mips);
                Some((composited, mips))
            })
            .collect();

        // Tiles retained only as prefetch margin (in `live`, but not currently on screen) are
        // the ones sacrificed first when the atlas is full — see the fallback inside the
        // upload loop below.
        let mut evictable: Vec<TileKey> = self
            .tiles
            .keys()
            .filter(|k| live.contains(*k) && !visible_keys.contains(*k))
            .copied()
            .collect();

        let mut shared_gpu: HashMap<(usize, usize), u32> = HashMap::new();
        // Only tiles that actually reached the atlas may be marked clean at the end. An upload
        // the atlas had no room for has to stay dirty, or it is skipped by `build_layer_draws`
        // (no slot) and never retried (not dirty) — a permanent hole in the layer, showing
        // through as bare paper until something happens to dirty that tile again.
        let mut uploaded: Vec<(usize, TileCoord)> = Vec::with_capacity(uploads.len());

        for ((layer_index, coord, key, skip_mips), payload) in uploads.iter().zip(payloads.iter()) {
            let key = *key;
            let skip_mips = *skip_mips;
            let Some((composited, mips)) = payload else {
                continue;
            };
            let baked = composited.is_some();
            let layer = &doc.layers[*layer_index];
            let Some(pixels) = layer.tiles().and_then(|g| g.get(*coord)) else {
                continue;
            };
            let base: &[u8] = composited.as_deref().unwrap_or(pixels.as_slice());
            if !baked {
                let ptr = Arc::as_ptr(pixels) as usize;
                if let Some(&array_layer) = shared_gpu.get(&(*layer_index, ptr)) {
                    self.tiles.insert(key, GpuTile { array_layer });
                    self.note_mip_state(key, skip_mips);
                    uploaded.push((*layer_index, *coord));
                    continue;
                }
            }

            if let Some(existing) = self.tiles.get(&key) {
                let slot = existing.array_layer;
                self.atlas.write(&self.queue, slot, base, mips);
                if !baked {
                    let ptr = Arc::as_ptr(pixels) as usize;
                    shared_gpu.insert((*layer_index, ptr), slot);
                }
                self.note_mip_state(key, skip_mips);
                uploaded.push((*layer_index, *coord));
                continue;
            }

            let shared = SharedBindings {
                layout: &self.tile_shared_bgl,
                camera: &self.tile_camera_buf,
                layers: &self.layer_data_buf,
                samplers: &self.samplers,
            };
            let array_layer = match self.atlas.allocate(&self.device, &self.queue, &shared) {
                Some(slot) => slot,
                None => {
                    let victim = evictable
                        .pop()
                        .or_else(|| live.iter().copied().find(|key| !visible_keys.contains(key)));
                    let Some(victim) = victim else {
                        continue;
                    };
                    if let Some(freed) = self.tiles.remove(&victim) {
                        self.atlas.free(freed.array_layer);
                    }
                    let Some(slot) = self.atlas.allocate(&self.device, &self.queue, &shared) else {
                        continue;
                    };
                    slot
                }
            };
            self.atlas.write(&self.queue, array_layer, base, mips);
            self.tiles.insert(key, GpuTile { array_layer });
            if !baked {
                let ptr = Arc::as_ptr(pixels) as usize;
                shared_gpu.insert((*layer_index, ptr), array_layer);
            }
            self.note_mip_state(key, skip_mips);
            uploaded.push((*layer_index, *coord));
        }

        // Anything no longer live (scrolled entirely out of the retention margin, or its
        // layer was removed) frees its atlas slot for reuse. Tiles evicted above under
        // capacity pressure are already gone from `self.tiles`, so this does not double-free.
        let dropped: Vec<u32> = self
            .tiles
            .iter()
            .filter(|(k, _)| !live.contains(*k))
            .map(|(_, gpu)| gpu.array_layer)
            .collect();
        for slot in dropped {
            self.atlas.free(slot);
        }
        self.tiles.retain(|k, _| live.contains(k));
        self.base_only_tiles.retain(|k| live.contains(k));
        let live_layers: FxHashSet<&str> = doc.layers.iter().map(|l| l.id.as_str()).collect();
        self.layer_slots
            .retain(|id, _| live_layers.contains(id.as_str()));

        for (layer_index, coord) in uploaded {
            if let Some(grid) = doc.layers.get_mut(layer_index).and_then(|l| l.tiles_mut()) {
                grid.clear_dirty_tile(DirtyChannel::Render, coord);
            }
        }
        // Eviction under capacity pressure happens above, so this is the other half of the
        // memo's invalidation: residency changed, ask again next frame.
        self.visible_upload_needed = None;
    }

    /// The whole layer stack as one ordered draw list, filling the instance buffers as it
    /// goes. A document layer's visible tiles become one range in `tiles` — one instanced
    /// draw call regardless of how many tiles that is. A vector layer is one item, so it is
    /// one `LayerDraw::Vector` and nothing in the layer coalesces.
    pub(super) fn build_layer_draws(
        &mut self,
        doc: &Document,
        tiles: &mut Vec<TileInstance>,
        strokes: &mut Vec<StrokeInstance>,
        shapes: &mut Vec<VectorShapeInstance>,
    ) -> Vec<LayerDraw> {
        let Some(visible) = doc.visible_rect() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (layer_index, layer) in doc.layers.iter().enumerate() {
            let layer_index = layer_index as u32;
            if !layer.visible {
                continue;
            }
            if let Some(item) = layer.content.item() {
                let placement = vector_placement(layer);
                if !item_visible(item, placement, visible) {
                    continue;
                }
                match item {
                    VectorItem::Shape(shape) => {
                        let start = shapes.len() as u32;
                        shapes.push(shape_instance(shape, placement));
                        out.push(LayerDraw::Vector(
                            VectorRun::Shapes,
                            start..shapes.len() as u32,
                        ));
                    }
                    VectorItem::Path(path) => {
                        let start = strokes.len() as u32;
                        push_path_instances(path, placement, strokes);
                        if strokes.len() as u32 > start {
                            out.push(LayerDraw::Vector(
                                VectorRun::Paths,
                                start..strokes.len() as u32,
                            ));
                        }
                    }
                }
                continue;
            }
            let Some(grid) = layer.tiles() else {
                continue;
            };
            let Some(slot) = self.layer_slots.get(&layer.id).copied() else {
                continue;
            };
            if layer.is_paper() && grid.whole_tiles_share_one_arc() {
                // `write_layer_data` already put this layer's atlas slot in its table row; the
                // draw only has to name the row.
                if self.tiles.contains_key(&(slot, 0, 0)) {
                    out.push(LayerDraw::Solid(layer.blend_mode, layer_index));
                }
                continue;
            }
            let visible_grid = layer.doc_rect_to_grid(visible);
            let start = tiles.len() as u32;
            for coord in grid.coords_intersecting(visible_grid) {
                let key: TileKey = (slot, coord.x, coord.y);
                let Some(gpu) = self.tiles.get(&key) else {
                    continue;
                };
                let (ox, oy) = coord.origin();
                tiles.push(TileInstance {
                    origin: [ox as f32, oy as f32],
                    slot: gpu.array_layer,
                    layer_index,
                });
            }
            if tiles.len() as u32 > start {
                out.push(LayerDraw::Tiles(
                    layer.blend_mode,
                    start..tiles.len() as u32,
                ));
            }
        }
        out
    }

    /// Replays `cached_draws` into whatever color attachment `pass` targets — the shared body
    /// behind both a full content redraw (the whole visible tile/vector set, into a fresh
    /// `PanCache` reference) and a blit-frame's exposed-strip repair (the same draws, scissored
    /// down to just the strip). Positions are document-space in the instance buffer, so the
    /// same buffers and draw calls reproduce correctly at any camera state — nothing here reads
    /// `doc` directly.
    pub(super) fn draw_cached_content<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        for draw in &self.cached_draws {
            match draw {
                LayerDraw::Tiles(mode, range) => {
                    pass.set_pipeline(self.tile_pipeline(*mode));
                    pass.set_bind_group(0, self.atlas.bind_group(), &[]);
                    pass.set_vertex_buffer(0, self.tile_instance_buf.slice(..));
                    pass.draw(0..6, range.clone());
                }
                LayerDraw::Solid(mode, layer_index) => {
                    pass.set_pipeline(self.solid_pipeline(*mode));
                    pass.set_bind_group(0, self.atlas.bind_group(), &[]);
                    // The instance range *is* the argument: `vs_doc_quad` reads its layer row
                    // from `instance_index`, so a one-instance draw at `layer_index` says which.
                    pass.draw(0..6, *layer_index..*layer_index + 1);
                }
                LayerDraw::Vector(kind, range) => {
                    let (pipeline, buf) = match kind {
                        VectorRun::Shapes => (&self.vector_shape_pipeline, &self.vector_shape_buf),
                        VectorRun::Paths => (&self.stroke_pipeline, &self.stroke_buf),
                    };
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(0, &self.preview_bg, &[]);
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw(0..6, range.clone());
                }
            }
        }
    }

    /// Draws the whole visible stack fresh into the `PanCache` reference texture, scissored to
    /// the current paper rect, and commits it as the new blit baseline. This is the "content
    /// pass" side of `ChunkDraw` in the plan's terms — a full redraw, just retargeted from the
    /// swapchain to an offscreen texture so a later camera-only frame has something to shift.
    pub(super) fn redraw_pan_cache_reference(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        pan: (f32, f32),
        zoom: f32,
        dpr: f32,
        scissor: PxRect,
    ) {
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pan-cache-full"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: self.pan_cache.reference_view(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                ..Default::default()
            });
            pass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
            self.draw_cached_content(&mut pass);
        }
        self.pan_cache.commit_reference(pan, zoom, dpr, scissor);
    }

    /// Copies the previous frame's content shifted by this frame's pan delta into the
    /// `PanCache` working texture, patches the strips the copy could not have populated
    /// (`framebuffer::exposed_rects`) by replaying `cached_draws` scissored to just those
    /// rects, then promotes the result to be the next frame's reference. Each strip is cleared
    /// to transparent first — `LoadOp::Load` preserves the freshly copied region, so without an
    /// explicit clear a semi-transparent stroke in the strip would blend against whatever this
    /// texture held two frames ago instead of nothing.
    ///
    /// The promotion at the end is what keeps the strips thin: measured against the previous
    /// frame the exposed band is one frame's worth of travel, a few pixels on a normal drag.
    /// Measured against a reference frozen at the last full redraw — the way this used to work
    /// — it grew with the whole gesture. See `PanCache`'s own note.
    pub(super) fn patch_pan_cache_working(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        plan: framebuffer::BlitPlan,
        dpr: f32,
        scissor: PxRect,
    ) {
        let framebuffer::BlitPlan { src, dst, shift } = plan;
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: self.pan_cache.reference_texture(),
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: src.0,
                    y: src.1,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: self.pan_cache.working_texture(),
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: dst.0,
                    y: dst.1,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: src.2,
                height: src.3,
                depth_or_array_layers: 1,
            },
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("pan-cache-patch"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: self.pan_cache.working_view(),
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            ..Default::default()
        });
        {
            for strip in framebuffer::exposed_rects(scissor, dst)
                .into_iter()
                .flatten()
            {
                pass.set_scissor_rect(strip.0, strip.1, strip.2, strip.3);
                pass.set_pipeline(self.pan_cache.clear_pipeline());
                pass.draw(0..3, 0..1);
                self.draw_cached_content(&mut pass);
            }
        }
        drop(pass);
        self.pan_cache.commit_shift(shift, dpr, scissor);
    }

    pub fn render(&mut self, doc: &mut Document) {
        // The caret is the whole of `has_animated_overlay`, and it is a square wave: parking the
        // cursor in a text layer used to run the full board pass at display rate to service a
        // signal that changes state twice a second. Comparing the phase against the frame that
        // was actually drawn turns that into two frames a second — and the same comparison
        // catches the caret going away, which needs one frame to erase it.
        let caret_phase = doc
            .has_animated_overlay()
            .then(|| text_caret_visible(self.started.elapsed().as_secs_f32()));
        if self.frame_dirty == FrameDirty::Clean
            && !doc.has_live_preview()
            && caret_phase == self.drawn_caret_phase
        {
            return;
        }

        let (dw, dh) = doc.camera.device_size();
        self.resize(dw, dh);
        self.pan_cache
            .resize(&self.device, self.config.width, self.config.height);

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

        let preview_shape = doc.preview_shape();
        let ink = doc.ink_rgba();
        let color = [
            ink[0] as f32 / 255.0,
            ink[1] as f32 / 255.0,
            ink[2] as f32 / 255.0,
            ink[3] as f32 / 255.0,
        ];
        let (p0, p1, tool, half_width, fill, shape_stroke, shape_color, shape_stroke_color) =
            match preview_shape {
                Some(s) => {
                    let (fill_ink, stroke_ink) = doc.shape_paint(s.tool);
                    (
                        [s.start.0, s.start.1],
                        [s.end.0, s.end.1],
                        s.tool as u32 as f32,
                        s.half_width,
                        f32::from(u8::from(s.fill)),
                        f32::from(u8::from(s.stroke)),
                        rgba_unit(fill_ink),
                        rgba_unit(stroke_ink),
                    )
                }
                None => match selection_rect_or_ellipse(doc) {
                    // The marquee is an outline and nothing else, so it rides in on the stroke
                    // half of the same uniform the shape preview uses.
                    Some((p0, p1, sel_tool)) => (
                        p0,
                        p1,
                        sel_tool as u32 as f32,
                        SELECTION_OUTLINE_WIDTH,
                        0.0,
                        1.0,
                        SELECTION_OUTLINE_COLOR,
                        SELECTION_OUTLINE_COLOR,
                    ),
                    None => ([0.0, 0.0], [0.0, 0.0], 0.0, 0.0, 0.0, 0.0, color, color),
                },
            };
        // Written every frame, not only on the ones that build an overlay: the guide pass reads
        // the camera out of this buffer, and guides are board furniture that has to keep up with
        // a pan the overlay sits out.
        let preview = PreviewUniforms {
            pan: [doc.camera.pan_x, doc.camera.pan_y],
            zoom: doc.camera.zoom,
            dpr: doc.camera.dpr,
            viewport,
            _align_color: [0.0, 0.0],
            color: shape_color,
            p0,
            p1,
            half_width,
            tool,
            fill,
            shape_stroke,
            stroke_ink: rgba_unit(doc.stroke_ink()),
            shape_stroke_color,
        };
        self.queue
            .write_buffer(&self.preview_buf, 0, bytemuck::bytes_of(&preview));

        let guide_count = self.write_guides(doc);
        let mut overlay_range = 0u32..0u32;
        let mut screen_overlay_range = 0u32..0u32;
        // `brush_range` is the segments to *union into* the coverage target this frame, which is
        // empty on any frame the pointer did not move; `brush_active` is whether there is a live
        // brush stroke to composite onto the board at all. They used to be the same question,
        // because the target was rebuilt from the first point every frame.
        let mut brush_range = 0u32..0u32;
        let mut brush_active = false;
        let mut brush_restart = false;
        if !camera_only {
            let radius = doc.effective_brush_size() * 0.5;
            let stroke_color = if doc.tool == Tool::Eraser {
                ERASER_PREVIEW_COLOR
            } else {
                color
            };
            let mut brush_instances: Vec<StrokeInstance> = Vec::new();
            // The stroke buffer is a vector-path prefix (owned by `cached_draws`' ranges)
            // followed by this frame's overlay. Only the overlay changes on an overlay frame,
            // so it is built into a reused scratch buffer and written at the prefix's offset —
            // cloning `cached_strokes` every frame just to rewrite a suffix put a full copy of
            // every vector path in the document on the hot path.
            //
            // The overlay itself splits in two, by which pass measures it: ink-shaped previews
            // stay in document units on `stroke_pipeline`, while chrome — the transform and
            // item frames, the text session's box and caret, the hover outline — is measured in
            // screen pixels on `overlay_pipeline`. Both are contiguous ranges of the same
            // buffer, so the split costs a second `draw`, not a second upload.
            let prefix_len = if self.camera_motion {
                0
            } else {
                self.cached_strokes.len()
            };
            let mut instances = std::mem::take(&mut self.overlay_scratch);
            instances.clear();
            let mut screen_instances = std::mem::take(&mut self.screen_overlay_scratch);
            screen_instances.clear();
            let overlay_start = prefix_len as u32;
            if doc.text_editing() {
                screen_instances.extend(text_overlay_instances(
                    doc,
                    self.started.elapsed().as_secs_f32(),
                ));
            } else if doc.tool == Tool::Crop {
                screen_instances.extend(crop_overlay_instances(doc));
            } else if doc.previews_brush_stroke() {
                brush_active = true;
                let profile = doc.active_brush_profile();
                // Ahead of the append decision rather than after the ranges are built: a target
                // the surface resize just recreated is empty, and only `ensure` knows that.
                let recreated = self.stroke_coverage.ensure(
                    &self.device,
                    self.config.width,
                    self.config.height,
                );
                let progress = CoverageProgress {
                    generation: doc.stroke_generation(),
                    points: doc.stroke_points.len(),
                    pan,
                    zoom: doc.camera.zoom,
                    dpr: doc.camera.dpr,
                    brush: brush_params(radius, &profile),
                    color: stroke_color,
                };
                let first_segment = match self.coverage_progress {
                    Some(prev) if !recreated && prev.appendable(&progress) => {
                        stroke_segment_count(prev.points)
                    }
                    _ => 0,
                };
                brush_restart = first_segment == 0;
                brush_instances = stroke_instances_from(
                    &doc.stroke_points,
                    first_segment,
                    radius,
                    stroke_color,
                    &profile,
                );
                self.coverage_progress = Some(progress);
            } else if !doc.stroke_points.is_empty() && doc.tool.previews_stroke() {
                instances.extend(stroke_instances(
                    &doc.stroke_points,
                    radius,
                    stroke_color,
                    &BrushProfile::HARD,
                ));
            } else if let Some(handles) = doc.transform_handles() {
                screen_instances.extend(transform_overlay_instances(handles));
            } else if let Some(points) = selection_lasso_points(doc) {
                instances.extend(stroke_instances(
                    &points,
                    SELECTION_OUTLINE_WIDTH,
                    SELECTION_OUTLINE_COLOR,
                    &BrushProfile::HARD,
                ));
            } else if let Some(edges) =
                selection_mask_edges(doc, SELECTION_OUTLINE_WIDTH, SELECTION_OUTLINE_COLOR)
            {
                instances.extend(edges);
            }
            // Not part of the chain above: a selected item's frame is drawn under the Move
            // tool too, where none of those branches is the one that ran. It costs nothing
            // when nothing is selected, and `transform_handles` stands the layer frame down
            // while it is on screen, so the two can never both draw.
            screen_instances.extend(vector_selection_instances(doc));
            // Unconditional for the same reason: the engine decides whether there is a brush
            // cursor to draw, and answers with nothing when there is not.
            screen_instances.extend(brush_ring_instances(doc));
            screen_instances.extend(clone_source_overlay_instances(doc));
            for (index, corners) in doc.layer_highlights() {
                let covered = doc
                    .transform_handles()
                    .is_some_and(|(handle_index, _, _)| handle_index == index);
                if !covered {
                    screen_instances.extend(layer_highlight_instances(
                        corners,
                        self.started.elapsed().as_secs_f32(),
                        doc.camera.zoom,
                    ));
                }
            }
            overlay_range = overlay_start..overlay_start + instances.len() as u32;
            let screen_start = overlay_range.end;
            instances.append(&mut screen_instances);
            screen_overlay_range = screen_start..prefix_len as u32 + instances.len() as u32;
            let brush_start = screen_overlay_range.end;
            instances.append(&mut brush_instances);
            brush_range = brush_start..prefix_len as u32 + instances.len() as u32;
            let total = prefix_len + instances.len();
            let grew = self.ensure_stroke_capacity(total);
            let stride = std::mem::size_of::<StrokeInstance>() as u64;
            if (grew || need_draw_rebuild) && prefix_len > 0 {
                self.queue.write_buffer(
                    &self.stroke_buf,
                    0,
                    bytemuck::cast_slice(&self.cached_strokes),
                );
            }
            if !instances.is_empty() {
                self.queue.write_buffer(
                    &self.stroke_buf,
                    prefix_len as u64 * stride,
                    bytemuck::cast_slice(&instances),
                );
            }
            self.overlay_scratch = instances;
            self.screen_overlay_scratch = screen_instances;
            if need_draw_rebuild && !self.cached_shapes.is_empty() {
                self.ensure_vector_shape_capacity(self.cached_shapes.len());
                self.queue.write_buffer(
                    &self.vector_shape_buf,
                    0,
                    bytemuck::cast_slice(&self.cached_shapes),
                );
            }
        }

        if need_draw_rebuild && !self.cached_tile_instances.is_empty() {
            self.ensure_tile_instance_capacity(self.cached_tile_instances.len());
            self.queue.write_buffer(
                &self.tile_instance_buf,
                0,
                bytemuck::cast_slice(&self.cached_tile_instances),
            );
        }

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

        if brush_active {
            // `accumulate` no-ops on an empty range that is not a restart, which is the frame
            // where the pointer has not moved far enough to add a segment — the target already
            // holds the whole stroke and the board pass below still composites it.
            self.stroke_coverage.accumulate(
                &mut encoder,
                &self.preview_bg,
                &self.stroke_buf,
                brush_range.clone(),
                scissor,
                brush_restart,
            );
        }

        // One render pass for the whole frame. These four stages used to be four separate
        // passes chained with LoadOp::Load — correct, but every begin/end pair is a real
        // boundary on tile-based GPUs (Apple Silicon among them), forcing a tile-memory
        // flush each time. They draw into the same attachment in the same order regardless,
        // so a single pass produces an identical image for a fraction of the pass overhead.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("board"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                ..Default::default()
            });

            pass.set_pipeline(&self.paper_pipeline);
            pass.set_bind_group(0, &self.paper_bg, &[]);
            pass.draw(0..3, 0..1);

            if let Some((x, y, w, h)) = scissor {
                pass.set_scissor_rect(x, y, w, h);

                if use_overview {
                    self.overview.draw(&mut pass);
                } else if has_content {
                    pass.set_pipeline(self.pan_cache.blit_pipeline());
                    pass.set_bind_group(0, self.pan_cache.bind_group(), &[]);
                    pass.draw(0..3, 0..1);
                }
            }

            // Over the artwork, under the transform box and the marching ants: a guide is
            // something the picture is aligned against, not something drawn on it. It is the one
            // pass *outside* the paper scissor, because a guide is measured against the view —
            // it runs edge to edge, meeting the ruler it was pulled from, rather than stopping
            // where the paper does (`guide_instances`). Drawn even with the paper fully off
            // screen, which is why it does not sit inside the `if let` either.
            if guide_count > 0 {
                pass.set_scissor_rect(0, 0, self.config.width, self.config.height);
                pass.set_pipeline(&self.guide_pipeline);
                pass.set_bind_group(0, &self.preview_bg, &[]);
                pass.set_vertex_buffer(0, self.guide_buf.slice(..));
                pass.draw(0..6, 0..guide_count);
            }

            if let Some((x, y, w, h)) = scissor {
                pass.set_scissor_rect(x, y, w, h);

                if !overlay_range.is_empty() {
                    pass.set_pipeline(&self.stroke_pipeline);
                    pass.set_bind_group(0, &self.preview_bg, &[]);
                    pass.set_vertex_buffer(0, self.stroke_buf.slice(..));
                    pass.draw(0..6, overlay_range.clone());
                }

                if !screen_overlay_range.is_empty() {
                    pass.set_pipeline(&self.overlay_pipeline);
                    pass.set_bind_group(0, &self.preview_bg, &[]);
                    pass.set_vertex_buffer(0, self.stroke_buf.slice(..));
                    pass.draw(0..6, screen_overlay_range.clone());
                }

                if preview_shape.is_some() {
                    pass.set_pipeline(&self.shape_pipeline);
                    pass.set_bind_group(0, &self.preview_bg, &[]);
                    pass.draw(0..3, 0..1);
                }
            }

            // The brush ring is a cursor, so it goes on top of everything and — like the guides —
            // outside the paper scissor. `Document::brush_ring` has already decided there is a
            // stamp to promise, and on a pasted layer that overflows the paper that stamp can
            // land out over the desk; clipping the ring to the paper drew nothing there while
            // the shell had already hidden its own cursor, so the pointer disappeared.
            if brush_active {
                pass.set_scissor_rect(0, 0, self.config.width, self.config.height);
                self.stroke_coverage.composite(&mut pass, &self.preview_bg);
            }
        }

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
        // early-out unreachable for as long as a text session was open.
        self.frame_dirty = if doc.has_live_preview() {
            FrameDirty::Overlay
        } else {
            FrameDirty::Clean
        };
        // Recorded here rather than at the top, so a frame abandoned on a lost surface leaves the
        // caret asking to be drawn instead of counting as drawn.
        self.drawn_caret_phase = caret_phase;
    }
}

#[cfg(test)]
mod headless_tests {
    use super::*;
    use calumma_core::{BlendMode, Brush, Document, MemoryPressureLevel, Tool};

    fn doc(w: u32, h: u32) -> Document {
        let mut doc = Document::new("p".into(), "t", w, h);
        doc.resize_viewport(256.0, 256.0, 1.0);
        doc.fit_to_view();
        doc
    }

    fn renderer() -> Option<Renderer> {
        Renderer::new_headless(256, 256)
    }

    #[test]
    fn headless_renderer_draws_an_empty_board() {
        let Some(mut r) = renderer() else {
            return;
        };
        let mut doc = doc(128, 128);
        r.render(&mut doc);
        assert!(r.cached_tile_count() <= 2);
    }

    #[test]
    fn headless_renderer_uploads_painted_tiles() {
        let Some(mut r) = renderer() else {
            return;
        };
        let mut doc = doc(128, 128);
        doc.tool = Tool::Pen;
        doc.pointer_down(20.0, 20.0);
        doc.pointer_move(40.0, 40.0);
        doc.pointer_up(40.0, 40.0);
        r.invalidate();
        r.render(&mut doc);
        assert!(r.cached_tile_count() > 0);
        assert!(r.gpu_tile_bytes() > 0);
    }

    #[test]
    fn headless_renderer_follows_camera_motion_and_pressure() {
        let Some(mut r) = renderer() else {
            return;
        };
        let mut doc = doc(256, 256);
        r.render(&mut doc);
        r.begin_camera_motion();
        doc.camera.pan_x += 32.0;
        r.invalidate_camera();
        r.render(&mut doc);
        r.end_camera_motion();
        r.set_memory_pressure(MemoryPressureLevel::Critical);
        r.request_overview_prewarm();
        let _hint = r.frame_hint(&doc);
        r.release_document();
        assert_eq!(r.cached_tile_count(), 0);
    }

    #[test]
    fn headless_renderer_draws_vectors_guides_and_previews() {
        let Some(mut r) = renderer() else {
            return;
        };
        let mut doc = doc(256, 256);
        doc.add_guide(calumma_core::GuideAxis::Horizontal, 40.0);
        doc.set_vector_mode(true);
        doc.tool = Tool::Rect;
        doc.pointer_down(30.0, 30.0);
        doc.pointer_move(90.0, 90.0);
        r.invalidate_overlay();
        r.render(&mut doc);
        doc.pointer_up(90.0, 90.0);
        r.invalidate();
        r.render(&mut doc);

        doc.tool = Tool::Pen;
        doc.brush = Brush::Airbrush;
        doc.pointer_down(10.0, 10.0);
        doc.pointer_move(50.0, 50.0);
        r.invalidate_overlay();
        r.render(&mut doc);

        doc.tool = Tool::Eraser;
        doc.pointer_move(55.0, 55.0);
        r.render(&mut doc);
    }

    #[test]
    fn headless_renderer_handles_blend_modes_and_layer_props() {
        let Some(mut r) = renderer() else {
            return;
        };
        let mut doc = doc(128, 128);
        let layer = doc.active_layer;
        doc.layers[layer].blend_mode = BlendMode::Multiply;
        doc.layers[layer].opacity = 0.5;
        doc.layers[layer].adjustments = Some(calumma_core::Adjustments {
            brightness: 0.1,
            contrast: 0.0,
            vibrance: 0.0,
            saturation: 0.0,
            levels_gamma: 1.0,
        });
        doc.tool = Tool::Pen;
        doc.pointer_down(8.0, 8.0);
        doc.pointer_move(40.0, 40.0);
        doc.pointer_up(40.0, 40.0);
        r.invalidate();
        r.render(&mut doc);
        r.resize(320, 240);
        r.invalidate_overlay();
        r.render(&mut doc);
    }

    /// Three layers, each painted across a fifth of the visible tiles (well under the 48-tile
    /// enter threshold on its own) but summing past it together. The old sum-across-layers gate
    /// would have entered the overview here; the busiest single layer never asked for one.
    #[test]
    fn the_overview_gate_reads_the_busiest_layer_not_the_stack_total() {
        use calumma_core::limits::OVERVIEW_ENTER_TILE_THRESHOLD;
        use calumma_core::tile::TILE_SIZE;

        let Some(r) = renderer() else {
            return;
        };
        let mut d = doc(2048, 2048);
        for name in ["A", "B", "C"] {
            d.add_layer(name);
            let i = d.layers.len() - 1;
            let grid = d.layers[i].tiles_mut().unwrap();
            for ty in 0..4 {
                for tx in 0..5 {
                    grid.set_pixel(
                        (tx * TILE_SIZE) as i32,
                        (ty * TILE_SIZE) as i32,
                        [1, 2, 3, 255],
                    );
                }
            }
        }

        let busiest = r.busiest_layer_tile_count(&d);
        assert_eq!(busiest, 20, "one layer's own 4x5 painted block: {busiest}");
        assert!(
            busiest < OVERVIEW_ENTER_TILE_THRESHOLD,
            "no single layer alone needs the overview: {busiest}"
        );

        let summed_across_layers = busiest * 3;
        assert!(
            summed_across_layers >= OVERVIEW_ENTER_TILE_THRESHOLD,
            "the old sum-across-layers count would have crossed the threshold here \
             ({summed_across_layers}) even though the fix must not"
        );
    }

    #[test]
    fn headless_renderer_zooms_out_into_overview() {
        let Some(mut r) = renderer() else {
            return;
        };
        let mut doc = doc(2048, 2048);
        doc.camera.zoom = 0.05;
        doc.tool = Tool::Pen;
        doc.pointer_down(100.0, 100.0);
        doc.pointer_move(200.0, 200.0);
        doc.pointer_up(200.0, 200.0);
        r.invalidate();
        r.render(&mut doc);
        doc.camera.zoom = 0.01;
        r.invalidate_camera();
        r.render(&mut doc);
        doc.pointer_down(400.0, 400.0);
        doc.pointer_move(500.0, 480.0);
        doc.pointer_up(500.0, 480.0);
        r.invalidate();
        r.render(&mut doc);
    }

    #[test]
    fn headless_renderer_pushes_a_vector_item_layer() {
        let Some(mut r) = renderer() else {
            return;
        };
        let mut doc = doc(256, 256);
        doc.set_vector_mode(true);
        doc.tool = Tool::Rect;
        doc.pointer_down(20.0, 20.0);
        doc.pointer_move(80.0, 80.0);
        doc.pointer_up(80.0, 80.0);
        r.invalidate();
        r.render(&mut doc);
    }

    #[test]
    fn headless_renderer_text_caret_and_selection_overlay() {
        let Some(mut r) = renderer() else {
            return;
        };
        let mut doc = doc(256, 256);
        doc.tool = Tool::Text;
        doc.pointer_down(40.0, 40.0);
        doc.text_insert("hello");
        r.invalidate_overlay();
        r.render(&mut doc);
        r.render(&mut doc);

        doc.tool = Tool::SelectRect;
        doc.pointer_down(60.0, 60.0);
        doc.pointer_move(120.0, 120.0);
        r.invalidate_overlay();
        r.render(&mut doc);
    }

    #[test]
    fn headless_renderer_clone_and_transform_overlays() {
        let Some(mut r) = renderer() else {
            return;
        };
        let mut doc = doc(256, 256);
        doc.tool = Tool::Pen;
        doc.pointer_down(30.0, 30.0);
        doc.pointer_move(80.0, 80.0);
        doc.pointer_up(80.0, 80.0);
        doc.tool = Tool::Clone;
        doc.set_clone_anchor(40.0, 50.0);
        let (sx, sy) = doc.camera.to_screen(80.0, 60.0);
        doc.set_pointer_hover(sx, sy);
        r.invalidate_overlay();
        r.render(&mut doc);

        doc.enter_transform();
        r.invalidate_overlay();
        r.render(&mut doc);
    }

    #[test]
    fn headless_renderer_dark_theme_and_shape_preview() {
        let Some(mut r) = renderer() else {
            return;
        };
        let mut doc = doc(256, 256);
        doc.dark_theme = true;
        doc.fill = true;
        doc.stroke = true;
        doc.tool = Tool::Ellipse;
        doc.pointer_down(40.0, 40.0);
        doc.pointer_move(120.0, 120.0);
        r.invalidate_overlay();
        r.render(&mut doc);
        doc.pointer_up(120.0, 120.0);
        r.invalidate();
        r.render(&mut doc);
    }

    #[test]
    fn headless_renderer_lasso_selection_and_pan_cache() {
        let Some(mut r) = renderer() else {
            return;
        };
        let mut doc = doc(512, 512);
        doc.camera.zoom = 2.0;
        doc.tool = Tool::Pen;
        doc.pointer_down(40.0, 40.0);
        doc.pointer_move(100.0, 100.0);
        doc.pointer_up(100.0, 100.0);
        r.invalidate();
        r.render(&mut doc);

        r.begin_camera_motion();
        doc.camera.pan_x += 48.0;
        r.invalidate_camera();
        r.render(&mut doc);
        r.end_camera_motion();

        doc.tool = Tool::SelectLasso;
        doc.pointer_down(60.0, 60.0);
        doc.pointer_move(80.0, 90.0);
        doc.pointer_move(110.0, 70.0);
        r.invalidate_overlay();
        r.render(&mut doc);
    }

    #[test]
    fn headless_renderer_heal_and_fill_tools() {
        let Some(mut r) = renderer() else {
            return;
        };
        let mut doc = doc(256, 256);
        doc.tool = Tool::Pen;
        doc.pointer_down(20.0, 20.0);
        doc.pointer_move(60.0, 60.0);
        doc.pointer_up(60.0, 60.0);
        doc.tool = Tool::Heal;
        doc.set_clone_anchor(30.0, 30.0);
        doc.pointer_down(80.0, 80.0);
        doc.pointer_move(100.0, 100.0);
        r.invalidate_overlay();
        r.render(&mut doc);
        doc.pointer_up(100.0, 100.0);
        r.invalidate();
        r.render(&mut doc);

        doc.tool = Tool::Fill;
        doc.pointer_down(50.0, 50.0);
        r.render(&mut doc);
    }

    #[test]
    fn headless_renderer_stacked_layers_and_screen_blend() {
        let Some(mut r) = renderer() else {
            return;
        };
        let mut doc = doc(256, 256);
        doc.tool = Tool::Pen;
        doc.pointer_down(10.0, 10.0);
        doc.pointer_move(80.0, 80.0);
        doc.pointer_up(80.0, 80.0);
        doc.add_layer("Ink");
        doc.layers[doc.active_layer].blend_mode = BlendMode::Screen;
        doc.tool = Tool::Pen;
        doc.pointer_down(30.0, 30.0);
        doc.pointer_move(90.0, 90.0);
        doc.pointer_up(90.0, 90.0);
        r.invalidate();
        r.render(&mut doc);
    }
}
