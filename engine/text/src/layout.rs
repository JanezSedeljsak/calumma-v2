use crate::buffer::with_buffer;
use crate::run::TextRun;
use cosmic_text::{Buffer, Cursor, LayoutRun, Motion};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretRect {
    pub x: f32,
    pub y: f32,
    pub height: f32,
}

use num_enum::TryFromPrimitive;

#[derive(Clone, Copy, Debug, PartialEq, Eq, TryFromPrimitive)]
#[repr(u32)]
pub enum Step {
    Left = 0,
    Right = 1,
    Up = 2,
    Down = 3,
    LineStart = 4,
    LineEnd = 5,
    DocStart = 6,
    DocEnd = 7,
    WordLeft = 8,
    WordRight = 9,
}

impl Step {
    pub fn from_u32(value: u32) -> Option<Self> {
        Self::try_from(value).ok()
    }
}

pub(crate) fn cursor_to_offset(buffer: &Buffer, cursor: Cursor) -> usize {
    let mut offset = 0usize;
    for (i, line) in buffer.lines.iter().enumerate() {
        if i == cursor.line {
            return offset + cursor.index.min(line.text().len());
        }
        offset += line.text().len() + 1;
    }
    offset
}

pub(crate) fn offset_to_cursor(buffer: &Buffer, offset: usize) -> Cursor {
    let mut remaining = offset;
    for (i, line) in buffer.lines.iter().enumerate() {
        let len = line.text().len();
        if remaining <= len {
            return Cursor::new(i, remaining);
        }
        remaining -= len + 1;
    }
    let last = buffer.lines.len().saturating_sub(1);
    let len = buffer.lines.get(last).map_or(0, |l| l.text().len());
    Cursor::new(last, len)
}

/// Width and height of the laid-out block, in document units, measured from the run origin.
pub fn measure(run: &TextRun) -> (f32, f32) {
    with_buffer(run, |buffer, _| {
        let mut width = 0.0f32;
        let mut height = 0.0f32;
        for line in buffer.layout_runs() {
            width = width.max(line.line_w);
            height = height.max(line.line_top + line.line_height);
        }
        if height <= 0.0 {
            height = run.line_spacing();
        }
        (width, height)
    })
}

/// Byte range of the original line that one visual row covers. A wrapped paragraph is a
/// single `BufferLine` laid out as several rows, so the row a caret belongs to can only be
/// found by asking which glyphs it holds — `line_i` alone names the paragraph, not the row.
pub(crate) fn run_span(run: &LayoutRun<'_>) -> (usize, usize) {
    let mut start = usize::MAX;
    let mut end = 0usize;
    for glyph in run.glyphs {
        start = start.min(glyph.start);
        end = end.max(glyph.end);
    }
    if start == usize::MAX {
        (0, 0)
    } else {
        (start, end)
    }
}

/// Where the caret sits relative to one glyph: before it in reading order, which is its left
/// edge in an LTR run and its right edge in an RTL one.
fn leading_edge(glyph: &cosmic_text::LayoutGlyph) -> f32 {
    if glyph.level.is_rtl() {
        glyph.x + glyph.w
    } else {
        glyph.x
    }
}

fn trailing_edge(run: &LayoutRun<'_>) -> f32 {
    let mut edge = 0.0f32;
    let mut seen = false;
    for glyph in run.glyphs {
        let x = if glyph.level.is_rtl() {
            glyph.x
        } else {
            glyph.x + glyph.w
        };
        edge = if seen {
            if glyph.level.is_rtl() {
                edge.min(x)
            } else {
                edge.max(x)
            }
        } else {
            x
        };
        seen = true;
    }
    edge
}

/// The visual row a caret belongs to: the first row of its paragraph that reaches the caret,
/// or the paragraph's last row when the caret is past them all.
///
/// A caret exactly on a soft-wrap boundary belongs to the row it *ends*, not the one that
/// follows — after typing a word that wrapped, the caret has to stay next to the word.
fn row_for<'a>(buffer: &'a Buffer, cursor: Cursor) -> Option<LayoutRun<'a>> {
    let mut rows: Vec<LayoutRun<'a>> = buffer
        .layout_runs()
        .filter(|row| row.line_i == cursor.line)
        .collect();
    let at = rows
        .iter()
        .position(|row| cursor.index <= run_span(row).1)
        .unwrap_or(rows.len().saturating_sub(1));
    if at >= rows.len() {
        return None;
    }
    Some(rows.swap_remove(at))
}

/// End of a visual row for the caret's purposes. A soft-wrapped row keeps the space that
/// broke it, and parking the caret after that space would draw it at the start of the next
/// row — so a wrapped row ends before its trailing blanks, and only a real paragraph end
/// includes everything.
fn row_end(row: &LayoutRun<'_>, start: usize, end: usize) -> usize {
    if end >= row.text.len() {
        return end;
    }
    let slice = row.text.get(start..end).unwrap_or_default();
    start + slice.trim_end().len()
}

