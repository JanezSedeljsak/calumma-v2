use super::*;

impl Renderer {
    pub(super) fn clear_layer_cache(&mut self) {
        self.cached_retained_span = None;
        self.cached_visible_span = None;
        self.cached_tile_instances.clear();
        self.cached_strokes.clear();
        self.cached_shapes.clear();
        self.cached_draws.clear();
    }

    pub(super) fn rebuild_layer_cache(&mut self, doc: &Document) {
        let mut tile_instances = Vec::new();
        let mut strokes = Vec::new();
        let mut shapes = Vec::new();
        let draws = self.build_layer_draws(doc, &mut tile_instances, &mut strokes, &mut shapes);
        self.cached_tile_instances = tile_instances;
        self.cached_strokes = strokes;
        self.cached_shapes = shapes;
        self.cached_draws = draws;
        self.cached_retained_span = Self::retained_span(doc, self.budget.retention_margin_tiles());
        self.cached_visible_span = Self::visible_span(doc);
    }

    /// [`Self::visible_needs_gpu_upload`], answered from the previous frame where that answer
    /// cannot have moved. Cleared by [`Self::invalidate`] and by `sync_tiles` — between them
    /// those are the only ways a visible tile stops being resident.
    pub(super) fn visible_upload_needed(&mut self, doc: &Document) -> bool {
        if let Some(cached) = self.visible_upload_needed {
            return cached;
        }
        let needed = self.visible_needs_gpu_upload(doc);
        self.visible_upload_needed = Some(needed);
        needed
    }

    pub(super) fn visible_needs_gpu_upload(&self, doc: &Document) -> bool {
        let Some(visible) = doc.visible_rect() else {
            return false;
        };
        for layer in &doc.layers {
            if !layer.visible {
                continue;
            }
            let Some(grid) = layer.tiles() else {
                continue;
            };
            let Some(slot) = self.layer_slots.get(&layer.id) else {
                return true;
            };
            if layer.is_paper() {
                if layer.tiles().is_some_and(|g| g.whole_tiles_share_one_arc()) {
                    let key: TileKey = (*slot, 0, 0);
                    if !self.tiles.contains_key(&key) {
                        return true;
                    }
                }
                continue;
            }
            for coord in grid.coords_intersecting(layer.doc_rect_to_grid(visible)) {
                let key: TileKey = (*slot, coord.x, coord.y);
                if !self.tiles.contains_key(&key) {
                    return true;
                }
            }
        }
        false
    }

    /// The overview gate's input: the *busiest single layer's* visible tile count, not the
    /// stack total. A document with ten sparse layers and a document with one layer painted
    /// edge to edge can need the same number of GPU draws, but only the second is actually too
    /// busy to draw as tiles — summing across layers charged the first for tiles that were
    /// never going to be a problem on their own, and a document with enough *layers* could
    /// never leave the overview even though each one individually was well under the tile
    /// path's budget. Comparing this against `OVERVIEW_ENTER_TILE_THRESHOLD`/
    /// `OVERVIEW_EXIT_TILE_THRESHOLD` (`overview.rs::should_use`) is exactly "does some layer
    /// alone cross the threshold", which a max reduces to directly.
    pub(super) fn busiest_layer_tile_count(&self, doc: &Document) -> usize {
        let Some(visible) = doc.visible_rect() else {
            return 0;
        };
        let mut busiest = 0;
        for layer in &doc.layers {
            if !layer.visible {
                continue;
            }
            let Some(grid) = layer.tiles() else {
                continue;
            };
            let count = if layer.is_paper() && grid.whole_tiles_share_one_arc() {
                1
            } else {
                grid.coords_intersecting(layer.doc_rect_to_grid(visible))
                    .count()
            };
            busiest = busiest.max(count);
        }
        busiest
    }

    pub(super) fn layer_slot(&mut self, layer_id: &str) -> u32 {
        if let Some(slot) = self.layer_slots.get(layer_id) {
            return *slot;
        }
        let slot = self.next_layer_slot;
        self.next_layer_slot += 1;
        self.layer_slots.insert(layer_id.to_string(), slot);
        slot
    }

