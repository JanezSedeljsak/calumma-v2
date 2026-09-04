use calumma_core::vector::{VectorItem, VectorShape};
use calumma_core::{BlendMode, Document, Shape, Tool};
use calumma_io::{encode_pdf, pdf_page_size, PDF_DEFAULT_DPI};
use flate2::read::ZlibDecoder;
use std::io::Read;

const SIDE: u32 = 64;

fn doc() -> Document {
    let mut doc = Document::new("p".into(), "t", SIDE, SIDE);
    doc.resize_viewport(SIDE as f32, SIDE as f32, 1.0);
    doc.fit_to_view();
    doc
}

fn rect(start: (f32, f32), end: (f32, f32), color: [u8; 4]) -> VectorItem {
    VectorItem::Shape(VectorShape {
        shape: Shape {
            tool: Tool::Rect,
            start,
            end,
            half_width: 1.0,
            fill: true,
            stroke: false,
        },
        stroke_color: color,
        color,
    })
}

fn painted_layer(doc: &mut Document, color: [u8; 4]) -> usize {
    doc.add_layer("Painted");
    let index = doc.layers.len() - 1;
    let tiles = doc.layers[index].tiles_mut().unwrap();
    for y in 10..20 {
        for x in 4..14 {
            let dim = (x + y) % 2 == 1;
            let shade = |c: u8| if dim { c / 2 } else { c };
            tiles.set_pixel(
                x,
                y,
                [shade(color[0]), shade(color[1]), shade(color[2]), 255],
            );
        }
    }
    index
}

/// PDF dictionaries are ASCII and always precede their stream's binary payload, so a lossy
/// read is enough to assert on them. Anything inside a content stream needs [`streams`].
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Every `/FlateDecode` stream in the file, inflated and concatenated. Content streams are
/// compressed, so the path operators a test wants to see are not in the raw bytes.
fn streams(pdf: &[u8]) -> String {
    const OPEN: &[u8] = b"stream\n";
    const CLOSE: &[u8] = b"\nendstream";
    let mut out = String::new();
    let mut at = 0;
    while let Some(found) = find(&pdf[at..], OPEN) {
        let start = at + found + OPEN.len();
        let Some(end) = find(&pdf[start..], CLOSE) else {
            break;
        };
        let mut decoded = Vec::new();
        if ZlibDecoder::new(&pdf[start..start + end])
            .read_to_end(&mut decoded)
            .is_ok()
        {
            out.push_str(&String::from_utf8_lossy(&decoded));
            out.push('\n');
        }
        at = start + end + CLOSE.len();
    }
    out
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// A reader locates every object through the cross-reference table, so a byte offset that
/// does not land on its own `N 0 obj` makes the file unopenable however good the rest is.
/// This is the one structural invariant worth asserting directly — and it has to be checked
/// against *bytes*, because a lossy string read renumbers every offset past the first
/// non-UTF-8 byte of compressed stream data.
fn assert_xref_resolves(pdf: &[u8]) {
    let marker = find(pdf, b"startxref").expect("startxref");
    let tail = text(&pdf[marker + "startxref".len()..]);
    let offset: usize = tail
        .split_whitespace()
        .next()
        .expect("startxref offset")
        .parse()
        .expect("numeric startxref");
    assert!(pdf[offset..].starts_with(b"xref"), "startxref misses xref");

    let table = text(&pdf[offset..]);
    let count: usize = table
        .lines()
        .nth(1)
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|n| n.parse().ok())
        .expect("xref subsection header");
    for (index, line) in table.lines().skip(2).take(count).enumerate().skip(1) {
        let at: usize = line.split_whitespace().next().unwrap().parse().unwrap();
        let expected = format!("{index} 0 obj");
        assert!(
            pdf[at..].starts_with(expected.as_bytes()),
            "xref entry {index} points at {:?}",
            text(&pdf[at..(at + 16).min(pdf.len())])
        );
    }
}

#[test]
fn a_pdf_is_well_formed_and_single_page() {
    let mut doc = doc();
    painted_layer(&mut doc, [200, 40, 60, 255]);
    let pdf = encode_pdf(&doc, PDF_DEFAULT_DPI);
    let body = text(&pdf);

    assert!(body.starts_with("%PDF-1.7"));
    assert!(body.trim_end().ends_with("%%EOF"));
    assert!(body.contains("/Type /Catalog"));
    assert!(body.contains("/Type /Pages"));
    assert!(body.contains("/Count 1"));
    assert_xref_resolves(&pdf);
}

