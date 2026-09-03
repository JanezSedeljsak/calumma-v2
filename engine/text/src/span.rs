use crate::limits::{TEXT_SIZE_DEFAULT, TEXT_SIZE_MAX, TEXT_SIZE_MIN};

/// The part of a run's formatting that may vary *inside* one block. Alignment, line height,
/// origin and wrap width are deliberately absent: they are paragraph properties, and
/// cosmic-text treats them that way too.
///
/// Every field is optional because a span states only what it overrides — a span that makes
/// one word bold must not also freeze that word's family against a later change to the run.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpanStyle {
    pub family: Option<String>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub size: Option<f32>,
    pub color: Option<[u8; 4]>,
}

impl SpanStyle {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    /// `other`'s stated fields win; everything it leaves unset keeps this span's answer. This
    /// is what makes "bold this word, then colour half of it" two spans instead of a conflict.
    pub fn overlay(&mut self, other: &Self) {
        if let Some(family) = &other.family {
            self.family = Some(family.clone());
        }
        if let Some(bold) = other.bold {
            self.bold = Some(bold);
        }
        if let Some(italic) = other.italic {
            self.italic = Some(italic);
        }
        if let Some(size) = other.size {
            self.size = Some(size);
        }
        if let Some(color) = other.color {
            self.color = Some(color);
        }
    }

    fn clamped(mut self) -> Self {
        self.size = self.size.map(|size| {
            if size.is_finite() {
                size.clamp(TEXT_SIZE_MIN, TEXT_SIZE_MAX)
            } else {
                TEXT_SIZE_DEFAULT
            }
        });
        if self.family.as_ref().is_some_and(|f| f.trim().is_empty()) {
            self.family = None;
        }
        self
    }
}

/// A byte range of `TextRun::text` carrying an override. Ranges are kept sorted and
/// non-overlapping by [`normalize`], so "what applies here" is one lookup and never a
/// priority question.
#[derive(Clone, Debug, PartialEq)]
pub struct StyleSpan {
    pub start: usize,
    pub end: usize,
    pub style: SpanStyle,
}

/// Sorts, clips to `len` and to char boundaries, drops what covers nothing, and merges
/// neighbours that say the same thing. Every mutation below ends here, so no other code has
/// to reason about overlap.
pub fn normalize(spans: &mut Vec<StyleSpan>, text: &str) {
    let boundary = |index: usize| {
        let mut i = index.min(text.len());
        while i > 0 && !text.is_char_boundary(i) {
            i -= 1;
        }
        i
    };
    for span in spans.iter_mut() {
        span.start = boundary(span.start);
        span.end = boundary(span.end);
        span.style = std::mem::take(&mut span.style).clamped();
    }
    spans.retain(|span| span.start < span.end && !span.style.is_empty());
    spans.sort_by_key(|span| (span.start, span.end));
    let mut out: Vec<StyleSpan> = Vec::with_capacity(spans.len());
    for mut span in spans.drain(..) {
        let Some(last) = out.last_mut() else {
            out.push(span);
            continue;
        };
        span.start = span.start.max(last.end);
        if span.start >= span.end {
            continue;
        }
        if span.start == last.end && span.style == last.style {
            last.end = span.end;
            continue;
        }
        out.push(span);
    }
    *spans = out;
}

/// Where a byte offset lands after `removed` bytes at `at` are replaced by `inserted` new ones.
///
/// A boundary sitting exactly *on* the edit point moves right, and that one choice is the whole
/// behaviour of typing at a span's edge: the span that *ends* there grows to cover what was
/// typed — so typing at the end of a bold word stays bold — while the span that *starts* there
/// is pushed along instead of adopting it. Anything inside a deleted range collapses onto the
/// edit point, which is what makes a span whose text is gone an empty span that
/// [`normalize`] then drops.
fn shift(offset: usize, at: usize, removed: usize, inserted: usize) -> usize {
    if offset < at {
        offset
    } else if offset >= at + removed {
        offset - removed + inserted
    } else {
        at
    }
}

/// The single place a text edit moves span boundaries. Every insert and delete goes through
/// it rather than open-coding the arithmetic, because getting it wrong in one call site is a
/// silent formatting corruption rather than a crash.
pub fn after_edit(
    spans: &[StyleSpan],
    at: usize,
    removed: usize,
    inserted: usize,
) -> Vec<StyleSpan> {
    spans
        .iter()
        .map(|span| StyleSpan {
            start: shift(span.start, at, removed, inserted),
            end: shift(span.end, at, removed, inserted),
            style: span.style.clone(),
        })
        .collect()
}

/// Writes `style`'s stated fields over `start..end`, splitting whatever was there.
///
/// Existing spans keep their own answers outside the range and merge underneath it; the parts
/// of the range no span covered become new spans carrying only what was asked for.
pub fn apply(spans: &mut Vec<StyleSpan>, start: usize, end: usize, style: &SpanStyle) {
    if start >= end || style.is_empty() {
        return;
    }
    let mut out: Vec<StyleSpan> = Vec::with_capacity(spans.len() + 2);
    let mut inside: Vec<StyleSpan> = Vec::new();
    for span in spans.drain(..) {
        if span.end <= start || span.start >= end {
            out.push(span);
            continue;
        }
        if span.start < start {
            out.push(StyleSpan {
                start: span.start,
                end: start,
                style: span.style.clone(),
            });
        }
        if span.end > end {
            out.push(StyleSpan {
                start: end,
                end: span.end,
                style: span.style.clone(),
            });
        }
        let mut merged = span.style;
        merged.overlay(style);
        inside.push(StyleSpan {
            start: span.start.max(start),
            end: span.end.min(end),
            style: merged,
        });
    }
    inside.sort_by_key(|span| span.start);
    let mut at = start;
    for span in inside {
        if span.start > at {
            out.push(StyleSpan {
                start: at,
                end: span.start,
                style: style.clone(),
            });
        }
        at = span.end;
        out.push(span);
    }
    if at < end {
        out.push(StyleSpan {
            start: at,
            end,
            style: style.clone(),
        });
    }
    *spans = out;
}

/// The span that governs a byte offset: the one covering the character *at* it, or — at the
/// very end of a span, where there is no character to the right — the span that ends there.
///
/// That second case is what the pending-input style needs: park the caret after a bold word
/// and the next keystroke is bold, which is exactly the boundary rule [`shift`] applies when
/// the keystroke lands.
pub fn at(spans: &[StyleSpan], index: usize) -> Option<&StyleSpan> {
    spans
        .iter()
        .find(|span| index >= span.start && index < span.end)
        .or_else(|| spans.iter().find(|span| index == span.end))
}
