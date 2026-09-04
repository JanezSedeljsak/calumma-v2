//! Real, selectable PDF text: glyph runs positioned by the same shaped layout everything else
//! in this crate reads, plus enough of the resolved TrueType font to embed it.
//!
//! Scoped to TrueType-outline (`glyf`) fonts on purpose. A CFF-outline face (bare PostScript
//! or an OpenType/CFF wrapper) needs a different descendant-font shape than the one this module
//! builds — `embed_font` answers `None` for one, and the caller falls back to the rasterized
//! text it already had for that layer rather than risk a malformed embed. Most system UI faces
//! (Helvetica Neue, SF Pro, and the `DEFAULT_FAMILY_PREFERENCE` fallbacks) are TrueType.
//!
//! A face loaded out of a `.ttc` collection — the common case for macOS system fonts — cannot
//! be embedded as-is: PDF's `/FontFile2` wants one standalone `sfnt` program, not a collection.
//! `repackage_face` rebuilds one from just the requested face's own tables, recomputing the
//! table directory and the `head` table's checksum adjustment the way the OpenType spec
//! prescribes — the same reason a hand-rolled writer already exists for PDF and PSD elsewhere
//! in this workspace rather than a dependency: the format is small and fully specified here.

use crate::buffer::with_buffer;
use crate::fonts::with_engine;
use crate::run::TextRun;
use cosmic_text::fontdb;
use std::collections::BTreeMap;

/// Identifies one embeddable font face. The `fontdb::ID` inside is private, so `calumma-core`
/// and `calumma-io` can key resources by this and dedupe them without ever naming `fontdb`
/// themselves — the same reason `ShapeKey` in `buffer.rs` stays crate-private.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PdfFontKey(fontdb::ID);

/// One glyph, already positioned in the run's own local space (the same space `TextRun::origin`
/// is added *into*, before any layer transform).
pub struct PdfGlyph {
    pub glyph_id: u16,
    pub x: f32,
    pub y: f32,
    /// The source text this glyph covers (`display_text()[start..end]`) — a ToUnicode CMap
    /// entry maps the glyph code back to this, so copying the exported text out of a PDF
    /// reader gives the real string back rather than nothing or the wrong codepoints. Usually
    /// one character; more than one for a ligature, since a CMap range may map a single code
    /// to a multi-character string.
    pub text: String,
}

/// Consecutive glyphs sharing a font, size and color — the unit a content-stream `Tf`/fill
/// color change is worth spending on. Positioning is still per-glyph (`PdfGlyph::x`/`y`), so a
/// run never needs its own advance-width bookkeeping to stay exactly where the buffer shaped it.
pub struct PdfRun {
    pub font: PdfFontKey,
    pub size: f32,
    pub color: [u8; 4],
    pub glyphs: Vec<PdfGlyph>,
}

/// A TrueType face, repackaged as a standalone program ready for `/FontFile2`, plus the metrics
/// and per-glyph widths a CIDFontType2 dictionary needs to describe it.
pub struct PdfFont {
    pub program: Vec<u8>,
    pub units_per_em: u16,
    pub ascender: i16,
    pub descender: i16,
    /// `(glyph_id, advance)` for every glyph this layer's runs actually use, advance already
    /// scaled to PDF's fixed 1000-unit glyph space — `(glyph_id, width)` pairs, not a dense
    /// array, since a run only ever touches a handful of a face's glyphs.
    pub widths: Vec<(u16, u16)>,
}

/// The run laid out for PDF export, grouped into `PdfRun`s — `None` for nothing to show,
/// matching `rasterize`'s own empty check. Marked (in-progress IME) text never reaches export,
/// so this reads `run.text` through the ordinary shaped buffer rather than `display_text()`.
pub fn layout_for_pdf(run: &TextRun) -> Option<Vec<PdfRun>> {
    if run.text.is_empty() {
        return None;
    }
    with_buffer(run, |buffer, _| {
        let mut runs: Vec<PdfRun> = Vec::new();
        for row in buffer.layout_runs() {
            for glyph in row.glyphs {
                let key = PdfFontKey(glyph.font_id);
                let color = glyph.color_opt.map(|c| c.as_rgba()).unwrap_or(run.color);
                // Mirrors `LayoutGlyph::physical`'s own pen-position math (scale 1, no extra
                // offset) so a glyph lands exactly where the rasterized path already draws it.
                let x = glyph.x + glyph.font_size * glyph.x_offset;
                let y = row.line_y + glyph.y - glyph.font_size * glyph.y_offset;
                let text = row
                    .text
                    .get(glyph.start..glyph.end)
                    .unwrap_or_default()
                    .to_string();
                let placed = PdfGlyph {
                    glyph_id: glyph.glyph_id,
                    x,
                    y,
                    text,
                };
                let same_run = runs.last().is_some_and(|r: &PdfRun| {
                    r.font == key && r.size == glyph.font_size && r.color == color
                });
                if same_run {
                    runs.last_mut().unwrap().glyphs.push(placed);
                } else {
                    runs.push(PdfRun {
                        font: key,
                        size: glyph.font_size,
                        color,
                        glyphs: vec![placed],
                    });
                }
            }
        }
        (!runs.is_empty()).then_some(runs)
    })
}

/// The embeddable font for one key a `layout_for_pdf` result named, with widths for exactly
/// `glyph_ids` (a run only ever touches a handful of a face's glyphs, and the font's full
/// glyph count can run into the thousands). `None` when the face is not TrueType-outline (see
/// module doc).
pub fn embed_font(key: PdfFontKey, glyph_ids: &[u16]) -> Option<PdfFont> {
    with_engine(|engine| embed_face(engine.font_system.db(), key.0, glyph_ids))
}