fn caret_in_buffer(buffer: &Buffer, cursor: Cursor, fallback_height: f32) -> CaretRect {
    let Some(row) = row_for(buffer, cursor) else {
        return CaretRect {
            x: 0.0,
            y: 0.0,
            height: fallback_height,
        };
    };
    let x = row
        .glyphs
        .iter()
        .filter(|glyph| cursor.index < glyph.end)
        .min_by_key(|glyph| glyph.start)
        .map_or_else(|| trailing_edge(&row), leading_edge);
    CaretRect {
        x,
        y: row.line_top,
        height: row.line_height,
    }
}

/// Caret position for a byte offset into `run.text`, in document coordinates.
pub fn caret_rect(run: &TextRun, index: usize) -> CaretRect {
    let display = run.display_index(index);
    let spacing = run.line_spacing();
    let local = with_buffer(run, |buffer, _| {
        let cursor = offset_to_cursor(buffer, display);
        caret_in_buffer(buffer, cursor, spacing)
    });
    CaretRect {
        x: local.x + run.origin.0,
        y: local.y + run.origin.1,
        height: local.height,
    }
}

pub(crate) fn display_to_text_index(run: &TextRun, display: usize) -> usize {
    if run.marked.is_empty() {
        return run.clamp_index(display);
    }
    let at = run.clamp_index(run.marked_at);
    if display <= at {
        run.clamp_index(display)
    } else {
        run.clamp_index(display.saturating_sub(run.marked.len()).max(at))
    }
}

/// Byte offset into `run.text` for a document-space point — what a click on the board maps
/// to when it lands inside a text layer.
pub fn index_at_point(run: &TextRun, x: f32, y: f32) -> usize {
    let local_x = x - run.origin.0;
    let local_y = y - run.origin.1;
    let display = with_buffer(run, |buffer, _| match buffer.hit(local_x, local_y) {
        Some(cursor) => cursor_to_offset(buffer, cursor),
        None => {
            if local_y < 0.0 {
                0
            } else {
                buffer
                    .lines
                    .iter()
                    .map(|l| l.text().len() + 1)
                    .sum::<usize>()
                    .saturating_sub(1)
            }
        }
    });
    display_to_text_index(run, display)
}

/// Caret motion, asked of the shaped layout rather than of the string.
///
/// Every step but the two document ends goes through the buffer: horizontal moves so they
/// cross whole grapheme clusters (an emoji with a skin-tone modifier is one keypress, not
/// two half-glyphs), line ends so they stop at the end of the *visual* row on a wrapped
/// paragraph, and vertical moves so they land under the caret's own x rather than where byte
/// arithmetic would put them.
pub fn step_index(run: &TextRun, index: usize, step: Step) -> usize {
    let index = run.clamp_index(index);
    match step {
        Step::DocStart => 0,
        Step::DocEnd => run.text.len(),
        Step::Up | Step::Down => {
            let display = run.display_index(index);
            let spacing = run.line_spacing();
            let target = with_buffer(run, |buffer, _| {
                let cursor = offset_to_cursor(buffer, display);
                let caret = caret_in_buffer(buffer, cursor, spacing);
                let dy = if step == Step::Up {
                    -caret.height * 0.5
                } else {
                    caret.height * 1.5
                };
                match buffer.hit(caret.x, caret.y + dy) {
                    Some(next) => cursor_to_offset(buffer, next),
                    None => display,
                }
            });
            display_to_text_index(run, target)
        }
        Step::LineStart | Step::LineEnd => {
            let display = run.display_index(index);
            let target = with_buffer(run, |buffer, _| {
                let cursor = offset_to_cursor(buffer, display);
                let Some(row) = row_for(buffer, cursor) else {
                    return display;
                };
                let (start, end) = run_span(&row);
                let at = if step == Step::LineStart {
                    start
                } else {
                    row_end(&row, start, end)
                };
                cursor_to_offset(buffer, Cursor::new(cursor.line, at))
            });
            display_to_text_index(run, target)
        }
        _ => {
            let display = run.display_index(index);
            let motion = match step {
                Step::Left => Motion::Left,
                Step::WordLeft => Motion::LeftWord,
                Step::WordRight => Motion::RightWord,
                _ => Motion::Right,
            };
            let target = with_buffer(run, |buffer, font_system| {
                let cursor = offset_to_cursor(buffer, display);
                match buffer.cursor_motion(font_system, cursor, None, motion) {
                    Some((next, _)) => cursor_to_offset(buffer, next),
                    None => display,
                }
            });
            display_to_text_index(run, target)
        }
    }
}
