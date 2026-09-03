use crate::limits::{
    TEXT_LINE_HEIGHT_DEFAULT, TEXT_LINE_HEIGHT_MAX, TEXT_LINE_HEIGHT_MIN, TEXT_SIZE_DEFAULT,
    TEXT_SIZE_MAX, TEXT_SIZE_MIN, TEXT_WRAP_MIN_WIDTH,
};
use crate::span::{self, SpanStyle, StyleSpan};

use num_enum::{IntoPrimitive, TryFromPrimitive};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u32)]
pub enum TextAlign {
    #[default]
    Left = 0,
    Center = 1,
    Right = 2,
}

impl TextAlign {
    pub fn from_u32(value: u32) -> Option<Self> {
        Self::try_from(value).ok()
    }

    pub fn as_u32(self) -> u32 {
        self.into()
    }
}

/// Everything needed to lay a string out and draw it, and the whole of what a text layer
/// persists. Caret position is *not* here — that belongs to the edit session, not to the
/// content — but `marked` is, because an in-flight IME composition has to be visible on the
/// board before it is committed.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub marked: String,
    pub marked_at: usize,
    pub family: String,
    pub bold: bool,
    pub italic: bool,
    pub size: f32,
    pub line_height: f32,
    pub color: [u8; 4],
    pub align: TextAlign,
    pub origin: (f32, f32),
    pub wrap_width: Option<f32>,
    /// Overrides on byte ranges of `text`. Empty is the common case and means the fields
    /// above apply to the whole block, which is what every run written before styled ranges
    /// existed decodes to.
    pub spans: Vec<StyleSpan>,
}

impl Default for TextRun {
    fn default() -> Self {
        Self {
            text: String::new(),
            marked: String::new(),
            marked_at: 0,
            family: crate::fonts::default_family(),
            bold: false,
            italic: false,
            size: TEXT_SIZE_DEFAULT,
            line_height: TEXT_LINE_HEIGHT_DEFAULT,
            color: [0, 0, 0, 255],
            align: TextAlign::Left,
            origin: (0.0, 0.0),
            wrap_width: None,
            spans: Vec::new(),
        }
    }
}

