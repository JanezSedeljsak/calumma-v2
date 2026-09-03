//! The one-way door out of a live layer: turn what is on screen into ordinary pixels.
//!
//! Text and vector layers are both "live" — a text layer's tiles are rebuilt from its run, and
//! a vector layer stores parameters and has no tiles at all — which is why neither can take a
//! brush. Rasterizing is what a user reaches for when they want to paint on one anyway, so
//! both kinds answer to the same verb.

use crate::document::Document;
use crate::layer::LayerContent;
use crate::tile::TileGrid;
use crate::vector;

impl Document {
    /// Turns a text layer into ordinary pixels, keeping exactly what is on screen. The run is
    /// dropped, so this is one-way — but it is also the only way a paint tool can ever touch
    /// a headline, since a text layer's tiles are a cache that the next keystroke overwrites.
    pub fn rasterize_text_layer(&mut self, index: usize) -> bool {
        if self.text_edit_layer() == Some(index) {
            self.text_edit.take();
        }
        let (width, height) = (self.width, self.height);
        let Some(layer) = self.layers.get(index) else {
            return false;
        };
        if !layer.is_text() {
            return false;
        }
        self.record_stack_history();
        let Some(layer) = self.layers.get_mut(index) else {
            return false;
        };
        let content = std::mem::replace(&mut layer.content, LayerContent::raster(width, height));
        if let LayerContent::Text { tiles, .. } = content {
            layer.content = LayerContent::Raster(tiles);
        }
        layer.mark_all_dirty();
        true
    }

    /// Turns a vector layer into ordinary pixels by evaluating its item once at document
    /// resolution — the same distance functions the board and the exporter use, so the result
    /// is what was already on screen.
    ///
    /// The layer transform is *baked* here rather than carried over, unlike the text case: a
    /// vector item's transform pivots on its parameter bounds, and the painted result's bounds
    /// are the inked ones clipped to the canvas. Keeping the transform would re-apply it about
    /// a pivot that had moved, which is a visible jump. Flattening it costs nothing else — the
    /// item is gone either way.
    pub fn rasterize_vector_layer(&mut self, index: usize) -> bool {
        let (width, height) = (self.width, self.height);
        if width == 0 || height == 0 {
            return false;
        }
        let Some(layer) = self.layers.get(index) else {
            return false;
        };
        let Some(item) = layer.content.item().cloned() else {
            return false;
        };
        let transform = layer.transform;
        self.record_stack_history();
        let mut buf = vec![0u8; (width as usize) * (height as usize) * 4];
        vector::rasterize_into_rgba(&item, transform, &mut buf, width, height);

        let mut tiles = TileGrid::new(width, height);
        tiles.blit_rgba(&buf, width, height);

        let Some(layer) = self.layers.get_mut(index) else {
            return false;
        };
        layer.content = LayerContent::Raster(tiles);
        layer.transform = None;
        layer.mark_all_dirty();
        self.clear_vector_selection();
        self.bump_vector_revision();
        true
    }

    /// Whichever of the two applies. The layers panel offers one command for both kinds, so
    /// the shell never has to ask what a layer is before offering to flatten it.
    pub fn rasterize_layer(&mut self, index: usize) -> bool {
        match self.layers.get(index).map(|layer| &layer.content) {
            Some(LayerContent::Text { .. }) => self.rasterize_text_layer(index),
            Some(LayerContent::Vector(_)) => self.rasterize_vector_layer(index),
            _ => false,
        }
    }

    /// Whether `rasterize_layer` would do anything — the panel greys the command out on a
    /// layer that is already pixels rather than offering a no-op.
    pub fn layer_is_rasterizable(&self, index: usize) -> bool {
        self.layers
            .get(index)
            .is_some_and(|layer| layer.is_text() || layer.content.is_vector())
    }
}
