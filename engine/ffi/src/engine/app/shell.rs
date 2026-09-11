use super::{Engine, Inner};
use anyhow::{Context, Result};
use calumma_io::{encode_pdf, encode_psd, encode_rgba, encode_svg, RasterFormat};
use calumma_ops::{run_op_on_document, OpKind, OpParams};

const THUMB_MAX_SIDE: u32 = 160;

impl Engine {
    pub fn layer_preview_revision(&self, index: usize) -> u64 {
        let inner = self.inner.lock();
        let doc = match inner.doc.as_ref() {
            Some(doc) => doc,
            None => return 0,
        };
        let layer = match doc.layers.get(index) {
            Some(layer) => layer,
            None => return 0,
        };
        let mut rev = match layer.tiles() {
            Some(grid) => grid.content_revision(),
            None => u64::from(layer.content.item().is_some()),
        };
        if layer.clips_to.is_some() {
            rev = rev.wrapping_mul(0x9E37_79B9).wrapping_add(1);
        }
        if let Some(t) = layer.transform {
            rev ^= u64::from(t.offset_x.to_bits());
            rev ^= u64::from(t.offset_y.to_bits()).rotate_left(16);
        }
        if let Some(adj) = layer.adjustments {
            rev ^= u64::from(adj.brightness.to_bits());
            rev ^= u64::from(adj.contrast.to_bits()).rotate_left(8);
            rev ^= u64::from(adj.vibrance.to_bits()).rotate_left(16);
            rev ^= u64::from(adj.saturation.to_bits()).rotate_left(24);
            rev ^= u64::from(adj.levels_gamma.to_bits()).rotate_left(32);
        }
        rev ^= u64::from(layer.opacity.to_bits()).rotate_left(40);
        rev
    }

    pub fn layer_thumbnail_rgba(&self, index: usize) -> Option<(u32, u32, Vec<u8>)> {
        let mut inner = self.inner.lock();
        let doc = inner.doc.as_mut()?;
        if doc.is_layer_clipped(index) {
            return doc.clipped_layer_thumbnail(index, THUMB_MAX_SIDE);
        }
        let layer = doc.layers.get_mut(index)?;
        let adjustments = layer.adjustments;
        let opacity = layer.opacity;
        let (w, h, mut rgba) = if let Some(tiles) = layer.tiles_mut() {
            tiles.preview().scaled(THUMB_MAX_SIDE.max(1))
        } else {
            let item = layer.content.item()?;
            let side = THUMB_MAX_SIDE.clamp(1, 64);
            let color = item.color();
            let mut rgba = vec![0u8; (side * side * 4) as usize];
            for px in rgba.chunks_exact_mut(4) {
                px.copy_from_slice(&color);
            }
            (side, side, rgba)
        };
        if let Some(adj) = adjustments.filter(|a| !a.is_neutral()) {
            adj.lut().apply_rgba(&mut rgba);
        }
        if opacity < 1.0 {
            for px in rgba.chunks_exact_mut(4) {
                px[3] = ((px[3] as f32) * opacity).round().clamp(0.0, 255.0) as u8;
            }
        }
        Some((w, h, rgba))
    }

