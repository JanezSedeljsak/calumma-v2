//! `text_pdf`'s own content-stream and CMap generation, isolated from `io::pdf`'s file-structure
//! plumbing. `io/tests/pdf_export.rs` already proves the whole pipeline produces a readable PDF;
//! these instead pin the exact shape of what `runs_pdf`/`to_unicode_cmap`/`glyph_ids_for`/
//! `widths_array` emit, using real `PdfRun`s from `layout_for_pdf` since `PdfFontKey` cannot be
//! fabricated outside `calumma_text`.

use calumma_core::text_pdf;
use calumma_core::{layout_for_pdf, PdfFontKey, PdfRun, SpanStyle, TextRun};
use std::collections::BTreeMap;

fn run(text: &str) -> TextRun {
    TextRun {
        text: text.to_string(),
        size: 24.0,
        ..TextRun::default()
    }
    .clamped()
}

fn font_names(runs: &[PdfRun]) -> BTreeMap<PdfFontKey, String> {
    let mut keys: Vec<PdfFontKey> = runs.iter().map(|r| r.font).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .enumerate()
        .map(|(i, k)| (k, format!("F{i}")))
        .collect()
}

#[test]
fn to_unicode_cmap_chunks_at_a_hundred_entries() {
    let entries: Vec<(u16, String)> = (0..150u16).map(|gid| (gid, "x".to_string())).collect();
    let cmap = text_pdf::to_unicode_cmap(&entries);

    assert_eq!(
        cmap.matches("beginbfchar").count(),
        2,
        "150 entries need two blocks under the 100-entry cap: {cmap}"
    );
    assert!(cmap.contains("100 beginbfchar"), "{cmap}");
    assert!(cmap.contains("50 beginbfchar"), "{cmap}");
}

#[test]
fn to_unicode_cmap_hex_encodes_gid_and_utf16be_text() {
    let cmap = text_pdf::to_unicode_cmap(&[(0x0041, "A".to_string())]);
    assert!(cmap.contains("<0041> <0041>"), "{cmap}");
}

#[test]
fn widths_array_writes_gid_and_width_pairs() {
    let out = text_pdf::widths_array(&[(3, 600), (7, 250)]);
    assert_eq!(out, "3 [600] 7 [250] ");
}

/// Same font, two colours: the layer's style span splits `layout_for_pdf`'s output into two
/// `PdfRun`s sharing one `PdfFontKey`, and the content stream has to reissue `Tf`/`rg` for each
/// rather than only naming the font resource once and never changing colour.
#[test]
fn runs_pdf_reissues_tf_and_rg_per_colour_group() {
    let mut r = run("hello");
    r.apply_style(
        0,
        2,
        &SpanStyle {
            color: Some([255, 0, 0, 255]),
            ..SpanStyle::default()
        },
    );
    let runs = layout_for_pdf(&r).expect("some runs");
    assert!(runs.len() >= 2, "the coloured prefix should be its own run");

    let names = font_names(&runs);
    let content = text_pdf::runs_pdf(&runs, &names).expect("some glyphs");

    assert_eq!(content.matches(" Tf ").count(), runs.len(), "{content}");
    assert!(content.contains("1 0 0 rg"), "red group: {content}");
    assert!(
        content.contains("0 0 0 rg"),
        "default black group: {content}"
    );
    assert!(
        content.starts_with("BT\n") && content.trim_end().ends_with("ET"),
        "{content}"
    );
}

#[test]
fn runs_pdf_is_none_when_no_run_font_has_a_resource_name() {
    let runs = layout_for_pdf(&run("hi")).expect("some runs");
    assert_eq!(text_pdf::runs_pdf(&runs, &BTreeMap::new()), None);
}

/// The `/W` array must only cost as many entries as the layer actually draws — repeated letters
/// share a glyph id, so `glyph_ids_for` has to dedupe as well as sort.
#[test]
fn glyph_ids_for_dedupes_and_sorts_repeated_letters() {
    let runs = layout_for_pdf(&run("aabb")).expect("some runs");
    let key = runs[0].font;
    let ids = text_pdf::glyph_ids_for(&runs, key);

    assert_eq!(
        ids.len(),
        2,
        "'a' and 'b' are the only distinct glyphs: {ids:?}"
    );
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted, "already sorted");
}
