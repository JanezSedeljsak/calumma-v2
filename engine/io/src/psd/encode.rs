use super::{blend_key, BLEND_SIGNATURE, CHANNEL_COUNT, SIGNATURE};
use calumma_core::{BlendMode, Document};
use rayon::prelude::*;

fn u16be(v: u16) -> [u8; 2] {
    v.to_be_bytes()
}

fn u32be(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

fn i32be(v: i32) -> [u8; 4] {
    v.to_be_bytes()
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

/// The legacy Pascal name above is 8-bit and gets mangled for anything outside ASCII —
/// Photoshop always also writes this `'luni'` additional-layer-info block and prefers it for
/// display whenever it's present, so it's the block that actually carries the name faithfully.
fn unicode_layer_name(name: &str) -> Vec<u8> {
    let units: Vec<u16> = name.encode_utf16().collect();
    let mut data = Vec::with_capacity(4 + units.len() * 2);
    data.extend_from_slice(&u32be(units.len() as u32));
    for unit in &units {
        data.extend_from_slice(&unit.to_be_bytes());
    }

    let mut block = Vec::with_capacity(12 + data.len());
    block.extend_from_slice(BLEND_SIGNATURE);
    block.extend_from_slice(b"luni");
    block.extend_from_slice(&u32be(data.len() as u32));
    block.extend_from_slice(&data);
    block
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
    extra.extend_from_slice(&unicode_layer_name(layer.name));
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
            if layer.tiles().is_none() && layer.content.item().is_none() {
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
