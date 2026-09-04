//! A text layer as PDF content-stream text operators — the twin of [`crate::vector_pdf`] for
//! the other kind of layer this exporter treats as real geometry rather than a picture of
//! itself. Real, selectable, embedded text rather than the rasterized image every layer used
//! to export as (`AGENTS.md`'s former "text rides as pixels" note).
//!
//! Font embedding — allocating `/Font`/`/FontDescriptor`/`/FontFile2` object numbers — is
//! `io::pdf`'s job, the same split `vector_pdf::item_pdf` already has with the file-structure
//! writer. This module only ever answers two questions: what does the `BT ... ET` text-showing
//! block look like, and what does a `ToUnicode` CMap body look like.

use calumma_text::{PdfFontKey, PdfGlyph, PdfRun};
use std::collections::BTreeMap;
use std::fmt::Write as _;

fn n(value: f32) -> String {
    let text = format!("{value:.4}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

/// The `BT ... ET` block for a text layer's runs, given the PDF resource name (`/F0`, `/F1`,
/// …) each font key was assigned. Every glyph carries its own `Tm` rather than relying on the
/// font's declared advance to place the next one — the shaped layout is the source of truth
/// for position, not a width table, so kerning and complex shaping land exactly where the
/// buffer already put them. `None` when every run's font is missing from `font_names` (nothing
/// this layer used could be embedded), so the caller knows to fall back to rasterizing it.
pub fn runs_pdf(runs: &[PdfRun], font_names: &BTreeMap<PdfFontKey, String>) -> Option<String> {
    let mut out = String::from("BT\n");
    let mut wrote_any = false;
    for run in runs {
        let Some(name) = font_names.get(&run.font) else {
            continue;
        };
        let (r, g, b) = (
            run.color[0] as f32 / 255.0,
            run.color[1] as f32 / 255.0,
            run.color[2] as f32 / 255.0,
        );
        let _ = writeln!(
            out,
            "/{name} {} Tf {} {} {} rg",
            n(run.size),
            n(r),
            n(g),
            n(b)
        );
        for glyph in &run.glyphs {
            let _ = writeln!(
                out,
                "1 0 0 -1 {} {} Tm <{:04X}> Tj",
                n(glyph.x),
                n(glyph.y),
                glyph.glyph_id
            );
            wrote_any = true;
        }
    }
    out.push_str("ET");
    wrote_any.then_some(out)
}

/// UTF-16BE, the encoding a `ToUnicode` CMap's replacement text is always written in
/// regardless of the font's own encoding — `bfchar`/`bfrange` values are hex byte strings, and
/// this is the byte order the spec fixes for them.
fn utf16be_hex(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 4);
    for unit in text.encode_utf16() {
        let _ = write!(out, "{unit:04X}");
    }
    out
}

/// A `/ToUnicode` CMap stream body mapping each glyph code back to the source text it stands
/// for, so a reader's "copy text" gives back the real string. `entries` is `(glyph_id, text)`
/// pairs, already deduplicated by glyph id (`calumma_text::to_unicode_entries`).
///
/// `beginbfchar`/`endbfchar` blocks are capped at 100 entries by the PDF spec, so this chunks
/// rather than assuming any layer's distinct-glyph count stays under that — a long CJK
/// paragraph would not.
pub fn to_unicode_cmap(entries: &[(u16, String)]) -> String {
    let mut out = String::from(
        "/CIDInit /ProcSet findresource begin\n\
12 dict begin\n\
begincmap\n\
/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
/CMapName /Adobe-Identity-UCS def\n\
/CMapType 2 def\n\
1 begincodespacerange\n\
<0000> <FFFF>\n\
endcodespacerange\n",
    );
    for chunk in entries.chunks(100) {
        let _ = writeln!(out, "{} beginbfchar", chunk.len());
        for (gid, text) in chunk {
            let _ = writeln!(out, "<{gid:04X}> <{}>", utf16be_hex(text));
        }
        out.push_str("endbfchar\n");
    }
    out.push_str(
        "endcmap\n\
CMapName currentdict /CMap defineresource pop\n\
end\n\
end",
    );
    out
}

/// Every distinct glyph id a font key's runs actually touch, sorted — what `embed_font` wants
/// so its `/W` array is not the whole face's glyph count.
pub fn glyph_ids_for(runs: &[PdfRun], key: PdfFontKey) -> Vec<u16> {
    let mut ids: Vec<u16> = runs
        .iter()
        .filter(|r| r.font == key)
        .flat_map(|r| r.glyphs.iter().map(|g: &PdfGlyph| g.glyph_id))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// A `/W` array body — `gid [width]` triples, the simplest legal form, one entry per glyph
/// actually used rather than a range compression a handful of glyphs has no use for.
pub fn widths_array(widths: &[(u16, u16)]) -> String {
    let mut out = String::new();
    for (gid, width) in widths {
        let _ = write!(out, "{gid} [{width}] ");
    }
    out
}
