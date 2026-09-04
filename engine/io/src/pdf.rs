//! Whole-document PDF export.
//!
//! Structured, so it encodes in Rust beside `svg.rs` and `psd.rs` rather than in the shell —
//! `CGPDFContext` would only work if Swift re-emitted every shape, which is exactly what the
//! one rule forbids and what `vector_pdf` exists to prevent.
//!
//! PDF is the closest fit of any export format Calumma has, because the layer model has real
//! equivalents rather than approximations: `layer.opacity` is `/ca` and `/CA`, the three blend
//! modes are `/BM` names that map exactly, and a vector layer stays a vector layer. Masks and
//! adjustments are still baked, the same way `layer_rgba` bakes them everywhere else — a
//! `/SMask` for masks is what makes PDF the one format that could carry them live, but the
//! mask storage has to grow first.
use crate::flate::deflate;
use calumma_core::transform::bounds_center;
use calumma_core::{embed_font, layout_for_pdf, text_pdf, to_unicode_entries, vector_pdf};
use calumma_core::{BlendMode, Document, Layer, PdfFontKey};
use std::collections::BTreeMap;

/// PDF measures in points, 72 to the inch, so this is the DPI at which one document pixel is
/// one point. Exporting at a higher DPI keeps the same pixels and prints them smaller.
pub const PDF_DEFAULT_DPI: f32 = 72.0;

/// The page a document fills at `dpi`, in PDF points. A core decision rather than a shell
/// one, so nothing outside the engine multiplies pixels by a scale factor.
pub fn page_size(width: u32, height: u32, dpi: f32) -> (f32, f32) {
    let scale = PDF_DEFAULT_DPI / dpi.max(1.0);
    (width.max(1) as f32 * scale, height.max(1) as f32 * scale)
}

fn blend_name(mode: BlendMode) -> &'static str {
    match mode {
        BlendMode::Normal => "Normal",
        BlendMode::Multiply => "Multiply",
        BlendMode::Screen => "Screen",
    }
}

/// A PDF file is a header, a set of numbered objects, a cross-reference table of their byte
/// offsets, and a trailer pointing at the catalog. Small enough that a focused writer is less
/// code than a dependency, which is the same call `svg.rs` already made.
#[derive(Default)]
struct Pdf {
    body: Vec<u8>,
    offsets: Vec<usize>,
}

impl Pdf {
    fn new() -> Self {
        Self {
            body: b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec(),
            offsets: Vec::new(),
        }
    }

    /// Reserves an object number without writing it yet, so an object can reference one that
    /// is emitted later — a page has to name its content stream before the stream exists.
    fn reserve(&mut self) -> usize {
        self.offsets.push(0);
        self.offsets.len()
    }

    fn write(&mut self, id: usize, contents: &[u8]) {
        self.offsets[id - 1] = self.body.len();
        self.body
            .extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        self.body.extend_from_slice(contents);
        self.body.extend_from_slice(b"\nendobj\n");
    }

    fn write_stream(&mut self, id: usize, dict: &str, data: &[u8]) {
        let packed = deflate(data);
        let mut out = format!(
            "<< {dict} /Length {} /Filter /FlateDecode >>\nstream\n",
            packed.len()
        )
        .into_bytes();
        out.extend_from_slice(&packed);
        out.extend_from_slice(b"\nendstream");
        self.write(id, &out);
    }