    /// Grows the layer table to hold `count` rows, rebinding group 0 if the buffer had to be
    /// replaced. Doubling, like the instance buffers, so a document that keeps gaining layers
    /// does not reallocate on every one.
    pub(super) fn ensure_layer_data_capacity(&mut self, count: usize) {
        if count <= self.layer_data_capacity {
            return;
        }
        let mut next = self.layer_data_capacity.max(1);
        while next < count {
            next *= 2;
        }
        self.layer_data_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("layer-data"),
            size: (next * std::mem::size_of::<LayerData>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.layer_data_capacity = next;
        // The old buffer is still held by the atlas's bind group, which would keep reading it.
        self.atlas.rebuild_bind_group(
            &self.device,
            &SharedBindings {
                layout: &self.tile_shared_bgl,
                camera: &self.tile_camera_buf,
                layers: &self.layer_data_buf,
                samplers: &self.samplers,
            },
        );
    }

    /// Writes one table row per document layer, in stack order, as one buffer write.
    ///
    /// Every layer gets a row — vector layers and hidden ones included — so that a row index is
    /// simply a stack position and never has to be mapped through a side table. An unread row
    /// costs 1072 bytes; an index that means different things in different frames costs
    /// correctness, which is what the old per-layer-id lookup was quietly risking.
    ///
    /// Must run after `sync_tiles`: solid Paper's `atlas_slot` is only known once its tile has
    /// an atlas slot. Runs whenever the draw list is rebuilt, which is exactly when a transform,
    /// the stack, the visible span, opacity or an adjustment can have changed — a slider drag
    /// reaches this the same way a `⌘T` drag already does, by calling `Renderer::invalidate`.
    pub(super) fn write_layer_data(&mut self, doc: &Document) {
        self.ensure_layer_data_capacity(doc.layers.len().max(1));
        let mut rows = std::mem::take(&mut self.layer_data_scratch);
        rows.clear();
        rows.reserve(doc.layers.len());
        for layer in &doc.layers {
            let mut row = match (layer.transform, layer.content_bounds()) {
                (Some(t), Some(bounds)) => LayerData {
                    pivot: [(bounds.0 + bounds.2) * 0.5, (bounds.1 + bounds.3) * 0.5],
                    offset: [t.offset_x, t.offset_y],
                    scale: [t.scale_x, t.scale_y],
                    rotation: t.rotation,
                    ..LayerData::default()
                },
                _ => LayerData::default(),
            };
            row.opacity = layer.opacity;
            if let Some(adjustments) = layer.adjustments {
                // `Document::set_layer_adjustments` already clears this to `None` for a neutral
                // result, but a fresh `AdjustmentLut` re-checks: cheaper than trusting a state
                // no type here enforces, and `is_neutral` is one struct-field compare.
                let lut = adjustments.lut();
                if !lut.is_neutral() {
                    row.tone = *lut.tone_table();
                    if lut.is_tone_only() {
                        row.lut_mode = LUT_MODE_TONE;
                    } else {
                        row.lut_mode = LUT_MODE_TONE_HSL;
                        row.saturation = adjustments.saturation;
                        row.vibrance = adjustments.vibrance;
                    }
                }
            }
            if let Some(slot) = self.solid_atlas_slot(layer) {
                row.atlas_slot = slot;
            }
            rows.push(row);
        }
        if !rows.is_empty() {
            self.queue
                .write_buffer(&self.layer_data_buf, 0, bytemuck::cast_slice(&rows));
        }
        self.layer_data_scratch = rows;
    }

    /// The atlas slot behind a Paper layer that has collapsed to one shared tile, or `None` for
    /// every layer that draws its tiles the ordinary way.
    pub(super) fn solid_atlas_slot(&self, layer: &calumma_core::Layer) -> Option<u32> {
        if !layer.is_paper() {
            return None;
        }
        let grid = layer.tiles()?;
        if !grid.whole_tiles_share_one_arc() {
            return None;
        }
        let slot = *self.layer_slots.get(&layer.id)?;
        self.tiles.get(&(slot, 0, 0)).map(|gpu| gpu.array_layer)
    }

    pub fn cached_tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// GPU-side bytes reserved for the open document's tiles: the atlas's whole declared
    /// capacity (mip chain included), not just the tiles currently written — a `wgpu::Texture`
    /// array reserves storage for every layer it declares regardless of how many are in use, so
    /// capacity is the number that actually reflects VRAM pressure.
    pub fn gpu_tile_bytes(&self) -> usize {
        self.atlas.capacity_bytes()
    }

    /// Forwards one OS memory-pressure report — the shell's only inbound knob for GPU
    /// residency, mirroring `DISPATCH_SOURCE_TYPE_MEMORYPRESSURE` on macOS (`Normal` / `Warn` /
    /// `Critical`). `PressureState` owns the hysteresis: escalating always applies on the very
    /// next report, while relaxing needs several consecutive reports at the lower level first,
    /// so a signal that oscillates doesn't thrash the retention margin every frame.
    ///
    /// Any effective-level change lowers the atlas's growth ceiling (or raises it back) and
    /// invalidates every cache keyed on the retention margin, which is what turns a narrower
    /// margin into eviction on the very next `sync_tiles` rather than only once the atlas
    /// happens to run out of room. Sustained `Critical` additionally recreates the atlas texture
    /// smaller — the one response expensive enough to reserve for pressure that has actually
    /// persisted rather than spiked once (`PressureState`'s shrink streak).
    pub fn set_memory_pressure(&mut self, level: MemoryPressureLevel) {
        let transition = self.budget.report_pressure(level);
        if !transition.effective_changed && !transition.shrink {
            return;
        }
        // Through the budget, never off the level directly: the device tier sets a floor under
        // the same two numbers, and a pressure report that recovered all the way to `Normal`
        // must not hand a weak GPU back the ceiling it never had.
        let capacity = self.budget.atlas_max_capacity();
        self.atlas.set_max_capacity(capacity);

        if transition.shrink {
            let shared = SharedBindings {
                layout: &self.tile_shared_bgl,
                camera: &self.tile_camera_buf,
                layers: &self.layer_data_buf,
                samplers: &self.samplers,
            };
            let remap = self
                .atlas
                .shrink_to(&self.device, &self.queue, &shared, capacity);
            for tile in self.tiles.values_mut() {
                if let Some(&new_layer) = remap.get(&tile.array_layer) {
                    tile.array_layer = new_layer;
                }
            }
        }

        self.invalidate();
    }

    /// Hand back everything that belonged to the document being closed — the atlas's slots and
    /// the per-layer uniform buffers keyed by its layer ids. Eviction otherwise only happens
    /// inside `sync_tiles`, which needs a document to run, so a closed project's textures
    /// would sit in VRAM until some *other* project was opened and drawn.
    pub fn release_document(&mut self) {
        self.tiles.clear();
        self.base_only_tiles.clear();
        self.atlas.clear();
        self.layer_slots.clear();
        self.next_layer_slot = 0;
        self.clear_layer_cache();
        self.overview.clear();
        self.pan_cache.invalidate();
        self.stroke_coverage.release();
        self.coverage_progress = None;
        self.visible_upload_needed = None;
        self.frame_dirty = FrameDirty::Content;
    }

    /// Whether this tile is sitting in the atlas with only its base level written and the
    /// camera has since settled, so there is now time to finish it.
    pub(super) fn needs_full_mips(&self, key: &TileKey) -> bool {
        !self.camera_motion && self.base_only_tiles.contains(key)
    }

    pub(super) fn note_mip_state(&mut self, key: TileKey, skipped_mips: bool) {
        if skipped_mips {
            self.base_only_tiles.insert(key);
        } else {
            self.base_only_tiles.remove(&key);
        }
    }

    /// Motion mode skips the mip chain to keep a gesture cheap, but that is only safe when the
    /// slot already holds a chain to fall back on. A tile reaching the atlas for the first time
    /// mid-gesture has nothing in its upper levels, so it pays for them even during motion —
    /// otherwise zooming out samples levels that were never written.
    pub(super) fn may_skip_mips(&self, key: &TileKey) -> bool {
        self.camera_motion && self.tiles.contains_key(key) && !self.base_only_tiles.contains(key)
    }
}

/// The layer table, exercised on a real device.
///
/// These build the tile and solid pipelines against the *same* `tile_shared_bgl` and the same
/// shader the app uses, so a disagreement between `LayerData` in Rust and `LayerData` in WGSL —
/// a field added on one side, a stride that stopped matching — fails here rather than showing up
/// as geometry in the wrong place on someone's board.
#[cfg(test)]
mod layer_table_tests {
    use super::*;
    use crate::renderer::pipeline::{premultiplied_target, tile_shared_bgl, TILE_INSTANCE_ATTRS};
    use crate::test_gpu::{gpu, read_texture_layer, Gpu};
    use calumma_core::filters::{AdjustmentLut, Adjustments};
    use calumma_core::tile::{TILE_BYTES, TILE_SIZE};

