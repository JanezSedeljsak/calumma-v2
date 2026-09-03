use crate::document::Document;
use calumma_text::{ResolvedStyle, SpanStyle, TextAlign, TEXT_WRAP_MIN_WIDTH};

/// The knobs the Text tool exposes. The one-way door out of a text layer moved to
/// `rasterize.rs`, which a vector layer now walks through too.
///
/// Each setter writes the session's run when there is one *and* the document's own
/// `text_style` always, so the next text layer starts where the last one left off.
///
/// With a **selection** the character knobs — family, weight, slant, size, colour — write a
/// span over the selected range instead of the whole block, which is what every text editor
/// does and the only reason `StyleSpan` exists. `align`, `line_height` and `wrap_width` never
/// take that branch: they are paragraph properties and there is nothing narrower to write
/// them to.
impl Document {
    /// A family the font database does not know is refused rather than stored: the run would
    /// keep a name nothing can shape and every glyph would silently come out of a fallback
    /// face, which reads as the setting having worked when it has not.
    pub fn set_text_family(&mut self, family: &str) -> bool {
        let Some(resolved) = calumma_text::canonical_family(family) else {
            return false;
        };
        self.text_style.family = resolved.to_string();
        self.style_selection_or_run(
            SpanStyle {
                family: Some(resolved.to_string()),
                ..SpanStyle::default()
            },
            |run| run.family = resolved.to_string(),
        );
        true
    }

    pub fn set_text_bold(&mut self, bold: bool) {
        self.text_style.bold = bold;
        self.style_selection_or_run(
            SpanStyle {
                bold: Some(bold),
                ..SpanStyle::default()
            },
            |run| run.bold = bold,
        );
    }

    pub fn set_text_italic(&mut self, italic: bool) {
        self.text_style.italic = italic;
        self.style_selection_or_run(
            SpanStyle {
                italic: Some(italic),
                ..SpanStyle::default()
            },
            |run| run.italic = italic,
        );
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
        self.style_selection_or_run(
            SpanStyle {
                size: Some(size),
                ..SpanStyle::default()
            },
            |run| {
                run.size = size;
                *run = std::mem::take(run).clamped();
            },
        );
    }

    pub fn set_text_align(&mut self, align: TextAlign) {
        self.text_style.align = align;
        self.with_run(|run, _| run.align = align);
    }

    /// How wide the block wraps, or `None` for a run that grows with its longest line. This is
    /// what a dragged text box sets, and the only paragraph property a box gesture writes.
    pub fn set_text_wrap_width(&mut self, wrap_width: Option<f32>) {
        let wrap = wrap_width.filter(|w| w.is_finite() && *w >= TEXT_WRAP_MIN_WIDTH);
        self.with_run(|run, _| {
            run.wrap_width = wrap;
            *run = std::mem::take(run).clamped();
        });
    }

    pub fn text_wrap_width(&self) -> Option<f32> {
        self.active_text_run()?.wrap_width
    }

    /// Called whenever the ink color changes: a text layer being typed into recolors live,
    /// exactly like re-picking a color with a text object selected in Photoshop — and only the
    /// selected words when there are any.
    pub fn apply_ink_to_text(&mut self) {
        let color = self.color;
        self.text_style.color = color;
        self.style_selection_or_run(
            SpanStyle {
                color: Some(color),
                ..SpanStyle::default()
            },
            |run| run.color = color,
        );
    }

    /// The style the shell shows and the next keystroke inherits: resolved at the selection's
    /// start when there is one, at the caret otherwise, and from the document's own defaults
    /// when no text layer is in play at all.
    pub fn active_text_style(&self) -> ResolvedStyle {
        let Some(run) = self.active_text_run() else {
            return self.text_style.style_at(0);
        };
        let at = self
            .text_selection()
            .map(|(start, _)| start)
            .or_else(|| self.text_caret())
            .unwrap_or(0);
        run.style_at(at)
    }

    fn style_selection_or_run(
        &mut self,
        span: SpanStyle,
        whole: impl FnOnce(&mut calumma_text::TextRun),
    ) {
        let selection = self.text_selection();
        self.with_run(|run, _| match selection {
            Some((start, end)) => run.apply_style(start, end, &span),
            None => {
                run.clear_span_overrides(&span);
                whole(run);
            }
        });
    }
}
