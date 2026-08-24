use calumma_core::vector::{VectorItem, VectorShape};
use calumma_core::{BlendMode, Document, Shape, Tool};
use calumma_io::{decode_png_rgba, encode_svg};

const SIDE: u32 = 64;

struct RgbaImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl RgbaImage {
    fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn get_pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y as usize) * self.width as usize + x as usize) * 4;
        [
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]
    }
}

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

fn vector_layer(doc: &mut Document, item: VectorItem) -> usize {
    doc.add_vector_layer("Shapes", item)
}

/// Ten by ten of *varying* pixels: a layer painted in one flat color leaves as a `<rect>`
/// instead (see `a_flat_fill_is_a_rect_not_a_bitmap`), so a bitmap test needs ink that a
/// rectangle cannot stand in for.
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

fn filled_layer(doc: &mut Document, color: [u8; 4]) -> usize {
    doc.add_layer("Flat");
    let index = doc.layers.len() - 1;
    let tiles = doc.layers[index].tiles_mut().unwrap();
    for y in 30..36 {
        for x in 30..36 {
            tiles.set_pixel(x, y, color);
        }
    }
    index
}

/// The `<image>` payloads back into pixels, so "the raster half is embedded correctly" is
/// checked against a real PNG decode rather than against the string that claims to hold one.
fn embedded_images(svg: &str) -> Vec<(u32, u32, RgbaImage)> {
    let mut out = Vec::new();
    for chunk in svg.split("<image ").skip(1) {
        let x: u32 = attr(chunk, "x").parse().unwrap();
        let y: u32 = attr(chunk, "y").parse().unwrap();
        let data = attr(chunk, "href");
        let payload = data
            .strip_prefix("data:image/png;base64,")
            .expect("data uri");
        let bytes = base64_decode(payload);
        let (width, height, pixels) = decode_png_rgba(&bytes).expect("embedded png decodes");
        out.push((
            x,
            y,
            RgbaImage {
                width,
                height,
                pixels,
            },
        ));
    }
    out
}

fn attr(chunk: &str, name: &str) -> String {
    let key = format!("{name}=\"");
    let start = chunk
        .find(&key)
        .unwrap_or_else(|| panic!("no {name} in {chunk:.120}"))
        + key.len();
    let rest = &chunk[start..];
    rest[..rest.find('"').unwrap()].to_string()
}

fn base64_decode(text: &str) -> Vec<u8> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bits = 0u32;
    let mut count = 0;
    let mut out = Vec::new();
    for byte in text.bytes().filter(|b| *b != b'=') {
        let value = ALPHABET
            .iter()
            .position(|c| *c == byte)
            .expect("base64 char") as u32;
        bits = (bits << 6) | value;
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
        }
    }
    out
}

#[test]
fn the_document_svg_carries_the_board_size() {
    let svg = encode_svg(&doc());
    assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(svg.contains("width=\"64\" height=\"64\""));
    assert!(svg.contains("viewBox=\"0 0 64 64\""));
    assert!(svg.trim_end().ends_with("</svg>"));
}

#[test]
fn a_vector_layer_stays_geometry() {
    let mut doc = doc();
    vector_layer(&mut doc, rect((8.0, 8.0), (40.0, 24.0), [255, 0, 0, 255]));
    let svg = encode_svg(&doc);
    assert!(
        svg.contains("<rect x=\"8\" y=\"8\" width=\"32\" height=\"16\""),
        "the rect should survive as a rect: {svg:.400}"
    );
    assert!(svg.contains("fill=\"rgb(255,0,0)\""));
    assert!(
        !svg.contains("data:image/png"),
        "neither the shapes nor the flat paper needs a bitmap"
    );
}

/// Paper is a document-sized field of solid white, and a flood fill makes more of them.
/// Embedding those as base64 would dwarf the rest of the file, so a layer painted in exactly
/// one color leaves as a rectangle.
#[test]
fn a_flat_fill_is_a_rect_not_a_bitmap() {
    let mut doc = doc();
    filled_layer(&mut doc, [10, 20, 30, 255]);
    let svg = encode_svg(&doc);
    assert!(
        svg.contains("<rect x=\"30\" y=\"30\" width=\"6\" height=\"6\" fill=\"rgb(10,20,30)\""),
        "{svg:.400}"
    );
    assert!(!svg.contains("data:image/png"));
    assert!(
        svg.len() < 600,
        "a flat document stays tiny: {} bytes",
        svg.len()
    );
}

#[test]
fn a_painted_layer_is_embedded_as_a_cropped_png() {
    let mut doc = doc();
    painted_layer(&mut doc, [0, 128, 255, 255]);
    let svg = encode_svg(&doc);
    let images = embedded_images(&svg);
    let painted = images.last().expect("the painted layer is embedded");
    let (x, y, png) = painted;
    assert_eq!(
        (*x, *y),
        (4, 10),
        "the image is placed at its ink, not at 0,0"
    );
    assert_eq!(png.dimensions(), (10, 10), "and cropped to it");
    assert_eq!(png.get_pixel(0, 0), [0, 128, 255, 255]);
    assert_eq!(png.get_pixel(1, 0), [0, 64, 127, 255], "and it is not flat");
}

#[test]
fn an_empty_layer_contributes_nothing() {
    let paper_only = encode_svg(&doc());
    let mut doc = doc();
    doc.add_layer("Blank");
    assert_eq!(
        encode_svg(&doc),
        paper_only,
        "a layer with no ink adds nothing at all"
    );
}

#[test]
fn hidden_layers_are_left_out() {
    let mut doc = doc();
    let index = painted_layer(&mut doc, [255, 0, 0, 255]);
    let visible = encode_svg(&doc);
    doc.set_layer_visible(index, false);
    let hidden = encode_svg(&doc);
    assert_eq!(visible.matches("<image ").count(), 1);
    assert_eq!(
        hidden.matches("<image ").count(),
        0,
        "only paper is left, and it is a rect"
    );
}

#[test]
fn opacity_and_blend_mode_ride_along_as_svg_attributes() {
    let mut doc = doc();
    let index = painted_layer(&mut doc, [255, 0, 0, 255]);
    doc.set_layer_opacity(index, 0.5);
    doc.set_layer_blend_mode(index, BlendMode::Multiply);
    let svg = encode_svg(&doc);
    assert!(svg.contains("opacity=\"0.5\""));
    assert!(svg.contains("style=\"mix-blend-mode:multiply\""));

    doc.set_layer_blend_mode(index, BlendMode::Screen);
    assert!(encode_svg(&doc).contains("mix-blend-mode:screen"));
}

#[test]
fn a_layer_name_cannot_break_the_markup() {
    let mut doc = doc();
    let index = painted_layer(&mut doc, [1, 2, 3, 255]);
    doc.layers[index].name = "a<b>&\"c\"".to_string();
    let svg = encode_svg(&doc);
    assert!(svg.contains("id=\"a&lt;b&gt;&amp;&quot;c&quot;\""));
    assert_eq!(
        svg.matches("<g ").count(),
        svg.matches("</g>").count(),
        "groups stay balanced"
    );
}

#[test]
fn a_mixed_document_keeps_both_halves() {
    let mut doc = doc();
    painted_layer(&mut doc, [0, 255, 0, 255]);
    vector_layer(&mut doc, rect((2.0, 2.0), (20.0, 20.0), [0, 0, 255, 255]));
    let svg = encode_svg(&doc);
    assert!(svg.contains("<rect "), "vector layer stays vector");
    assert!(
        svg.contains("data:image/png;base64,"),
        "painted layer rides along"
    );
    assert!(!embedded_images(&svg).is_empty());
}