    const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
    const RED: [u8; 4] = [255, 0, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];

    struct Fixture {
        bgl: wgpu::BindGroupLayout,
        camera: wgpu::Buffer,
        layers: wgpu::Buffer,
        samplers: TileSamplers,
        atlas: TileAtlas,
        target: wgpu::Texture,
    }

    impl Fixture {
        /// A slot in the atlas holding one flat colour, mip chain included so the sampler's
        /// choice of level cannot change what the test reads back.
        fn solid_slot(&mut self, gpu: &Gpu, rgba: [u8; 4]) -> u32 {
            let shared = SharedBindings {
                layout: &self.bgl,
                camera: &self.camera,
                layers: &self.layers,
                samplers: &self.samplers,
            };
            let slot = self
                .atlas
                .allocate(&gpu.device, &gpu.queue, &shared)
                .expect("slot");
            let base = rgba.repeat(TILE_BYTES / 4);
            let mut mips = Vec::new();
            let mut side = TILE_SIZE / 2;
            while side >= 1 {
                mips.push(rgba.repeat((side * side) as usize));
                if side == 1 {
                    break;
                }
                side /= 2;
            }
            self.atlas.write(&gpu.queue, slot, &base, &mips);
            slot
        }

        fn write_rows(&self, gpu: &Gpu, rows: &[LayerData]) {
            gpu.queue
                .write_buffer(&self.layers, 0, bytemuck::cast_slice(rows));
        }
    }

