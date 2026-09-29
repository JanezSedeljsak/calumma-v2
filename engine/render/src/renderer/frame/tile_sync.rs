//! Keeping the GPU's copy of the document's tiles current: which tiles upload this frame,
//! which stay resident, and the clip links a transform change re-evaluates.

use super::*;

impl Renderer {
    pub(in crate::renderer) fn sync_clip_for_transform_changes(&mut self, doc: &mut Document) {
        if self.layer_transform_stamp.len() != doc.layers.len() {
            self.layer_transform_stamp = doc.layers.iter().map(|l| l.transform).collect();
            return;
        }
        let mut changed = Vec::new();
        for (i, layer) in doc.layers.iter().enumerate() {
            if self.layer_transform_stamp[i] != layer.transform {
                changed.push(i);
                self.layer_transform_stamp[i] = layer.transform;
            }
        }
        if !changed.is_empty() {
            doc.schedule_clip_recalc_for_indices(&changed);
        }
    }

    pub(in crate::renderer) fn sync_tiles(&mut self, doc: &mut Document) {
        self.sync_clip_for_transform_changes(doc);
        let Some(visible) = doc.visible_rect() else {
            return;
        };
        let retained = visible.expanded_by_tiles(self.budget.retention_margin_tiles());
        let _doc_width = doc.width;

        let dirty_bases: Vec<usize> = doc
            .layers
            .iter()
            .enumerate()
            .filter(|(_, layer)| {
                layer
                    .tiles()
                    .is_some_and(|grid| !grid.dirty_tiles(DirtyChannel::Render).is_empty())
            })
            .map(|(i, _)| i)
            .collect();
        for base_index in dirty_bases {
            doc.mark_clip_dependents_render_dirty(base_index);
        }

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
            if doc.is_mask_base_id(&layer.id) {
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

        // Bake clip alpha into every dirty tile up front and in parallel, alongside the mip chain
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
                let clip_base = doc.clip_base_for_layer(layer);
                let composited = composited_tile_payload(pixels, *coord, layer, clip_base);
                let base: &[u8] = composited.as_deref().unwrap_or(pixels.as_slice());
                let mips = tile_upload_mips(base, *skip_mips);
                Some((composited, mips))
            })
            .collect();

        // Tiles retained only as prefetch margin (in `live`, but not currently on screen) are
        // the ones sacrificed first when the atlas is full — see the eviction loop inside the
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
                    // A solid fill (`TileGrid::fill_uniform`) and the `shared_gpu` reuse just
                    // above both point several tile coordinates at one atlas slot. Freeing that
                    // slot because *one* of those coordinates was picked as prefetch-margin
                    // filler would corrupt every sibling still drawing from it the moment the
                    // slot is handed to whatever gets uploaded next — visible as some other
                    // layer's tiles glitching even though nothing on it was touched. Only a
                    // slot no other resident tile still references may actually be freed; a
                    // victim that turns out to be shared just stops being tracked as resident
                    // and is picked up again as a plain re-upload if it is ever needed.
                    let mut freed_slot = None;
                    while let Some(victim) = evictable.pop() {
                        let Some(freed) = self.tiles.remove(&victim) else {
                            continue;
                        };
                        if self
                            .tiles
                            .values()
                            .any(|t| t.array_layer == freed.array_layer)
                        {
                            continue;
                        }
                        self.atlas.free(freed.array_layer);
                        freed_slot = self.atlas.allocate(&self.device, &self.queue, &shared);
                        break;
                    }
                    let Some(slot) = freed_slot else {
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
}
