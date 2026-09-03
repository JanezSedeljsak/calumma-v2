use calumma_core::{SpanStyle, StyleSpan, TextAlign, TextRun};

/// Blob layout in use. Version 1 knew no weight or slant and version 2 no style spans; both
/// are still read — the older fields answered as off, a missing span section as "uniform" —
/// so a project saved before either keeps opening. This is the same versioned-header shape
/// `vector_blob.rs` uses.
const VERSION: u8 = 3;
const VERSION_UNSTYLED: u8 = 1;
const VERSION_UNIFORM: u8 = 2;

/// Which fields a span carries, as a bitmask ahead of its payload. A span states only what it
/// overrides, so the mask is what keeps `None` distinguishable from a written default.
const SPAN_FAMILY: u8 = 1;
const SPAN_BOLD: u8 = 2;
const SPAN_ITALIC: u8 = 4;
const SPAN_SIZE: u8 = 8;
const SPAN_COLOR: u8 = 16;

fn put_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn take_u32(bytes: &[u8], at: &mut usize) -> Option<u32> {
    let slice = bytes.get(*at..*at + 4)?;
    *at += 4;
    Some(u32::from_le_bytes(slice.try_into().ok()?))
}

fn take_f32(bytes: &[u8], at: &mut usize) -> Option<f32> {
    let slice = bytes.get(*at..*at + 4)?;
    *at += 4;
    Some(f32::from_le_bytes(slice.try_into().ok()?))
}

fn put_str(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u32).to_le_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn take_str(bytes: &[u8], at: &mut usize) -> Option<String> {
    let len_slice = bytes.get(*at..*at + 4)?;
    *at += 4;
    let len = u32::from_le_bytes(len_slice.try_into().ok()?) as usize;
    let slice = bytes.get(*at..*at + len)?;
    *at += len;
    String::from_utf8(slice.to_vec()).ok()
}

/// A text layer persists its run, not its pixels — the tiles are a cache the engine rebuilds
/// on open. That keeps the project file small (a paragraph instead of a bitmap) and is what
/// makes text still editable in a project reopened months later.
///
/// `marked` is deliberately absent: an IME composition is never committed content.
pub fn encode(run: &TextRun) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(VERSION);
    put_str(&mut out, &run.text);
    put_str(&mut out, &run.family);
    out.push(u8::from(run.bold));
    out.push(u8::from(run.italic));
    put_f32(&mut out, run.size);
    put_f32(&mut out, run.line_height);
    out.extend_from_slice(&run.color);
    out.extend_from_slice(&run.align.as_u32().to_le_bytes());
    put_f32(&mut out, run.origin.0);
    put_f32(&mut out, run.origin.1);
    match run.wrap_width {
        Some(w) => {
            out.push(1);
            put_f32(&mut out, w);
        }
        None => out.push(0),
    }
    put_u32(&mut out, run.spans.len() as u32);
    for span in &run.spans {
        put_u32(&mut out, span.start as u32);
        put_u32(&mut out, span.end as u32);
        let mut mask = 0u8;
        if span.style.family.is_some() {
            mask |= SPAN_FAMILY;
        }
        if span.style.bold.is_some() {
            mask |= SPAN_BOLD;
        }
        if span.style.italic.is_some() {
            mask |= SPAN_ITALIC;
        }
        if span.style.size.is_some() {
            mask |= SPAN_SIZE;
        }
        if span.style.color.is_some() {
            mask |= SPAN_COLOR;
        }
        out.push(mask);
        if let Some(family) = &span.style.family {
            put_str(&mut out, family);
        }
        if let Some(bold) = span.style.bold {
            out.push(u8::from(bold));
        }
        if let Some(italic) = span.style.italic {
            out.push(u8::from(italic));
        }
        if let Some(size) = span.style.size {
            put_f32(&mut out, size);
        }
        if let Some(color) = span.style.color {
            out.extend_from_slice(&color);
        }
    }
    out
}

fn take_spans(bytes: &[u8], at: &mut usize) -> Option<Vec<StyleSpan>> {
    let count = take_u32(bytes, at)? as usize;
    let mut out = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        let start = take_u32(bytes, at)? as usize;
        let end = take_u32(bytes, at)? as usize;
        let mask = bytes.get(*at).copied()?;
        *at += 1;
        let mut style = SpanStyle::default();
        if mask & SPAN_FAMILY != 0 {
            style.family = Some(take_str(bytes, at)?);
        }
        if mask & SPAN_BOLD != 0 {
            style.bold = Some(bytes.get(*at).copied()? != 0);
            *at += 1;
        }
        if mask & SPAN_ITALIC != 0 {
            style.italic = Some(bytes.get(*at).copied()? != 0);
            *at += 1;
        }
        if mask & SPAN_SIZE != 0 {
            style.size = Some(take_f32(bytes, at)?);
        }
        if mask & SPAN_COLOR != 0 {
            style.color = Some(<[u8; 4]>::try_from(bytes.get(*at..*at + 4)?).ok()?);
            *at += 4;
        }
        out.push(StyleSpan { start, end, style });
    }
    Some(out)
}

pub fn decode(bytes: &[u8]) -> Option<TextRun> {
    let version = bytes.first().copied()?;
    if !matches!(version, VERSION | VERSION_UNIFORM | VERSION_UNSTYLED) {
        return None;
    }
    let mut at = 1usize;
    let text = take_str(bytes, &mut at)?;
    let family = take_str(bytes, &mut at)?;
    let (bold, italic) = if version == VERSION_UNSTYLED {
        (false, false)
    } else {
        let bold = bytes.get(at).copied()? != 0;
        let italic = bytes.get(at + 1).copied()? != 0;
        at += 2;
        (bold, italic)
    };
    let size = take_f32(bytes, &mut at)?;
    let line_height = take_f32(bytes, &mut at)?;
    let color = <[u8; 4]>::try_from(bytes.get(at..at + 4)?).ok()?;
    at += 4;
    let align_bits = u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?);
    at += 4;
    let origin_x = take_f32(bytes, &mut at)?;
    let origin_y = take_f32(bytes, &mut at)?;
    let wrap_width = match bytes.get(at).copied()? {
        1 => {
            at += 1;
            Some(take_f32(bytes, &mut at)?)
        }
        _ => {
            at += 1;
            None
        }
    };
    let spans = if version == VERSION {
        take_spans(bytes, &mut at)?
    } else {
        Vec::new()
    };
    Some(
        TextRun {
            text,
            marked: String::new(),
            marked_at: 0,
            family,
            bold,
            italic,
            size,
            line_height,
            color,
            align: TextAlign::from_u32(align_bits).unwrap_or_default(),
            origin: (origin_x, origin_y),
            wrap_width,
            spans,
        }
        .clamped(),
    )
}
