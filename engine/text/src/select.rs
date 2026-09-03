use crate::buffer::with_buffer;
use crate::layout::run_span;
use crate::run::TextRun;

/// One row of a selection highlight, in document coordinates — the quad the board fills
/// behind the glyphs. A selection is a list of these because a range crosses visual rows, and
/// a wrapped paragraph has more rows than it has lines.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectionRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// How wide a selected line break is drawn. A range that swallows a newline has to look like
/// it did, and the break itself has no glyph to measure.
const BREAK_WIDTH_RATIO: f32 = 0.3;

/// Byte offset of the first character of each `BufferLine`, so a glyph's line-relative range
/// can be compared against a caret offset, which is always document-wide.
fn line_offsets(buffer: &cosmic_text::Buffer) -> Vec<usize> {
    let mut out = Vec::with_capacity(buffer.lines.len());
    let mut at = 0usize;
    for line in buffer.lines.iter() {
        out.push(at);
        at += line.text().len() + 1;
    }
    out
}

/// The highlight geometry for a byte range of the run, asked of the shaped layout rather than
/// of the string — the same rule caret placement follows, and the only way a wrapped,
/// aligned or bidirectional row answers correctly.
pub fn selection_rects(run: &TextRun, start: usize, end: usize) -> Vec<SelectionRect> {
    let low = run.clamp_index(start.min(end));
    let high = run.clamp_index(start.max(end));
    if low >= high {
        return Vec::new();
    }
    let from = run.display_index(low);
    let to = run.display_index(high);
    let break_width = run.size * BREAK_WIDTH_RATIO;
    with_buffer(run, |buffer, _| {
        let offsets = line_offsets(buffer);
        let mut out = Vec::new();
        for row in buffer.layout_runs() {
            let base = offsets.get(row.line_i).copied().unwrap_or(0);
            let line_len = buffer
                .lines
                .get(row.line_i)
                .map_or(0, |line| line.text().len());
            let (span_start, span_end) = run_span(&row);
            let mut min_x = f32::MAX;
            let mut max_x = f32::MIN;
            for glyph in row.glyphs {
                if base + glyph.start >= to || base + glyph.end <= from {
                    continue;
                }
                min_x = min_x.min(glyph.x);
                max_x = max_x.max(glyph.x + glyph.w);
            }
            // The break at the end of this line is inside the range: show it, the way a
            // selection dragged past the end of a line does in every editor.
            let takes_break = to > base + line_len && from <= base + line_len;
            if min_x > max_x {
                if !takes_break || span_start != span_end {
                    continue;
                }
                min_x = 0.0;
                max_x = 0.0;
            }
            if takes_break {
                max_x += break_width;
            }
            out.push(SelectionRect {
                x: min_x + run.origin.0,
                y: row.line_top + run.origin.1,
                width: max_x - min_x,
                height: row.line_height,
            });
        }
        out
    })
}

/// What a character counts as when a double-click asks for "the word here". Punctuation
/// groups with punctuation rather than each mark standing alone, which is what makes
/// double-clicking `...` select all three dots.
#[derive(Clone, Copy, PartialEq)]
enum CharClass {
    Word,
    Space,
    Other,
}

fn class_of(c: char) -> CharClass {
    if c.is_alphanumeric() || c == '_' {
        CharClass::Word
    } else if c.is_whitespace() {
        CharClass::Space
    } else {
        CharClass::Other
    }
}

/// The word around a byte offset. Word *motion* goes through cosmic-text (`Step::WordLeft`),
/// but a double-click is a different question — it asks what is under the pointer, not where
/// the next boundary is, and at a boundary the two answers disagree.
pub fn word_range(run: &TextRun, index: usize) -> (usize, usize) {
    let text = &run.text;
    let at = run.clamp_index(index);
    let after = text[at..].chars().next();
    let before = text[..at].chars().next_back();
    // The character the click landed on decides, unless it is past the end of a word — then
    // the word just closed is the one meant.
    let class = match (after, before) {
        (Some(a), Some(b))
            if class_of(a) == CharClass::Space && class_of(b) != CharClass::Space =>
        {
            class_of(b)
        }
        (Some(a), _) => class_of(a),
        (None, Some(b)) => class_of(b),
        (None, None) => return (0, 0),
    };
    let mut start = at;
    for c in text[..at].chars().rev() {
        if class_of(c) != class {
            break;
        }
        start -= c.len_utf8();
    }
    let mut end = at;
    for c in text[at..].chars() {
        if class_of(c) != class {
            break;
        }
        end += c.len_utf8();
    }
    (start, end)
}

/// The paragraph around a byte offset — newline to newline, the break itself excluded. This
/// is what a triple-click selects; a *visual* row is `Step::LineStart`/`LineEnd`, and on a
/// wrapped block the two are deliberately different answers.
pub fn paragraph_range(run: &TextRun, index: usize) -> (usize, usize) {
    let text = &run.text;
    let at = run.clamp_index(index);
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let end = text[at..].find('\n').map_or(text.len(), |i| at + i);
    (start, end)
}