    /// One tile's worth of board, drawn 1:1 into a `TILE_SIZE` target: document pixel *n* lands
    /// on target pixel *n*, so a readback coordinate is a document coordinate and the mip level
    /// is 0 everywhere.
    fn fixture(gpu: &Gpu) -> Fixture {
        let bgl = tile_shared_bgl(&gpu.device);
        let camera = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tile-camera"),
            size: std::mem::size_of::<TileCamera>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let side = TILE_SIZE as f32;
        gpu.queue.write_buffer(
            &camera,
            0,
            bytemuck::bytes_of(&TileCamera {
                pan: [0.0, 0.0],
                zoom: 1.0,
                dpr: 1.0,
                viewport: [side, side],
                doc_size: [side, side],
                crisp: 0.0,
                _pad: [0.0; 3],
            }),
        );
        let layers = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("layer-data"),
            size: (LAYER_DATA_CAPACITY * std::mem::size_of::<LayerData>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let samplers = TileSamplers::new(&gpu.device);
        let atlas = TileAtlas::new(
            &gpu.device,
            &SharedBindings {
                layout: &bgl,
                camera: &camera,
                layers: &layers,
                samplers: &samplers,
            },
            8,
        );
        let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("layer-table-target"),
            size: wgpu::Extent3d {
                width: TILE_SIZE,
                height: TILE_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        Fixture {
            bgl,
            camera,
            layers,
            samplers,
            atlas,
            target,
        }
    }

    fn pipeline(
        gpu: &Gpu,
        f: &Fixture,
        vs: &str,
        fs: &str,
        instanced: bool,
    ) -> wgpu::RenderPipeline {
        let layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("tile-pl"),
                bind_group_layouts: &[Some(&f.bgl)],
                ..Default::default()
            });
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TileInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: TILE_INSTANCE_ATTRS,
        };
        let buffers: &[Option<wgpu::VertexBufferLayout>] = if instanced {
            &[Some(instance_layout)]
        } else {
            &[]
        };
        gpu.device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("layer-table-test"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &gpu.shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers,
                },
                fragment: Some(wgpu::FragmentState {
                    module: &gpu.shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(premultiplied_target(TARGET_FORMAT))],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
    }