impl TextRun {
    pub fn at(origin: (f32, f32), color: [u8; 4]) -> Self {
        Self {
            origin,
            color,
            ..Self::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.marked.is_empty()
    }

    /// The string as the board shows it: committed text with any IME composition spliced in
    /// at the caret. Layout, rasterizing and hit-testing all run against this, never against
    /// `text` alone, so a composition measures and draws exactly where it will land.
    pub fn display_text(&self) -> String {
        if self.marked.is_empty() {
            return self.text.clone();
        }
        let at = self.clamp_index(self.marked_at);
        let mut out = String::with_capacity(self.text.len() + self.marked.len());
        out.push_str(&self.text[..at]);
        out.push_str(&self.marked);
        out.push_str(&self.text[at..]);
        out
    }

    /// Maps a caret offset in `text` onto the same caret in `display_text`.
    pub fn display_index(&self, index: usize) -> usize {
        let index = self.clamp_index(index);
        if self.marked.is_empty() {
            return index;
        }
        let at = self.clamp_index(self.marked_at);
        if index <= at {
            index
        } else {
            index + self.marked.len()
        }
    }

    pub fn clamp_index(&self, index: usize) -> usize {
        let index = index.min(self.text.len());
        if self.text.is_char_boundary(index) {
            return index;
        }
        let mut i = index;
        while i > 0 && !self.text.is_char_boundary(i) {
            i -= 1;
        }
        i
    }

    pub fn line_spacing(&self) -> f32 {
        self.size * self.line_height
    }

    pub fn clamped(mut self) -> Self {
        self.size = if self.size.is_finite() {
            self.size.clamp(TEXT_SIZE_MIN, TEXT_SIZE_MAX)
        } else {
            TEXT_SIZE_DEFAULT
        };
        self.line_height = if self.line_height.is_finite() {
            self.line_height
                .clamp(TEXT_LINE_HEIGHT_MIN, TEXT_LINE_HEIGHT_MAX)
        } else {
            TEXT_LINE_HEIGHT_DEFAULT
        };
        self.wrap_width = self
            .wrap_width
            .filter(|w| w.is_finite())
            .map(|w| w.max(TEXT_WRAP_MIN_WIDTH));
        if !self.origin.0.is_finite() || !self.origin.1.is_finite() {
            self.origin = (0.0, 0.0);
        }
        if self.family.trim().is_empty() {
            self.family = crate::fonts::default_family();
        }
        self.marked_at = self.clamp_index(self.marked_at);
        span::normalize(&mut self.spans, &self.text);
        self
    }

    /// The style in force at a byte offset: the span covering it where there is one, the run's
    /// own fields everywhere else. This is what the options panel reads, and what a new
    /// keystroke inherits.
    pub fn style_at(&self, index: usize) -> ResolvedStyle {
        let mut out = ResolvedStyle {
            family: self.family.clone(),
            bold: self.bold,
            italic: self.italic,
            size: self.size,
            color: self.color,
        };
        let Some(span) = span::at(&self.spans, self.clamp_index(index)) else {
            return out;
        };
        if let Some(family) = &span.style.family {
            out.family = family.clone();
        }
        if let Some(bold) = span.style.bold {
            out.bold = bold;
        }
        if let Some(italic) = span.style.italic {
            out.italic = italic;
        }
        if let Some(size) = span.style.size {
            out.size = size;
        }
        if let Some(color) = span.style.color {
            out.color = color;
        }
        out
    }

    /// Drops, from every span, exactly the fields `style` states.
    ///
    /// This is what a knob turned with *nothing* selected does before it writes the run's own
    /// field: making the block bold has to mean the block is bold, and a leftover span saying
    /// otherwise would read as the setting having failed. Only the stated fields go — turning
    /// off bold must not also discard a colour somebody set on one word.
    pub fn clear_span_overrides(&mut self, style: &SpanStyle) {
        for span in &mut self.spans {
            if style.family.is_some() {
                span.style.family = None;
            }
            if style.bold.is_some() {
                span.style.bold = None;
            }
            if style.italic.is_some() {
                span.style.italic = None;
            }
            if style.size.is_some() {
                span.style.size = None;
            }
            if style.color.is_some() {
                span.style.color = None;
            }
        }
        span::normalize(&mut self.spans, &self.text);
    }

    pub fn apply_style(&mut self, start: usize, end: usize, style: &SpanStyle) {
        let start = self.clamp_index(start);
        let end = self.clamp_index(end);
        span::apply(&mut self.spans, start, end, style);
        span::normalize(&mut self.spans, &self.text);
    }

    /// The one text mutation. Insert is a replace of an empty range, delete a replace with an
    /// empty string, and both move every later span boundary through `span::after_edit` —
    /// which is why no caller shifts a boundary itself.
    pub fn replace_range(&mut self, start: usize, end: usize, insert: &str) {
        let start = self.clamp_index(start);
        let end = self.clamp_index(end).max(start);
        self.spans = span::after_edit(&self.spans, start, end - start, insert.len());
        self.text.replace_range(start..end, insert);
        span::normalize(&mut self.spans, &self.text);
    }

    /// The spans as they apply to [`display_text`], with an in-flight composition spliced in.
    /// The composition takes the style of the span the caret sits in — the same edge rule a
    /// committed keystroke gets, so what is shown while composing is what lands.
    pub fn display_spans(&self) -> Vec<StyleSpan> {
        if self.marked.is_empty() {
            return self.spans.clone();
        }
        span::after_edit(
            &self.spans,
            self.clamp_index(self.marked_at),
            0,
            self.marked.len(),
        )
    }

    /// Line spacing for one span's size, so a larger word makes its own row taller instead of
    /// overlapping the one above. The *ratio* stays the run's: line height is a paragraph
    /// property and does not vary inside a block.
    pub fn span_line_spacing(&self, size: f32) -> f32 {
        size * self.line_height
    }
}

/// Every varying field, answered — no `Option` left. What the shell shows and what layout
/// shapes with, so neither has to walk the span list itself.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedStyle {
    pub family: String,
    pub bold: bool,
    pub italic: bool,
    pub size: f32,
    pub color: [u8; 4],
}
