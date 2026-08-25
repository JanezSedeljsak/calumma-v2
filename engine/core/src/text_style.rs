use crate::document::Document;
use calumma_text::TextAlign;

/// The knobs the Text tool exposes. The one-way door out of a text layer moved to
/// `rasterize.rs`, which a vector layer now walks through too.
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
