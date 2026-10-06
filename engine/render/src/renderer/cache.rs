use super::*;

impl Renderer {
    pub(super) fn clear_layer_cache(&mut self) {
        self.cached_retained_span = None;
        self.cached_visible_span = None;
        self.cached_tile_instances.clear();
        self.cached_strokes.clear();
        self.cached_shapes.clear();
        self.cached_fills.clear();
        self.cached_fill_edges.clear();
        self.cached_draws.clear();
    }

    pub(super) fn rebuild_layer_cache(&mut self, doc: &Document) {
        let mut tile_instances = Vec::new();
        let mut vectors = VectorInstances::default();
        let draws = self.build_layer_draws(doc, &mut tile_instances, &mut vectors);
        self.cached_tile_instances = tile_instances;
        self.cached_strokes = vectors.strokes;
        self.cached_shapes = vectors.shapes;
        self.cached_fills = vectors.fills;
        self.cached_fill_edges = vectors.fill_edges;
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
            if doc.is_mask_base_id(&layer.id) {
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
            if doc.is_mask_base_id(&layer.id) {
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
        self.layer_transform_stamp.clear();
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
