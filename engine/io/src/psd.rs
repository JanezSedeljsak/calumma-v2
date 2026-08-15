use calumma_core::{BlendMode, Document};
use rayon::prelude::*;

const SIGNATURE: &[u8; 4] = b"8BPS";
const BLEND_SIGNATURE: &[u8; 4] = b"8BIM";
const CHANNEL_COUNT: u16 = 4;

fn u16be(v: u16) -> [u8; 2] {
    v.to_be_bytes()
}

fn u32be(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

fn i32be(v: i32) -> [u8; 4] {
    v.to_be_bytes()
}

fn blend_key(mode: BlendMode) -> &'static [u8; 4] {
    match mode {
        BlendMode::Normal => b"norm",
        BlendMode::Multiply => b"mul ",
        BlendMode::Screen => b"scrn",
    }
}

fn pascal_name(name: &str) -> Vec<u8> {
    let bytes: Vec<u8> = name.bytes().take(255).collect();
    let mut out = Vec::with_capacity(1 + bytes.len());
    out.push(bytes.len() as u8);
    out.extend_from_slice(&bytes);
    while out.len() % 4 != 0 {
        out.push(0);
    }
    out
}

struct Planes {
    r: Vec<u8>,
    g: Vec<u8>,
    b: Vec<u8>,
    a: Vec<u8>,
}

fn split_planes(rgba: &[u8], pixel_count: usize) -> Planes {
    let mut planes = Planes {
        r: Vec::with_capacity(pixel_count),
        g: Vec::with_capacity(pixel_count),
        b: Vec::with_capacity(pixel_count),
        a: Vec::with_capacity(pixel_count),
    };
    for px in rgba.chunks_exact(4) {
        planes.r.push(px[0]);
        planes.g.push(px[1]);
        planes.b.push(px[2]);
        planes.a.push(px[3]);
    }
    planes
}

struct PreparedLayer<'a> {
    name: &'a str,
    visible: bool,
    opacity: f32,
    blend_mode: BlendMode,
    planes: Planes,
}

fn layer_record(layer: &PreparedLayer, width: u32, height: u32, pixel_count: usize) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&i32be(0));
    out.extend_from_slice(&i32be(0));
    out.extend_from_slice(&i32be(height as i32));
    out.extend_from_slice(&i32be(width as i32));
    out.extend_from_slice(&u16be(CHANNEL_COUNT));

    let channel_data_len = 2 + pixel_count as u32;
    for id in [0i16, 1, 2, -1] {
        out.extend_from_slice(&(id as u16).to_be_bytes());
        out.extend_from_slice(&u32be(channel_data_len));
    }

    out.extend_from_slice(BLEND_SIGNATURE);
    out.extend_from_slice(blend_key(layer.blend_mode));
    out.push((layer.opacity.clamp(0.0, 1.0) * 255.0).round() as u8);
    out.push(0);
    out.push(if layer.visible { 0 } else { 0x02 });
    out.push(0);

    let mut extra = Vec::new();
    extra.extend_from_slice(&u32be(0));
    extra.extend_from_slice(&u32be(0));
    extra.extend_from_slice(&pascal_name(layer.name));
    out.extend_from_slice(&u32be(extra.len() as u32));
    out.extend_from_slice(&extra);

    out
}

