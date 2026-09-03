//! Turning a `TextRun` into a shaped `cosmic-text` buffer.
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

/// Shapes the run into a throwaway buffer, with rich-text pieces when it carries spans and a
/// single `Attrs` when it does not — so an unstyled run takes exactly the path it always did.
pub(crate) fn build_buffer(engine: &mut TextEngine, run: &TextRun) -> Buffer {
    let metrics = Metrics::new(run.size.max(1.0), run.line_spacing().max(1.0));
    let mut buffer = Buffer::new(&mut engine.font_system, metrics);
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
    buffer.shape_until_scroll(&mut engine.font_system, false);
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

pub(crate) fn with_buffer<R>(
    run: &TextRun,
    f: impl FnOnce(&mut Buffer, &mut FontSystem) -> R,
) -> R {
    with_engine(|engine| {
        let mut buffer = build_buffer(engine, run);
        f(&mut buffer, &mut engine.font_system)
    })
}
