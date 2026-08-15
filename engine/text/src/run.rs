use crate::limits::{
    TEXT_LINE_HEIGHT_DEFAULT, TEXT_LINE_HEIGHT_MAX, TEXT_LINE_HEIGHT_MIN, TEXT_SIZE_DEFAULT,
    TEXT_SIZE_MAX, TEXT_SIZE_MIN, TEXT_WRAP_MIN_WIDTH,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl TextAlign {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Left),
            1 => Some(Self::Center),
            2 => Some(Self::Right),
            _ => None,
        }
    }

    pub fn as_u32(self) -> u32 {
        match self {
            Self::Left => 0,
            Self::Center => 1,
            Self::Right => 2,
        }
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
        self
    }
}
