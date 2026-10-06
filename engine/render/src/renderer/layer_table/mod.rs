use super::*;

#[cfg(test)]
mod adjustment_parity;
#[cfg(test)]
mod tests;

impl Renderer {
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
    /// correctness.
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
            row.blend_mode = layer.blend_mode.as_u32();
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
                        row.hue = adjustments.hue;
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
}
