//! Turning a `TextRun` into a shaped `cosmic-text` buffer — and caching the result, since a
//! caret move, a selection drag or a hit-test asks this same question of the same run several
//! times in a row without anything about it having changed.
//!
//! Shaping lives here and nowhere else, because everything the rest of this crate answers —
//! extent, caret, hit test, selection geometry, coverage — is a question asked of the buffer
//! this module builds. Two runs that shape differently would give two different answers to the
//! same question.

use crate::fonts::{with_engine, TextEngine};
use crate::run::{TextAlign, TextRun};
use crate::span::StyleSpan;
use cosmic_text::fontdb::Family;
use cosmic_text::{Align, Attrs, Buffer, Color, FontSystem, Metrics, Shaping};

fn align_of(align: TextAlign) -> Align {
    match align {
        TextAlign::Left => Align::Left,
        TextAlign::Center => Align::Center,
        TextAlign::Right => Align::Right,
    }
}

/// Every `TextRun` field that changes what shaping produces — everything but `color` and
/// `origin`, which change how the shaped buffer is drawn or placed, never what it is. Compared
/// against a live `TextRun` field-by-field with no allocation (`matches`); a cache miss is the
/// only time anything here gets cloned, to describe the buffer that replaces it.
#[derive(Clone, PartialEq)]
pub(crate) struct ShapeKey {
    text: String,
    marked: String,
    marked_at: usize,
    family: String,
    bold: bool,
    italic: bool,
    size: f32,
    line_height: f32,
    align: TextAlign,
    wrap_width: Option<f32>,
    spans: Vec<StyleSpan>,
}

impl ShapeKey {
    fn of(run: &TextRun) -> Self {
        Self {
            text: run.text.clone(),
            marked: run.marked.clone(),
            marked_at: run.marked_at,
            family: run.family.clone(),
            bold: run.bold,
            italic: run.italic,
            size: run.size,
            line_height: run.line_height,
            align: run.align,
            wrap_width: run.wrap_width,
            spans: run.spans.clone(),
        }
    }

    fn matches(&self, run: &TextRun) -> bool {
        self.marked_at == run.marked_at
            && self.bold == run.bold
            && self.italic == run.italic
            && self.size == run.size
            && self.line_height == run.line_height
            && self.align == run.align
            && self.wrap_width == run.wrap_width
            && self.text == run.text
            && self.marked == run.marked
            && self.family == run.family
            && self.spans == run.spans
    }
}

/// One shaped buffer, held only as long as it still describes the run it was built from.
/// `TextEngine` is a single process-wide instance (`fonts::with_engine`), so this is a
/// single-slot cache rather than one per run — correct because only one text session is ever
/// being queried at a time, and a miss (a different run, or this one having actually changed)
/// costs exactly what building one always cost.
pub(crate) type ShapeCache = Option<(ShapeKey, Buffer)>;

/// Shapes the run into a buffer, with rich-text pieces when it carries spans and a single
/// `Attrs` when it does not — so an unstyled run takes exactly the path it always did.
fn build_buffer(font_system: &mut FontSystem, run: &TextRun) -> Buffer {
    let metrics = Metrics::new(run.size.max(1.0), run.line_spacing().max(1.0));
    let mut buffer = Buffer::new(font_system, metrics);
    buffer.set_size(run.wrap_width, None);
    let base = Attrs::new()
        .family(Family::Name(&run.family))
        .weight(crate::fonts::weight_of(run.bold))
        .style(crate::fonts::style_of(run.italic));
    let display = run.display_text();
    let spans = run.display_spans();
    if spans.is_empty() {
        buffer.set_text(
            &display,
            &base,
            Shaping::Advanced,
            Some(align_of(run.align)),
        );
    } else {
        buffer.set_rich_text(
            rich_pieces(run, &display, &spans, &base),
            &base,
            Shaping::Advanced,
            Some(align_of(run.align)),
        );
    }
    buffer.shape_until_scroll(font_system, false);
    buffer
}

/// The run cut into the pieces cosmic-text shapes separately: the gaps between spans carry the
/// run's own attributes, each span carries them with its overrides laid on top. An empty span
/// list never reaches here, so an unstyled run still takes the single-attrs path it always did.
fn rich_pieces<'a>(
    run: &'a TextRun,
    display: &'a str,
    spans: &'a [StyleSpan],
    base: &Attrs<'a>,
) -> Vec<(&'a str, Attrs<'a>)> {
    let boundary = |index: usize| {
        let mut i = index.min(display.len());
        while i > 0 && !display.is_char_boundary(i) {
            i -= 1;
        }
        i
    };
    let mut out = Vec::with_capacity(spans.len() * 2 + 1);
    let mut at = 0usize;
    for span in spans {
        let start = boundary(span.start).max(at);
        let end = boundary(span.end).max(start);
        if start > at {
            out.push((&display[at..start], base.clone()));
        }
        if end > start {
            out.push((&display[start..end], span_attrs(run, span, base)));
        }
        at = end;
    }
    if at < display.len() {
        out.push((&display[at..], base.clone()));
    }
    out
}

fn span_attrs<'a>(run: &'a TextRun, span: &'a StyleSpan, base: &Attrs<'a>) -> Attrs<'a> {
    let mut attrs = base.clone();
    if let Some(family) = &span.style.family {
        attrs = attrs.family(Family::Name(family));
    }
    if let Some(bold) = span.style.bold {
        attrs = attrs.weight(crate::fonts::weight_of(bold));
    }
    if let Some(italic) = span.style.italic {
        attrs = attrs.style(crate::fonts::style_of(italic));
    }
    if let Some(size) = span.style.size {
        attrs = attrs.metrics(Metrics::new(
            size.max(1.0),
            run.span_line_spacing(size).max(1.0),
        ));
    }
    if let Some(color) = span.style.color {
        attrs = attrs.color(Color::rgba(color[0], color[1], color[2], color[3]));
    }
    attrs
}

/// The buffer that shapes `run` right now — the cached one if it still describes `run`,
/// otherwise a fresh one that replaces it. Takes the cache and the font system as separate
/// borrows, rather than the whole `TextEngine`, specifically so a caller that also needs
/// `engine.swash_cache` (`raster.rs::rasterize`) can hold both at once: a function returning a
/// sub-borrow of an opaque `&mut TextEngine` would make the borrow checker assume the whole
/// engine stays borrowed for as long as the buffer is, which is exactly the borrow a rasterize
/// pass needs to share.
pub(crate) fn ensure_shaped<'c>(
    cache: &'c mut ShapeCache,
    font_system: &mut FontSystem,
    run: &TextRun,
) -> &'c mut Buffer {
    let hit = matches!(cache, Some((key, _)) if key.matches(run));
    if !hit {
        *cache = Some((ShapeKey::of(run), build_buffer(font_system, run)));
    }
    &mut cache.as_mut().expect("just populated above").1
}

pub(crate) fn with_buffer<R>(
    run: &TextRun,
    f: impl FnOnce(&mut Buffer, &mut FontSystem) -> R,
) -> R {
    with_engine(|engine: &mut TextEngine| {
        let buffer = ensure_shaped(&mut engine.shape_cache, &mut engine.font_system, run);
        f(buffer, &mut engine.font_system)
    })
}