    /// Runs one draw against a cleared target and hands back the rendered pixels.
    fn draw(
        gpu: &Gpu,
        f: &Fixture,
        pipeline: &wgpu::RenderPipeline,
        instances: &[TileInstance],
        range: std::ops::Range<u32>,
    ) -> Vec<u8> {
        let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tile-instances"),
            size: ((instances.len().max(1)) * std::mem::size_of::<TileInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !instances.is_empty() {
            gpu.queue
                .write_buffer(&buf, 0, bytemuck::cast_slice(instances));
        }
        let view = f
            .target
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("layer-table-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, f.atlas.bind_group(), &[]);
            if !instances.is_empty() {
                pass.set_vertex_buffer(0, buf.slice(..));
            }
            pass.draw(0..6, range);
        }
        gpu.queue.submit(Some(encoder.finish()));
        read_texture_layer(&gpu.device, &gpu.queue, &f.target, 0, TILE_SIZE)
    }

    fn pixel(image: &[u8], x: u32, y: u32) -> [u8; 4] {
        let i = ((y * TILE_SIZE + x) * 4) as usize;
        [image[i], image[i + 1], image[i + 2], image[i + 3]]
    }

    /// The whole point of the table: two tiles in **one** instanced draw, transformed
    /// differently, because each instance names its own row. Under the per-layer uniform this
    /// needed two draws with a bind group swap between them, and a single draw could only ever
    /// place every tile with one transform.
    #[test]
    fn an_instance_is_transformed_by_the_row_its_layer_index_names() {
        let Some(gpu) = gpu() else { return };
        let mut f = fixture(gpu);
        let red = f.solid_slot(gpu, RED);
        let blue = f.solid_slot(gpu, BLUE);
        let shift = (TILE_SIZE / 2) as f32;
        f.write_rows(
            gpu,
            &[
                LayerData::default(),
                LayerData {
                    offset: [shift, 0.0],
                    ..LayerData::default()
                },
            ],
        );
        let pipe = pipeline(gpu, &f, "vs_tile", "fs_tile", true);

        let image = draw(
            gpu,
            &f,
            &pipe,
            &[
                TileInstance {
                    origin: [0.0, 0.0],
                    slot: red,
                    layer_index: 0,
                },
                TileInstance {
                    origin: [0.0, 0.0],
                    slot: blue,
                    layer_index: 1,
                },
            ],
            0..2,
        );

        assert_eq!(
            pixel(&image, 8, 8),
            RED,
            "row 0 is identity, so the red tile sits where its origin says"
        );
        assert_eq!(
            pixel(&image, TILE_SIZE - 8, 8),
            BLUE,
            "row 1 offsets by half a tile, so the blue tile covers the right half — same draw, \
             same origin, different row"
        );
    }

    /// Both tiles carry the *same* origin and the same row; nothing should move. Guards against
    /// a shader that reads a row by something other than the index it was handed — an instance
    /// counter, say — which the test above alone would not catch.
    #[test]
    fn two_instances_sharing_a_row_land_in_the_same_place() {
        let Some(gpu) = gpu() else { return };
        let mut f = fixture(gpu);
        let red = f.solid_slot(gpu, RED);
        let blue = f.solid_slot(gpu, BLUE);
        f.write_rows(
            gpu,
            &[
                LayerData::default(),
                LayerData {
                    offset: [TILE_SIZE as f32, 0.0],
                    ..LayerData::default()
                },
            ],
        );
        let pipe = pipeline(gpu, &f, "vs_tile", "fs_tile", true);

        let image = draw(
            gpu,
            &f,
            &pipe,
            &[
                TileInstance {
                    origin: [0.0, 0.0],
                    slot: red,
                    layer_index: 0,
                },
                TileInstance {
                    origin: [0.0, 0.0],
                    slot: blue,
                    layer_index: 0,
                },
            ],
            0..2,
        );

        assert_eq!(
            pixel(&image, TILE_SIZE - 8, 8),
            BLUE,
            "the second instance read row 0 like the first, so it covers the first everywhere"
        );
        assert_eq!(pixel(&image, 8, 8), BLUE);
    }

    /// A layer's transform reaches the shader through the row, not through a bind group: scale
    /// about the pivot has to survive the move to the table.
    #[test]
    fn a_rows_scale_and_pivot_still_place_the_tile() {
        let Some(gpu) = gpu() else { return };
        let mut f = fixture(gpu);
        let red = f.solid_slot(gpu, RED);
        let centre = (TILE_SIZE / 2) as f32;
        f.write_rows(
            gpu,
            &[LayerData {
                pivot: [centre, centre],
                scale: [0.5, 0.5],
                ..LayerData::default()
            }],
        );
        let pipe = pipeline(gpu, &f, "vs_tile", "fs_tile", true);

        let image = draw(
            gpu,
            &f,
            &pipe,
            &[TileInstance {
                origin: [0.0, 0.0],
                slot: red,
                layer_index: 0,
            }],
            0..1,
        );

        assert_eq!(
            pixel(&image, centre as u32, centre as u32),
            RED,
            "half scale about the centre keeps the middle covered"
        );
        assert_eq!(
            pixel(&image, 8, 8),
            [0, 0, 0, 0],
            "and pulls the corner in, leaving it clear"
        );
    }

    /// Solid Paper has no instance buffer to carry an atlas slot, so its row holds one and the
    /// draw names the row through its instance range. This is what replaced bitcasting the slot
    /// into `pivot.x` — two draw paths reading the same bytes as different types.
    #[test]
    fn the_solid_quad_reads_its_atlas_slot_from_the_row_the_draw_range_names() {
        let Some(gpu) = gpu() else { return };
        let mut f = fixture(gpu);
        let red = f.solid_slot(gpu, RED);
        let blue = f.solid_slot(gpu, BLUE);
        f.write_rows(
            gpu,
            &[
                LayerData {
                    atlas_slot: red,
                    ..LayerData::default()
                },
                LayerData {
                    atlas_slot: blue,
                    ..LayerData::default()
                },
            ],
        );
        let pipe = pipeline(gpu, &f, "vs_doc_quad", "fs_solid_tile", false);

        let image = draw(gpu, &f, &pipe, &[], 1..2);

        assert_eq!(
            pixel(&image, 8, 8),
            BLUE,
            "instance range 1..2 selects row 1, whose atlas slot is the blue tile"
        );
        assert_eq!(pixel(&image, TILE_SIZE - 8, TILE_SIZE - 8), BLUE);
    }

    /// The Rust row and the WGSL row have to agree byte for byte, and nothing in the type system
    /// enforces it. 1072 bytes is also what makes the WGSL array stride 1072 with no tail
    /// padding (the struct's own alignment is 8, from the three `vec2<f32>` fields, and 1072 is
    /// already a multiple of 8) — a mismatch here misaddresses every row past the first. Plan 23
    /// grew this from 32 to 1072 deliberately; see `LayerData`'s own doc comment for the layout.
    #[test]
    fn a_table_row_is_the_size_the_shader_strides_by() {
        assert_eq!(std::mem::size_of::<LayerData>(), 1072);
        assert_eq!(std::mem::align_of::<LayerData>(), 4);
        assert_eq!(
            std::mem::size_of::<TileInstance>(),
            16,
            "layer_index took the place of padding, so instances did not grow"
        );
    }

    /// Renders one tile of `combos.len()` distinct texels (row-major, one combo per texel)
    /// through `fs_tile` with `row` as its only `LayerData` entry, and hands back the target's
    /// pixels. The target is a *separate* sRGB texture, not `Fixture::TARGET_FORMAT`: `fs_tile`
    /// hands back linear light for correct blending (see the comment above `linear_to_srgb` in
    /// board.wgsl), and only an sRGB target's automatic re-encode on write turns that back into
    /// the same sRGB-encoded byte `AdjustmentLut::apply` computes on the CPU.
    fn render_byte_cube(gpu: &Gpu, f: &Fixture, slot: u32, row: LayerData) -> Vec<u8> {
        f.write_rows(gpu, &[row]);

        let srgb_format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("byte-cube-target"),
            size: wgpu::Extent3d {
                width: TILE_SIZE,
                height: TILE_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: srgb_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("byte-cube-pl"),
                bind_group_layouts: &[Some(&f.bgl)],
                ..Default::default()
            });
        let pipe = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("byte-cube-test"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &gpu.shader,
                    entry_point: Some("vs_tile"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<TileInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: TILE_INSTANCE_ATTRS,
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &gpu.shader,
                    entry_point: Some("fs_tile"),
                    compilation_options: Default::default(),
                    targets: &[Some(premultiplied_target(srgb_format))],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });

        let instance = TileInstance {
            origin: [0.0, 0.0],
            slot,
            layer_index: 0,
        };
        let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("byte-cube-instance"),
            size: std::mem::size_of::<TileInstance>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        gpu.queue
            .write_buffer(&buf, 0, bytemuck::bytes_of(&instance));

        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("byte-cube-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipe);
            pass.set_bind_group(0, f.atlas.bind_group(), &[]);
            pass.set_vertex_buffer(0, buf.slice(..));
            pass.draw(0..6, 0..1);
        }
        gpu.queue.submit(Some(encoder.finish()));
        read_texture_layer(&gpu.device, &gpu.queue, &target, 0, TILE_SIZE)
    }