    fn finish(mut self, root: usize) -> Vec<u8> {
        let xref_at = self.body.len();
        let count = self.offsets.len() + 1;
        self.body
            .extend_from_slice(format!("xref\n0 {count}\n0000000000 65535 f \n").as_bytes());
        for offset in &self.offsets {
            self.body
                .extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        self.body.extend_from_slice(
            format!("trailer\n<< /Size {count} /Root {root} 0 R >>\nstartxref\n{xref_at}\n%%EOF\n")
                .as_bytes(),
        );
        self.body
    }
}

/// One layer's graphics state: opacity and blend mode as a real `/ExtGState` rather than
/// baked into pixels. A vector layer folds its item's own alpha in here too — it holds
/// exactly one item (the 1:1 rule), so the two multiply cleanly and there is never a second
/// alpha to reconcile.
struct GraphicsState {
    fill_alpha: f32,
    stroke_alpha: f32,
    blend: BlendMode,
}

impl GraphicsState {
    fn dict(&self) -> String {
        format!(
            "<< /Type /ExtGState /ca {:.4} /CA {:.4} /BM /{} >>",
            self.fill_alpha.clamp(0.0, 1.0),
            self.stroke_alpha.clamp(0.0, 1.0),
            blend_name(self.blend)
        )
    }
}

fn layer_state(layer: &Layer) -> GraphicsState {
    let opacity = layer.opacity.clamp(0.0, 1.0);
    let (fill, stroke) = match layer.content.item() {
        Some(item) => (
            item.color()[3] as f32 / 255.0,
            item.stroke_color()[3] as f32 / 255.0,
        ),
        None => (1.0, 1.0),
    };
    GraphicsState {
        fill_alpha: opacity * fill,
        stroke_alpha: opacity * stroke,
        blend: layer.blend_mode,
    }
}

/// The tight box of everything a layer paints, so a mostly-empty stack does not embed a
/// full-page image per layer. Same crop `svg.rs` does, and for the same reason.
fn ink_bounds(rgba: &[u8], width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
    let (mut min_x, mut min_y) = (u32::MAX, u32::MAX);
    let (mut max_x, mut max_y) = (0u32, 0u32);
    for y in 0..height {
        let row = (y as usize) * (width as usize) * 4;
        for x in 0..width {
            if rgba[row + (x as usize) * 4 + 3] == 0 {
                continue;
            }
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    (min_x != u32::MAX).then(|| (min_x, min_y, max_x - min_x + 1, max_y - min_y + 1))
}

/// RGBA split into the two planes PDF wants: colour as `/DeviceRGB` samples, and alpha as a
/// separate `/DeviceGray` image referenced through `/SMask`. PDF images carry no alpha
/// channel of their own, so this split is not an optimisation — it is the only way a
/// transparent layer composites correctly.
fn split_planes(rgba: &[u8], width: u32, box_: (u32, u32, u32, u32)) -> (Vec<u8>, Vec<u8>, bool) {
    let (x, y, w, h) = box_;
    let pixels = (w as usize) * (h as usize);
    let mut color = Vec::with_capacity(pixels * 3);
    let mut alpha = Vec::with_capacity(pixels);
    let mut opaque = true;
    for row in 0..h {
        let start = (((y + row) as usize) * (width as usize) + x as usize) * 4;
        for col in 0..w as usize {
            let i = start + col * 4;
            color.extend_from_slice(&rgba[i..i + 3]);
            alpha.push(rgba[i + 3]);
            opaque &= rgba[i + 3] == u8::MAX;
        }
    }
    (color, alpha, opaque)
}

/// `Do` draws an image into the unit square with its own origin at the bottom-left, so the
/// `cm` here both scales the unit square to the ink box and flips it back — the page content
/// stream is already running in Calumma's top-left space, and the image would otherwise land
/// upside down.
fn image_placement(name: &str, box_: (u32, u32, u32, u32)) -> String {
    let (x, y, w, h) = box_;
    format!("q {w} 0 0 -{h} {x} {} cm /{name} Do Q", y.saturating_add(h))
}

pub fn encode(doc: &Document, dpi: f32) -> Vec<u8> {
    let mut pdf = Pdf::new();
    let catalog = pdf.reserve();
    let pages = pdf.reserve();
    let page = pdf.reserve();
    let contents = pdf.reserve();

    let mut ext_g_states = String::new();
    let mut xobjects = String::new();
    let mut fonts = String::new();
    let mut deferred: Vec<(usize, String, Vec<u8>)> = Vec::new();
    let mut body = String::new();

    let (page_w, page_h) = page_size(doc.width, doc.height, dpi);
    let scale = PDF_DEFAULT_DPI / dpi.max(1.0);
    // One flip at the top of the page instead of negating every emitted y: PDF measures y
    // upward from the bottom-left and Calumma downward from the top-left.
    body.push_str(&format!("{scale:.6} 0 0 -{scale:.6} 0 {page_h:.4} cm\n"));

    for (index, layer) in doc.layers.iter().enumerate() {
        if !layer.visible || layer.opacity <= 0.0 {
            continue;
        }
        let drawn = if let Some(item) = layer.content.item() {
            vector_pdf::item_pdf(item).map(|geometry| {
                let pivot = layer.content_bounds().map(bounds_center);
                let matrix = pivot.and_then(|p| vector_pdf::transform_matrix(p, layer.transform));
                format!("q {}{geometry} Q", matrix.unwrap_or_default())
            })
        } else if layer.is_text() {
            text_layer(layer, index, &mut pdf, &mut deferred, &mut fonts).or_else(|| {
                raster_layer(doc, index, &mut pdf, &mut deferred).map(|(id, box_)| {
                    let name = format!("Im{index}");
                    xobjects.push_str(&format!("/{name} {id} 0 R "));
                    image_placement(&name, box_)
                })
            })
        } else {
            raster_layer(doc, index, &mut pdf, &mut deferred).map(|(id, box_)| {
                let name = format!("Im{index}");
                xobjects.push_str(&format!("/{name} {id} 0 R "));
                image_placement(&name, box_)
            })
        };
        let Some(drawn) = drawn else {
            continue;
        };
        let state = pdf.reserve();
        let name = format!("GS{index}");
        pdf.write(state, layer_state(layer).dict().as_bytes());
        ext_g_states.push_str(&format!("/{name} {state} 0 R "));
        body.push_str(&format!("q /{name} gs {drawn} Q\n"));
    }

    for (id, dict, data) in deferred {
        pdf.write_stream(id, &dict, &data);
    }

    pdf.write(
        catalog,
        format!("<< /Type /Catalog /Pages {pages} 0 R >>").as_bytes(),
    );
    pdf.write(
        pages,
        format!("<< /Type /Pages /Kids [{page} 0 R] /Count 1 >>").as_bytes(),
    );
    pdf.write(
        page,
        format!(
            "<< /Type /Page /Parent {pages} 0 R /MediaBox [0 0 {page_w:.4} {page_h:.4}] \
/Resources << /ExtGState << {ext_g_states}>> /XObject << {xobjects}>> /Font << {fonts}>> >> \
/Contents {contents} 0 R >>"
        )
        .as_bytes(),
    );
    pdf.write_stream(contents, "", body.as_bytes());
    pdf.finish(catalog)
}

type RasterObject = (usize, (u32, u32, u32, u32));

fn raster_layer(
    doc: &Document,
    index: usize,
    pdf: &mut Pdf,
    deferred: &mut Vec<(usize, String, Vec<u8>)>,
) -> Option<RasterObject> {
    let (width, height, rgba) = doc.layer_rgba(index)?;
    let box_ = ink_bounds(&rgba, width, height)?;
    let (color, alpha, opaque) = split_planes(&rgba, width, box_);
    let (_, _, w, h) = box_;

    let smask = (!opaque).then(|| {
        let id = pdf.reserve();
        deferred.push((
            id,
            format!(
                "/Type /XObject /Subtype /Image /Width {w} /Height {h} \
/ColorSpace /DeviceGray /BitsPerComponent 8"
            ),
            alpha,
        ));
        id
    });

    let id = pdf.reserve();
    let mut dict = format!(
        "/Type /XObject /Subtype /Image /Width {w} /Height {h} \
/ColorSpace /DeviceRGB /BitsPerComponent 8"
    );
    if let Some(smask) = smask {
        dict.push_str(&format!(" /SMask {smask} 0 R"));
    }
    deferred.push((id, dict, color));
    Some((id, box_))
}

/// A text layer as real, selectable content-stream text rather than a picture of itself —
/// `None` falls the caller back to `raster_layer`, which is what happens whenever any font
/// this layer's text needs cannot be embedded (a CFF-outline face; see `calumma_text::pdf`'s
/// module doc) or carries a translucent ink color (the real-text path is opaque-only for now,
/// the one thing the rasterized path could do that this one cannot yet). Falling every run of
/// a layer back together, rather than dropping just the glyphs that would not embed, is
/// deliberate: invisible text is a worse failure than a slightly larger file.
fn text_layer(
    layer: &Layer,
    index: usize,
    pdf: &mut Pdf,
    deferred: &mut Vec<(usize, String, Vec<u8>)>,
    fonts: &mut String,
) -> Option<String> {
    let run = layer.run()?;
    let runs = layout_for_pdf(run)?;
    if runs.iter().any(|r| r.color[3] != 255) {
        return None;
    }

    let mut keys: Vec<PdfFontKey> = runs.iter().map(|r| r.font).collect();
    keys.sort();
    keys.dedup();

    let mut names: BTreeMap<PdfFontKey, String> = BTreeMap::new();
    for (i, key) in keys.into_iter().enumerate() {
        let glyph_ids = text_pdf::glyph_ids_for(&runs, key);
        let font = embed_font(key, &glyph_ids)?;
        let to_unicode: Vec<(u16, String)> = runs
            .iter()
            .filter(|r| r.font == key)
            .flat_map(|r| to_unicode_entries(&r.glyphs))
            .collect();
        let type0 = embed_text_font(pdf, deferred, &font, &to_unicode);
        let name = format!("TF{index}_{i}");
        fonts.push_str(&format!("/{name} {type0} 0 R "));
        names.insert(key, name);
    }

    let text = text_pdf::runs_pdf(&runs, &names)?;
    let pivot = layer.content_bounds().map(bounds_center)?;
    let matrix = vector_pdf::transform_matrix(pivot, layer.transform);
    Some(format!("q {}{text} Q", matrix.unwrap_or_default()))
}

/// The PDF objects one embedded font needs — a `/FontFile2` stream, a `/FontDescriptor`, a
/// `/CIDFontType2` descendant naming it, a `/ToUnicode` CMap stream, and the `/Type0` font
/// that ties them together — returning the `Type0` object's id, the one a `/Font` resource
/// entry and a content stream's `Tf` both name.
///
/// Metrics in the descriptor and the descendant font's `/W` are in PDF's fixed 1000-unit glyph
/// space, not the face's own `unitsPerEm` — `calumma_text::PdfFont::widths` is pre-scaled
/// already; the ascender/descender here are the two values that are not.
fn embed_text_font(
    pdf: &mut Pdf,
    deferred: &mut Vec<(usize, String, Vec<u8>)>,
    font: &calumma_core::PdfFont,
    to_unicode: &[(u16, String)],
) -> usize {
    let scale = 1000.0 / font.units_per_em.max(1) as f32;
    let ascent = (font.ascender as f32 * scale).round() as i32;
    let descent = (font.descender as f32 * scale).round() as i32;

    let file_id = pdf.reserve();
    deferred.push((
        file_id,
        format!("/Length1 {}", font.program.len()),
        font.program.clone(),
    ));

    let descriptor_id = pdf.reserve();
    pdf.write(
        descriptor_id,
        format!(
            "<< /Type /FontDescriptor /FontName /Embedded{file_id} /Flags 4 \
/FontBBox [0 {descent} 1000 {ascent}] /ItalicAngle 0 /Ascent {ascent} /Descent {descent} \
/CapHeight {ascent} /StemV 80 /FontFile2 {file_id} 0 R >>"
        )
        .as_bytes(),
    );

    let cid_font_id = pdf.reserve();
    pdf.write(
        cid_font_id,
        format!(
            "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /Embedded{file_id} \
/CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
/FontDescriptor {descriptor_id} 0 R /CIDToGIDMap /Identity /DW 1000 \
/W [{}] >>",
            text_pdf::widths_array(&font.widths)
        )
        .as_bytes(),
    );

    let to_unicode_id = pdf.reserve();
    pdf.write_stream(
        to_unicode_id,
        "",
        text_pdf::to_unicode_cmap(to_unicode).as_bytes(),
    );

    let type0_id = pdf.reserve();
    pdf.write(
        type0_id,
        format!(
            "<< /Type /Font /Subtype /Type0 /BaseFont /Embedded{file_id} \
/Encoding /Identity-H /DescendantFonts [{cid_font_id} 0 R] /ToUnicode {to_unicode_id} 0 R >>"
        )
        .as_bytes(),
    );
    type0_id
}