    pub fn set_layer_visible(&mut self, index: usize, visible: bool) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_layer_visible(index, visible);
            inner.dirty_save = true;
            inner.invalidate_renderer();
        }
    }

    pub fn set_layer_locked(&mut self, index: usize, locked: bool) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            if doc.set_layer_locked(index, locked) {
                inner.dirty_save = true;
                inner.invalidate_renderer();
            }
        }
    }

    pub fn layer_opacity(&self, index: usize) -> f32 {
        self.inner
            .lock()
            .doc
            .as_ref()
            .and_then(|doc| doc.layers.get(index))
            .map(|layer| layer.opacity)
            .unwrap_or(1.0)
    }

    pub fn set_layer_opacity(&mut self, index: usize, opacity: f32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.set_layer_opacity(index, opacity);
            inner.dirty_save = true;
            inner.invalidate_renderer();
        }
    }

    pub fn remove_layer(&mut self, index: usize) -> bool {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            if !doc.remove_layer(index) {
                return false;
            }
            inner.dirty_save = true;
            inner.invalidate_renderer();
            return true;
        }
        false
    }

    pub fn duplicate_layer(&mut self, index: usize) -> bool {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            if !doc.duplicate_layer(index) {
                return false;
            }
            inner.dirty_save = true;
            inner.invalidate_renderer();
            return true;
        }
        false
    }

    pub fn has_selection(&self) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .is_some_and(|doc| doc.selection.is_some())
    }

    pub fn active_layer_index(&self) -> Option<usize> {
        self.inner.lock().doc.as_ref().map(|doc| doc.active_layer)
    }

    pub fn active_layer_is_raster(&self) -> bool {
        let inner = self.inner.lock();
        let doc = match inner.doc.as_ref() {
            Some(doc) => doc,
            None => return false,
        };
        doc.layers
            .get(doc.active_layer)
            .is_some_and(|layer| layer.tiles().is_some())
    }

    pub fn op_available(&self, kind: OpKind) -> bool {
        self.inner.lock().registry.available(kind)
    }

    pub fn run_smart_op(&mut self, kind: OpKind) -> Result<()> {
        self.run_smart_op_with(kind, OpParams::default())
    }

    fn run_smart_op_with(&mut self, kind: OpKind, params: OpParams) -> Result<()> {
        let index = self
            .active_layer_index()
            .context("no active layer for the smart tool")?;
        self.run_op_on_layer(kind, index, params)
    }

    pub fn run_upscale(&mut self) -> Result<()> {
        self.run_smart_op(OpKind::Upscale)
    }

    /// With a selection live, that selection is the seed — everything outside it is background
    /// — which is what the menu promises when it reads "Cut Out What You Drew Around".
    pub fn run_smart_matte(&mut self) -> Result<()> {
        let seed_region = {
            let inner = self.inner.lock();
            inner.doc.as_ref().and_then(|doc| {
                doc.selection
                    .as_ref()
                    .map(|selection| selection.to_mask(doc.width, doc.height))
            })
        };
        self.run_smart_op_with(
            OpKind::SmartMatte,
            OpParams {
                seed_region,
                ..OpParams::default()
            },
        )
    }

    pub fn run_seam_carve_narrow(&mut self) -> Result<()> {
        let mut inner = self.inner.lock();
        let Inner {
            doc,
            registry,
            dirty_save,
            ..
        } = &mut *inner;
        let doc = doc.as_mut().context("no project is open")?;
        let index = doc.active_layer;
        let (w, h, _) = doc
            .layer_rgba(index)
            .context("seam carve needs a pixel layer")?;
        let target_w = (w * 9 / 10).max(1);
        let params = OpParams {
            target_size: Some((target_w, h)),
            ..Default::default()
        };
        run_op_on_document(registry, doc, index, OpKind::SeamCarve, &params)
            .context("running seam carve")?;
        *dirty_save = true;
        inner.invalidate_renderer();
        Ok(())
    }

    fn run_op_on_layer(&mut self, kind: OpKind, index: usize, params: OpParams) -> Result<()> {
        let mut inner = self.inner.lock();
        if !inner.registry.available(kind) {
            anyhow::bail!("tool is not available");
        }
        let Inner {
            doc,
            registry,
            dirty_save,
            ..
        } = &mut *inner;
        let doc = doc.as_mut().context("no project is open")?;
        run_op_on_document(registry, doc, index, kind, &params).context("running the tool")?;
        *dirty_save = true;
        inner.invalidate_renderer();
        Ok(())
    }

    pub fn export_raster(&self, format: RasterFormat) -> Result<Vec<u8>> {
        let inner = self.inner.lock();
        let doc = inner.doc.as_ref().context("no project is open")?;
        let (width, height, rgba) = doc.composite_rgba();
        encode_rgba(&rgba, width, height, format).ok_or_else(|| anyhow::anyhow!("encoding export"))
    }

    pub fn export_layer_raster(&self, index: usize, format: RasterFormat) -> Result<Vec<u8>> {
        let inner = self.inner.lock();
        let doc = inner.doc.as_ref().context("no project is open")?;
        let (width, height, rgba) = doc
            .layer_rgba(index)
            .context("layer has no raster content")?;
        encode_rgba(&rgba, width, height, format)
            .ok_or_else(|| anyhow::anyhow!("encoding layer export"))
    }

    pub fn export_psd_bytes(&self) -> Result<Vec<u8>> {
        let inner = self.inner.lock();
        let doc = inner.doc.as_ref().context("no project is open")?;
        Ok(encode_psd(doc))
    }

    pub fn export_svg_string(&self) -> Result<String> {
        let inner = self.inner.lock();
        let doc = inner.doc.as_ref().context("no project is open")?;
        Ok(encode_svg(doc))
    }

    pub fn export_pdf_bytes(&self) -> Result<Vec<u8>> {
        let inner = self.inner.lock();
        let doc = inner.doc.as_ref().context("no project is open")?;
        Ok(encode_pdf(doc, 144.0))
    }

    pub fn export_layer_svg(&self, index: usize) -> Result<String> {
        let inner = self.inner.lock();
        let doc = inner.doc.as_ref().context("no project is open")?;
        doc.layer_svg(index)
            .ok_or_else(|| anyhow::anyhow!("layer has no vector content"))
    }

    pub fn layer_is_vector(&self, index: usize) -> bool {
        let inner = self.inner.lock();
        inner
            .doc
            .as_ref()
            .and_then(|doc| doc.layers.get(index))
            .is_some_and(|layer| layer.content.item().is_some())
    }

    pub fn layer_bounds(&self, index: usize) -> Option<(f32, f32, f32, f32)> {
        self.inner
            .lock()
            .doc
            .as_ref()
            .and_then(|doc| doc.layer_bounds(index))
    }

    pub fn set_layer_bounds(&mut self, index: usize, x: f32, y: f32, w: f32, h: f32) -> bool {
        let mut inner = self.inner.lock();
        let Some(doc) = inner.doc.as_mut() else {
            return false;
        };
        if !doc.set_layer_bounds(index, x, y, w, h) {
            return false;
        }
        inner.dirty_save = true;
        inner.invalidate_renderer();
        true
    }

    pub fn resize_document(&mut self, width: u32, height: u32) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            doc.resize(width, height);
            inner.dirty_save = true;
            inner.invalidate_renderer();
        }
    }

    pub fn set_hover_layer(&mut self, index: Option<usize>) {
        let mut inner = self.inner.lock();
        if let Some(doc) = &mut inner.doc {
            if doc.hover_layer != index {
                doc.hover_layer = index;
                inner.invalidate_overlay();
            }
        }
    }
}
