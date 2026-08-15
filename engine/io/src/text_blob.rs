use calumma_core::{TextAlign, TextRun};

/// Blob layout in use. Version 1 knew no weight or slant; it is still read, with both
/// answered as off, so a project saved before styled text keeps opening.
const VERSION: u8 = 2;
const VERSION_UNSTYLED: u8 = 1;

fn put_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
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
    out
}

pub fn decode(bytes: &[u8]) -> Option<TextRun> {
    let version = bytes.first().copied()?;
    if version != VERSION && version != VERSION_UNSTYLED {
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
        _ => None,
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
        }
        .clamped(),
    )
}