#[test]
fn the_page_box_is_the_document_at_seventy_two_dpi() {
    let doc = doc();
    let pdf = text(&encode_pdf(&doc, PDF_DEFAULT_DPI));
    let side = f32::from(SIDE as u16);
    assert!(
        pdf.contains(&format!("/MediaBox [0 0 {side:.4} {side:.4}]")),
        "missing 1px-per-point MediaBox"
    );
}

/// Doubling the DPI has to halve the page while keeping the same pixels — the whole point of
/// the knob is printing a large board at a sensible physical size.
#[test]
fn a_higher_dpi_shrinks_the_page_not_the_content() {
    assert_eq!(pdf_page_size(144, 144, PDF_DEFAULT_DPI), (144.0, 144.0));
    assert_eq!(pdf_page_size(144, 144, 144.0), (72.0, 72.0));
    let doc = doc();
    let pdf = text(&encode_pdf(&doc, 144.0));
    let half = f32::from(SIDE as u16) / 2.0;
    assert!(pdf.contains(&format!("/MediaBox [0 0 {half:.4} {half:.4}]")));
}

/// The reason PDF is worth having: a rectangle exports as a rectangle. `re` is the PDF path
/// operator, so finding it proves the shape did not go out as a picture of itself.
#[test]
fn a_vector_rect_exports_as_a_path_not_an_image() {
    let mut doc = doc();
    doc.add_vector_layer(
        "Shapes",
        rect((8.0, 8.0), (40.0, 30.0), [10, 120, 200, 255]),
    );
    let content = streams(&encode_pdf(&doc, PDF_DEFAULT_DPI));
    assert!(
        content.contains("8 8 32 22 re"),
        "rect not emitted as a path"
    );
    assert!(
        content.contains("0.0392 0.4706 0.7843 rg"),
        "fill colour missing"
    );
}

#[test]
fn an_ellipse_becomes_bezier_curves() {
    let mut doc = doc();
    let mut item = rect((0.0, 0.0), (40.0, 20.0), [0, 0, 0, 255]);
    if let VectorItem::Shape(s) = &mut item {
        s.shape.tool = Tool::Ellipse;
    }
    doc.add_vector_layer("Shapes", item);
    let content = streams(&encode_pdf(&doc, PDF_DEFAULT_DPI));
    assert_eq!(
        content.matches(" c ").count(),
        4,
        "expected four quarter arcs"
    );
}

/// Opacity and blend mode survive as real PDF graphics state instead of being baked into
/// pixels — the row of the layer model PDF matches most exactly.
#[test]
fn opacity_and_blend_mode_ride_an_extgstate() {
    let mut doc = doc();
    let index = painted_layer(&mut doc, [200, 40, 60, 255]);
    doc.layers[index].opacity = 0.5;
    doc.layers[index].blend_mode = BlendMode::Multiply;
    let pdf = text(&encode_pdf(&doc, PDF_DEFAULT_DPI));
    assert!(pdf.contains("/Type /ExtGState"));
    assert!(pdf.contains("/ca 0.5000"));
    assert!(pdf.contains("/BM /Multiply"));
}

/// PDF images carry no alpha channel, so a layer with soft edges only composites correctly if
/// its alpha went out as a separate `/SMask` image.
#[test]
fn a_partly_transparent_raster_layer_carries_an_smask() {
    let mut doc = doc();
    doc.add_layer("Soft");
    let index = doc.layers.len() - 1;
    let tiles = doc.layers[index].tiles_mut().unwrap();
    for y in 10..20i32 {
        for x in 4..14i32 {
            tiles.set_pixel(x, y, [200, 40, 60, (x * 20) as u8]);
        }
    }
    let pdf = text(&encode_pdf(&doc, PDF_DEFAULT_DPI));
    assert!(pdf.contains("/Subtype /Image"));
    assert!(pdf.contains("/ColorSpace /DeviceRGB"));
    assert!(pdf.contains("/SMask"));
    assert!(pdf.contains("/ColorSpace /DeviceGray"));
}

/// The other half of the same rule: an ink box that is fully opaque needs no soft mask, and
/// emitting one anyway would cost a second image for nothing. Paper is exactly this case.
#[test]
fn a_fully_opaque_layer_needs_no_smask() {
    let mut doc = doc();
    doc.layers.truncate(1);
    let pdf = text(&encode_pdf(&doc, PDF_DEFAULT_DPI));
    assert!(pdf.contains("/Subtype /Image"), "Paper should export");
    assert!(!pdf.contains("/SMask"), "opaque layer emitted a soft mask");
}