pub fn encode(doc: &Document) -> Vec<u8> {
    let width = doc.width.max(1);
    let height = doc.height.max(1);
    let pixel_count = (width as usize) * (height as usize);

    let prepared: Vec<PreparedLayer> = doc
        .layers
        .par_iter()
        .enumerate()
        .filter_map(|(index, layer)| {
            if layer.tiles().is_none() && layer.content.items().is_none() {
                return None;
            }
            let (w, h, rgba) = doc.layer_rgba(index)?;
            if w != width || h != height {
                return None;
            }
            Some(PreparedLayer {
                name: layer.name.as_str(),
                visible: layer.visible,
                opacity: layer.opacity,
                blend_mode: layer.blend_mode,
                planes: split_planes(&rgba, pixel_count),
            })
        })
        .collect();

    let mut layer_info = Vec::new();
    layer_info.extend_from_slice(&u16be(prepared.len() as u16));
    for layer in &prepared {
        layer_info.extend_from_slice(&layer_record(layer, width, height, pixel_count));
    }
    for layer in &prepared {
        for plane in [
            &layer.planes.r,
            &layer.planes.g,
            &layer.planes.b,
            &layer.planes.a,
        ] {
            layer_info.extend_from_slice(&u16be(0));
            layer_info.extend_from_slice(plane);
        }
    }
    if layer_info.len() % 2 != 0 {
        layer_info.push(0);
    }

    let mut layer_mask_info = Vec::new();
    layer_mask_info.extend_from_slice(&u32be(layer_info.len() as u32));
    layer_mask_info.extend_from_slice(&layer_info);
    layer_mask_info.extend_from_slice(&u32be(0));

    let (_, _, composite) = doc.composite_rgba();
    let composite_planes = split_planes(&composite, pixel_count);

    let mut out = Vec::new();
    out.extend_from_slice(SIGNATURE);
    out.extend_from_slice(&u16be(1));
    out.extend_from_slice(&[0u8; 6]);
    out.extend_from_slice(&u16be(CHANNEL_COUNT));
    out.extend_from_slice(&u32be(height));
    out.extend_from_slice(&u32be(width));
    out.extend_from_slice(&u16be(8));
    out.extend_from_slice(&u16be(3));

    out.extend_from_slice(&u32be(0));
    out.extend_from_slice(&u32be(0));

    out.extend_from_slice(&u32be(layer_mask_info.len() as u32));
    out.extend_from_slice(&layer_mask_info);

    out.extend_from_slice(&u16be(0));
    out.extend_from_slice(&composite_planes.r);
    out.extend_from_slice(&composite_planes.g);
    out.extend_from_slice(&composite_planes.b);
    out.extend_from_slice(&composite_planes.a);

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use calumma_core::Document;

    fn read_u16(bytes: &[u8], at: usize) -> u16 {
        u16::from_be_bytes([bytes[at], bytes[at + 1]])
    }

    fn read_u32(bytes: &[u8], at: usize) -> u32 {
        u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    }

    #[test]
    fn header_matches_document_dimensions() {
        let doc = Document::new("p".into(), "t", 64, 32);
        let bytes = encode(&doc);
        assert_eq!(&bytes[0..4], b"8BPS");
        assert_eq!(read_u16(&bytes, 4), 1);
        assert_eq!(read_u16(&bytes, 12), CHANNEL_COUNT);
        assert_eq!(read_u32(&bytes, 14), 32);
        assert_eq!(read_u32(&bytes, 18), 64);
        assert_eq!(read_u16(&bytes, 22), 8);
        assert_eq!(read_u16(&bytes, 24), 3);
    }

    #[test]
    fn layer_count_matches_raster_layers() {
        let mut doc = Document::new("p".into(), "t", 8, 8);
        doc.add_layer("Extra");
        let bytes = encode(&doc);
        let layer_mask_info_len = read_u32(&bytes, 34) as usize;
        assert!(layer_mask_info_len > 0);
        let layer_info_len = read_u32(&bytes, 38) as usize;
        assert!(layer_info_len > 0);
        let layer_count = read_u16(&bytes, 42);
        assert_eq!(layer_count, 3);
    }

    /// A vector layer has no tiles of its own, so it used to fall out of the export entirely.
    /// PSD has no shape layer in this writer, but dropping the artwork is worse than
    /// rasterizing it.
    #[test]
    fn a_vector_layer_reaches_the_psd_as_pixels() {
        use calumma_core::vector::{VectorItem, VectorShape};
        use calumma_core::{Shape, Tool};

        let mut doc = Document::new("p".into(), "t", 32, 32);
        let flat = encode(&doc);
        let flat_layers = read_u16(&flat, 42);

        let index = doc.add_vector_layer("Shapes");
        *doc.layers[index].content.items_mut().unwrap() = vec![VectorItem::Shape(VectorShape {
            shape: Shape {
                tool: Tool::Rect,
                start: (4.0, 4.0),
                end: (20.0, 20.0),
                half_width: 1.0,
                fill: true,
            },
            color: [255, 0, 0, 255],
        })];
        let bytes = encode(&doc);
        assert_eq!(read_u16(&bytes, 42), flat_layers + 1);
        assert!(bytes.len() > flat.len());
    }

    #[test]
    fn output_is_non_trivial_and_reasonably_sized() {
        let doc = Document::new("p".into(), "t", 16, 16);
        let bytes = encode(&doc);
        assert!(bytes.len() > 16 * 16 * 4 * 3);
    }
}