    /// `fs_tile`'s `apply_adjustments` and `core::filters::AdjustmentLut::apply` are two
    /// independent implementations of the same math — one WGSL, one Rust — kept in step by
    /// hand. This is the test that actually enforces it, over a stratified sample of the byte
    /// cube covering both `LUT_MODE_TONE` (tone only) and `LUT_MODE_TONE_HSL` (tone + hue/sat).
    /// A 1-of-255 tolerance absorbs the sRGB round trip: `apply_adjustments` undoes the atlas
    /// texture's automatic sRGB decode in software (`linear_to_srgb`/`srgb_to_linear`) so the
    /// lookup lands on the same byte the CPU path would use, and redoes it in software before
    /// the GPU's own hardware re-encodes on write to the sRGB target — two curves computed two
    /// different ways, not required to be bit-identical.
    #[test]
    fn fs_tile_adjustments_agree_with_the_cpu_lut_over_a_byte_cube() {
        let Some(gpu) = gpu() else { return };
        let mut f = fixture(gpu);

        const STEPS: [u8; 9] = [0, 32, 64, 96, 128, 160, 192, 224, 255];
        let mut combos: Vec<[u8; 3]> = Vec::new();
        for &r in &STEPS {
            for &g in &STEPS {
                for &b in &STEPS {
                    combos.push([r, g, b]);
                }
            }
        }
        assert!(combos.len() <= (TILE_SIZE * TILE_SIZE) as usize);

        let mut base = vec![0u8; TILE_BYTES];
        for (i, rgb) in combos.iter().enumerate() {
            let px = i * 4;
            base[px] = rgb[0];
            base[px + 1] = rgb[1];
            base[px + 2] = rgb[2];
            base[px + 3] = 255;
        }
        let shared = SharedBindings {
            layout: &f.bgl,
            camera: &f.camera,
            layers: &f.layers,
            samplers: &f.samplers,
        };
        let slot = f
            .atlas
            .allocate(&gpu.device, &gpu.queue, &shared)
            .expect("slot");
        f.atlas.write(&gpu.queue, slot, &base, &[]);

        for adjustments in [
            // Tone only: saturation and vibrance neutral, so `write_layer_data` would pick
            // `LUT_MODE_TONE` and the shader never enters `hsl_stage`.
            Adjustments {
                brightness: 0.15,
                contrast: 0.2,
                vibrance: 0.0,
                saturation: 0.0,
                levels_gamma: 1.4,
            },
            // Tone + HSL: exercises `rgb_to_hsl` / `hue_to_rgb` / `hsl_to_rgb` too.
            Adjustments {
                brightness: 0.15,
                contrast: 0.2,
                vibrance: 0.3,
                saturation: -0.25,
                levels_gamma: 1.4,
            },
        ] {
            let lut = AdjustmentLut::new(&adjustments);
            let row = if lut.is_tone_only() {
                LayerData {
                    tone: *lut.tone_table(),
                    lut_mode: LUT_MODE_TONE,
                    ..LayerData::default()
                }
            } else {
                LayerData {
                    tone: *lut.tone_table(),
                    lut_mode: LUT_MODE_TONE_HSL,
                    saturation: adjustments.saturation,
                    vibrance: adjustments.vibrance,
                    ..LayerData::default()
                }
            };

            let image = render_byte_cube(gpu, &f, slot, row);

            let mut max_diff = 0i32;
            for (i, rgb) in combos.iter().enumerate() {
                let expected = lut.apply(*rgb);
                let got = pixel(&image, (i as u32) % TILE_SIZE, (i as u32) / TILE_SIZE);
                assert_eq!(got[3], 255, "alpha is untouched by adjustments");
                for c in 0..3 {
                    max_diff = max_diff.max((got[c] as i32 - expected[c] as i32).abs());
                }
            }
            assert!(
                max_diff <= 1,
                "GPU and CPU adjustments disagree by more than 1 of 255 somewhere (max {max_diff}, lut_mode {})",
                row.lut_mode
            );
        }
    }
}