#[test]
fn a_hidden_layer_is_not_exported() {
    let mut doc = doc();
    let index = painted_layer(&mut doc, [200, 40, 60, 255]);
    let visible = text(&encode_pdf(&doc, PDF_DEFAULT_DPI))
        .matches("/Subtype /Image")
        .count();
    doc.layers[index].visible = false;
    let hidden = text(&encode_pdf(&doc, PDF_DEFAULT_DPI))
        .matches("/Subtype /Image")
        .count();
    assert!(hidden < visible, "hidden layer still emitted an image");
}

#[test]
fn an_empty_document_still_produces_a_readable_pdf() {
    let pdf = encode_pdf(&doc(), PDF_DEFAULT_DPI);
    assert!(text(&pdf).contains("/Type /Page "));
    assert_xref_resolves(&pdf);
}

fn text_layer(doc: &mut Document, text: &str) -> usize {
    doc.tool = Tool::Text;
    doc.text_style.size = 20.0;
    let (sx, sy) = doc.camera.to_screen(4.0, 8.0);
    doc.pointer_down(sx, sy);
    doc.text_insert(text);
    doc.commit_text();
    doc.layers.len() - 1
}

/// The reason any of this exists: real, selectable text instead of a picture of it. `BT`/`Tj`
/// in the content stream and no extra image XObject for the layer are the two halves of that.
#[test]
fn a_text_layer_exports_as_real_text_not_an_image() {
    let mut doc = doc();
    let baseline_images = text(&encode_pdf(&doc, PDF_DEFAULT_DPI))
        .matches("/Subtype /Image")
        .count();

    text_layer(&mut doc, "Hi");
    let pdf = encode_pdf(&doc, PDF_DEFAULT_DPI);
    let images = text(&pdf).matches("/Subtype /Image").count();
    assert_eq!(images, baseline_images, "no new image for the text layer");

    let content = streams(&pdf);
    assert!(content.contains("BT"), "missing a text-showing block");
    assert!(content.contains("Tj"), "missing a glyph-showing operator");
    assert_xref_resolves(&pdf);
}

/// A `Type0`/`CIDFontType2` pair over an embedded `FontFile2`, with a `ToUnicode` CMap so a
/// reader's "copy text" gives back the real string rather than nothing.
#[test]
fn a_text_layer_embeds_a_real_font_with_a_tounicode_cmap() {
    let mut doc = doc();
    text_layer(&mut doc, "Hi");
    let pdf = text(&encode_pdf(&doc, PDF_DEFAULT_DPI));

    assert!(
        pdf.contains("/Subtype /Type0"),
        "missing the composite font"
    );
    assert!(
        pdf.contains("/Subtype /CIDFontType2"),
        "missing the descendant font"
    );
    assert!(pdf.contains("/Encoding /Identity-H"));
    assert!(pdf.contains("/CIDToGIDMap /Identity"));
    assert!(
        pdf.contains("/FontFile2"),
        "the font program was not embedded"
    );
    assert!(pdf.contains("/ToUnicode"));
}

/// The CMap is what makes the exported text worth having at all — assert the actual mapping
/// rather than just its presence, by inflating the CMap stream and reading the hex pairs back.
#[test]
fn the_tounicode_cmap_names_the_letters_actually_typed() {
    let mut doc = doc();
    text_layer(&mut doc, "AB");
    let content = streams(&encode_pdf(&doc, PDF_DEFAULT_DPI));
    assert!(content.contains("beginbfchar"), "no CMap body decoded");
    // "A" is U+0041, "B" is U+0042 — their UTF-16BE hex forms have to appear as a CMap value.
    assert!(
        content.contains("<0041>"),
        "missing the mapping for 'A': {content}"
    );
    assert!(
        content.contains("<0042>"),
        "missing the mapping for 'B': {content}"
    );
}

/// The real-text path is opaque-only for now; a translucent ink color falls back to the
/// rasterized image rather than silently rendering fully opaque.
#[test]
fn translucent_text_falls_back_to_a_rasterized_image() {
    let mut doc = doc();
    let baseline_images = text(&encode_pdf(&doc, PDF_DEFAULT_DPI))
        .matches("/Subtype /Image")
        .count();
    let index = text_layer(&mut doc, "Hi");
    if let Some(run) = doc.layers[index].content.run_mut() {
        run.color = [0, 0, 0, 128];
    }
    calumma_core::text_layer::resync(&mut doc.layers[index]);

    let pdf = encode_pdf(&doc, PDF_DEFAULT_DPI);
    let images = text(&pdf).matches("/Subtype /Image").count();
    assert!(
        images > baseline_images,
        "should have fallen back to a rasterized image (translucent ink also means an /SMask)"
    );
    assert!(
        !text(&pdf).contains("/Subtype /Type0"),
        "should not have also embedded a font for the fallback"
    );
}
