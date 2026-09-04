//! Real, selectable PDF text — the glyph layout and the font it embeds. These run against
//! whatever fonts the machine actually has, the same way `text.rs`'s tests do, so assertions
//! are about structure (a glyph landed somewhere reasonable, a repackaged font still parses)
//! rather than exact pixel-for-pixel values a different font file would break.

use calumma_text::{embed_font, layout_for_pdf, TextRun};

fn run(text: &str) -> TextRun {
    TextRun {
        text: text.to_string(),
        size: 32.0,
        ..TextRun::default()
    }
    .clamped()
}

#[test]
fn empty_text_lays_out_to_nothing() {
    assert!(layout_for_pdf(&run("")).is_none());
}

#[test]
fn every_character_becomes_a_glyph() {
    let runs = layout_for_pdf(&run("hi")).expect("some runs");
    let total: usize = runs.iter().map(|r| r.glyphs.len()).sum();
    assert_eq!(total, 2, "two characters, two glyphs");
}

#[test]
fn glyphs_advance_left_to_right() {
    let runs = layout_for_pdf(&run("abc")).expect("some runs");
    let glyphs: Vec<_> = runs.iter().flat_map(|r| r.glyphs.iter()).collect();
    assert_eq!(glyphs.len(), 3);
    assert!(glyphs[1].x > glyphs[0].x, "b should sit right of a");
    assert!(glyphs[2].x > glyphs[1].x, "c should sit right of b");
}

/// Each glyph carries the exact slice of the source string it stands for, which is what makes
/// a correct `ToUnicode` CMap possible — get this wrong and copy-pasting the exported text
/// back out of a PDF reader gives you the wrong string.
#[test]
fn each_glyph_names_its_own_source_text() {
    let runs = layout_for_pdf(&run("hi")).expect("some runs");
    let glyphs: Vec<_> = runs.iter().flat_map(|r| r.glyphs.iter()).collect();
    assert_eq!(glyphs[0].text, "h");
    assert_eq!(glyphs[1].text, "i");
}

#[test]
fn a_span_color_override_splits_the_run() {
    let mut r = run("hello");
    r.apply_style(
        0,
        2,
        &calumma_text::SpanStyle {
            color: Some([255, 0, 0, 255]),
            ..calumma_text::SpanStyle::default()
        },
    );
    let runs = layout_for_pdf(&r).expect("some runs");
    assert!(runs.len() >= 2, "the coloured prefix should be its own run");
    assert_eq!(runs[0].color, [255, 0, 0, 255]);
    assert_ne!(runs.last().unwrap().color, [255, 0, 0, 255]);
}

/// The default UI family resolves to *some* installed font on every machine this runs on
/// (`fonts::default_family`), so this is a real embed, not a mock — either it is TrueType and
/// comes back repackaged and re-parseable, or it is a CFF face and `embed_font` correctly
/// says no rather than handing back something malformed.
#[test]
fn embedding_the_default_family_either_works_or_is_refused_cleanly() {
    let r = run("Calumma");
    let runs = layout_for_pdf(&r).expect("some runs");
    let key = runs[0].font;
    let mut glyph_ids: Vec<u16> = runs[0].glyphs.iter().map(|g| g.glyph_id).collect();
    glyph_ids.sort_unstable();
    glyph_ids.dedup();

    match embed_font(key, &glyph_ids) {
        None => {} // a CFF-outline default face — the disclosed, refused case.
        Some(font) => {
            assert!(font.units_per_em > 0);
            assert!(!font.program.is_empty());
            assert_eq!(
                font.widths.len(),
                glyph_ids.len(),
                "widths for exactly the glyphs this run used"
            );
            let parsed = ttf_parser::Face::parse(&font.program, 0)
                .expect("the repackaged program must still parse as a standalone sfnt");
            assert_eq!(parsed.units_per_em(), font.units_per_em);
            assert_eq!(parsed.ascender(), font.ascender);
            assert_eq!(parsed.descender(), font.descender);
        }
    }
}