fn embed_face(db: &fontdb::Database, id: fontdb::ID, glyph_ids: &[u16]) -> Option<PdfFont> {
    let (units_per_em, ascender, descender, has_glyf) =
        db.with_face_data(id, |data, index| {
            let face = ttf_parser::Face::parse(data, index).ok()?;
            Some((
                face.units_per_em(),
                face.ascender(),
                face.descender(),
                face.tables().glyf.is_some(),
            ))
        })??;
    if !has_glyf {
        return None;
    }
    let program = db.with_face_data(id, repackage_face)??;
    let widths = db.with_face_data(id, |data, index| {
        let Ok(face) = ttf_parser::Face::parse(data, index) else {
            return Vec::new();
        };
        glyph_ids
            .iter()
            .map(|&gid| {
                let advance = face
                    .glyph_hor_advance(ttf_parser::GlyphId(gid))
                    .unwrap_or(0);
                let scaled = (advance as f32 * 1000.0 / units_per_em.max(1) as f32).round() as u16;
                (gid, scaled)
            })
            .collect::<Vec<_>>()
    })?;
    Some(PdfFont {
        program,
        units_per_em,
        ascender,
        descender,
        widths,
    })
}

/// Every table of one face — the requested one, if `data` is a `.ttc` collection — copied into
/// a fresh `sfnt` with its own table directory, so it stands alone as a program `/FontFile2`
/// can hand a PDF reader. macOS ships most of its system faces this way (`Helvetica Neue.ttc`
/// and friends), so this is the common path, not an edge case.
fn repackage_face(data: &[u8], face_index: u32) -> Option<Vec<u8>> {
    let raw = ttf_parser::RawFace::parse(data, face_index).ok()?;
    let mut records: Vec<_> = raw.table_records.into_iter().collect();
    records.sort_by_key(|r| r.tag);

    let num_tables = records.len() as u16;
    let has_cff = records
        .iter()
        .any(|r| r.tag == ttf_parser::Tag::from_bytes(b"CFF "));
    let sfnt_version: u32 = if has_cff { 0x4F54544F } else { 0x0001_0000 };
    let (search_range, entry_selector, range_shift) = table_dir_search_params(num_tables);

    let mut out = Vec::new();
    out.extend_from_slice(&sfnt_version.to_be_bytes());
    out.extend_from_slice(&num_tables.to_be_bytes());
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_selector.to_be_bytes());
    out.extend_from_slice(&range_shift.to_be_bytes());

    let dir_start = out.len();
    out.resize(dir_start + records.len() * 16, 0);

    let mut head_pos = None;
    for (i, rec) in records.iter().enumerate() {
        let start = rec.offset as usize;
        let end = start.checked_add(rec.length as usize)?;
        let mut bytes = data.get(start..end)?.to_vec();
        let table_start = out.len();
        if rec.tag == ttf_parser::Tag::from_bytes(b"head") && bytes.len() >= 12 {
            bytes[8..12].copy_from_slice(&[0, 0, 0, 0]);
            head_pos = Some(table_start);
        }
        let checksum = sfnt_checksum(&bytes);
        out.extend_from_slice(&bytes);
        while out.len() % 4 != 0 {
            out.push(0);
        }

        let rp = dir_start + i * 16;
        out[rp..rp + 4].copy_from_slice(&rec.tag.0.to_be_bytes());
        out[rp + 4..rp + 8].copy_from_slice(&checksum.to_be_bytes());
        out[rp + 8..rp + 12].copy_from_slice(&(table_start as u32).to_be_bytes());
        out[rp + 12..rp + 16].copy_from_slice(&rec.length.to_be_bytes());
    }

    if let Some(pos) = head_pos {
        let whole = sfnt_checksum(&out);
        let adjustment = 0xB1B0_AFBAu32.wrapping_sub(whole);
        out[pos + 8..pos + 12].copy_from_slice(&adjustment.to_be_bytes());
    }
    Some(out)
}

/// The `sfnt` table-directory checksum: every 4-byte big-endian word summed with wrapping
/// overflow, the last word zero-padded when the table's length is not a multiple of 4 — exactly
/// as the OpenType spec defines `CalcTableChecksum`.
fn sfnt_checksum(data: &[u8]) -> u32 {
    let mut sum = 0u32;
    for chunk in data.chunks(4) {
        let mut word = [0u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum = sum.wrapping_add(u32::from_be_bytes(word));
    }
    sum
}

/// `searchRange`/`entrySelector`/`rangeShift` for the `sfnt` table directory header: the
/// largest power of two not exceeding `num_tables`, and what is left over.
fn table_dir_search_params(num_tables: u16) -> (u16, u16, u16) {
    let mut pow2 = 0u32;
    while (1u32 << (pow2 + 1)) <= num_tables as u32 {
        pow2 += 1;
    }
    let search_range = (1u32 << pow2) * 16;
    let range_shift = (num_tables as u32 * 16).saturating_sub(search_range);
    (search_range as u16, pow2 as u16, range_shift as u16)
}

/// A CMap `bfchar`/`bfrange` body mapping each glyph code back to the Unicode text it stands
/// for, so a reader's "copy text" gives back the real string instead of nothing or mojibake.
/// One entry per *distinct* glyph id in `glyphs` — a glyph reused across a run states its text
/// once, at whichever occurrence set it, the same rule a dictionary lookup already gives for
/// free by keying on the code.
pub fn to_unicode_entries(glyphs: &[PdfGlyph]) -> Vec<(u16, String)> {
    let mut map: BTreeMap<u16, String> = BTreeMap::new();
    for glyph in glyphs {
        map.entry(glyph.glyph_id)
            .or_insert_with(|| glyph.text.clone());
    }
    map.into_iter().collect()
}
