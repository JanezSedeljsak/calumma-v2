use crate::document::Document;
use crate::layer::LayerContent;
use calumma_text::TextAlign;

/// The knobs the Text tool exposes, and the one-way door out of a text layer.
///
/// Each setter writes the session's run when there is one *and* the document's own
/// `text_style` always, so the next text layer starts where the last one left off.
impl Document {
    /// A family the font database does not know is refused rather than stored: the run would
    /// keep a name nothing can shape and every glyph would silently come out of a fallback
    /// face, which reads as the setting having worked when it has not.
    pub fn set_text_family(&mut self, family: &str) -> bool {
        let Some(resolved) = calumma_text::canonical_family(family) else {
            return false;
        };
        self.text_style.family = resolved.to_string();
        self.with_run(|run, _| run.family = resolved.to_string());
        true
    }

    pub fn set_text_bold(&mut self, bold: bool) {
        self.text_style.bold = bold;
        self.with_run(|run, _| run.bold = bold);
    }

    pub fn set_text_italic(&mut self, italic: bool) {
        self.text_style.italic = italic;
        self.with_run(|run, _| run.italic = italic);
    }

    pub fn set_text_line_height(&mut self, line_height: f32) {
        self.text_style.line_height = line_height;
        self.text_style = std::mem::take(&mut self.text_style).clamped();
        let line_height = self.text_style.line_height;
        self.with_run(|run, _| {
            run.line_height = line_height;
            *run = std::mem::take(run).clamped();
        });
    }

    /// Turns a text layer into ordinary pixels, keeping exactly what is on screen. The run is
    /// dropped, so this is one-way — but it is also the only way a paint tool can ever touch
    /// a headline, since a text layer's tiles are a cache that the next keystroke overwrites.
    pub fn rasterize_text_layer(&mut self, index: usize) -> bool {
        if self.text_edit_layer() == Some(index) {
            self.commit_text();
        }
        let (width, height) = (self.width, self.height);
        let Some(layer) = self.layers.get_mut(index) else {
            return false;
        };
        if !layer.is_text() {
            return false;
        }
        let content = std::mem::replace(&mut layer.content, LayerContent::raster(width, height));
        if let LayerContent::Text { tiles, .. } = content {
            layer.content = LayerContent::Raster(tiles);
        }
        layer.mark_all_dirty();
        true
    }

    pub fn set_text_size(&mut self, size: f32) {
        self.text_style.size = size;
        self.text_style = std::mem::take(&mut self.text_style).clamped();
        let size = self.text_style.size;
        self.with_run(|run, _| {
            run.size = size;
            *run = std::mem::take(run).clamped();
        });
    }

    pub fn set_text_align(&mut self, align: TextAlign) {
        self.text_style.align = align;
        self.with_run(|run, _| run.align = align);
    }

    /// Called whenever the ink color changes: a text layer being typed into recolors live,
    /// exactly like re-picking a color with a text object selected in Photoshop.
    pub fn apply_ink_to_text(&mut self) {
        let color = self.color;
        self.text_style.color = color;
        self.with_run(|run, _| run.color = color);
    }
}
