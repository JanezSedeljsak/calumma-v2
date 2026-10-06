//! Layered PSD import through the public `decode_psd`: this crate's own export read back, a
//! hand-built file with a layer smaller than the canvas, PackBits channel data, and the inputs
//! the decoder has to refuse without panicking.

use calumma_core::tile::DocRect;
use calumma_core::{BlendMode, Document};
use calumma_io::{decode_psd, encode_psd};

fn be16(v: u16) -> [u8; 2] {
    v.to_be_bytes()
}

fn be32(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

fn pascal_name(name: &str) -> Vec<u8> {
    let mut out = vec![name.len() as u8];
    out.extend_from_slice(name.as_bytes());
    while out.len() % 4 != 0 {
        out.push(0);
    }
    out
}

/// A minimal 8-bit RGB file with one layer of `width × height` at `(left, top)`, its three
/// colour channels written exactly as given — compression word included — so a test chooses
/// raw or PackBits per channel. Our own `encode_psd` always writes full-canvas, raw layers,
/// so it can never exercise either of those shapes.
fn one_layer_psd(
    canvas: u32,
    (left, top): (i32, i32),
    (width, height): (u32, u32),
    channels: [Vec<u8>; 3],
) -> Vec<u8> {
    let mut layer_info = Vec::new();
    layer_info.extend_from_slice(&be16(1));
    let mut record = Vec::new();
    record.extend_from_slice(&(top).to_be_bytes());
    record.extend_from_slice(&(left).to_be_bytes());
    record.extend_from_slice(&(top + height as i32).to_be_bytes());
    record.extend_from_slice(&(left + width as i32).to_be_bytes());
    record.extend_from_slice(&be16(3));
    for (id, data) in channels.iter().enumerate() {
        record.extend_from_slice(&(id as u16).to_be_bytes());
        record.extend_from_slice(&be32(data.len() as u32));
    }
    record.extend_from_slice(b"8BIM");
    record.extend_from_slice(b"norm");
    record.extend_from_slice(&[255, 0, 0, 0]);
    let mut extra = Vec::new();
    extra.extend_from_slice(&be32(0));
    extra.extend_from_slice(&be32(0));
    extra.extend_from_slice(&pascal_name("Patch"));
    record.extend_from_slice(&be32(extra.len() as u32));
    record.extend_from_slice(&extra);
    layer_info.extend_from_slice(&record);
    for data in &channels {
        layer_info.extend_from_slice(data);
    }
    if layer_info.len() % 2 != 0 {
        layer_info.push(0);
    }

    let mut layer_mask_info = Vec::new();
    layer_mask_info.extend_from_slice(&be32(layer_info.len() as u32));
    layer_mask_info.extend_from_slice(&layer_info);
    layer_mask_info.extend_from_slice(&be32(0));

    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"8BPS");
    bytes.extend_from_slice(&be16(1));
    bytes.extend_from_slice(&[0u8; 6]);
    bytes.extend_from_slice(&be16(3));
    bytes.extend_from_slice(&be32(canvas));
    bytes.extend_from_slice(&be32(canvas));
    bytes.extend_from_slice(&be16(8));
    bytes.extend_from_slice(&be16(3));
    bytes.extend_from_slice(&be32(0));
    bytes.extend_from_slice(&be32(0));
    bytes.extend_from_slice(&be32(layer_mask_info.len() as u32));
    bytes.extend_from_slice(&layer_mask_info);
    bytes.extend_from_slice(&be16(0));
    bytes.extend(vec![0u8; (canvas * canvas * 4) as usize]);
    bytes
}

fn raw_channel(value: u8, pixels: usize) -> Vec<u8> {
    let mut out = be16(0).to_vec();
    out.extend(vec![value; pixels]);
    out
}

/// A PackBits channel: the compression word, one row-length word per scanline (which the
/// decoder skips — it runs the stream continuously), then the packed stream itself.
fn packbits_channel(rows: u32, stream: &[u8]) -> Vec<u8> {
    let mut out = be16(1).to_vec();
    for _ in 0..rows {
        out.extend_from_slice(&be16(0));
    }
    out.extend_from_slice(stream);
    out
}

fn pixel(rgba: &[u8], canvas: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * canvas + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

#[test]
fn decoding_our_own_export_round_trips_every_layer() {
    let mut doc = Document::new("p".into(), "t", 12, 8);
    let full = DocRect::from_size(12, 8);
    doc.layers[0]
        .tiles_mut()
        .unwrap()
        .fill_uniform(full, [255, 255, 255, 255]);
    doc.add_layer("Sketch");
    let sketch = doc.active_layer;
    doc.layers[sketch].opacity = 0.5;
    doc.layers[sketch].blend_mode = BlendMode::Multiply;
    doc.layers[sketch].visible = false;
    doc.layers[sketch]
        .tiles_mut()
        .unwrap()
        .fill_uniform(full, [10, 20, 30, 128]);

    let decoded = decode_psd(&encode_psd(&doc)).expect("our own output must decode");
    assert_eq!(decoded.width, 12);
    assert_eq!(decoded.height, 8);
    // Document::new seeds Paper *and* a default "Layer 1" on its own, and the export doesn't
    // filter out that untouched empty layer, so the real stack is Paper, Layer 1, Sketch.
    assert_eq!(decoded.layers.len(), 3);

    assert_eq!(decoded.layers[0].name, "Paper");
    assert!(decoded.layers[0].visible);
    assert_eq!(decoded.layers[0].rgba[0..4], [255, 255, 255, 255]);

    let top = &decoded.layers[2];
    assert_eq!(top.name, "Sketch");
    assert!(!top.visible);
    assert!((top.opacity - 0.5).abs() < 1.0 / 255.0);
    assert_eq!(top.blend_mode, BlendMode::Multiply);
    assert_eq!(top.rgba[0..4], [10, 20, 30, 128]);
}

#[test]
fn decoding_preserves_non_ascii_layer_names_via_luni() {
    let mut doc = Document::new("p".into(), "t", 4, 4);
    doc.add_layer("日本語レイヤー");
    let decoded = decode_psd(&encode_psd(&doc)).expect("decodes");
    // Index 1 is the default "Layer 1" Document::new seeds on its own; ours is index 2.
    assert_eq!(decoded.layers[2].name, "日本語レイヤー");
}

#[test]
fn every_blend_mode_round_trips_through_its_psd_key() {
    let mut doc = Document::new("p".into(), "t", 4, 4);
    doc.add_layer("Blended");
    let index = doc.layers.len() - 1;
    let mut value = 0;
    while let Some(mode) = BlendMode::from_u32(value) {
        doc.layers[index].blend_mode = mode;
        let decoded = decode_psd(&encode_psd(&doc)).expect("decodes");
        assert_eq!(decoded.layers[index].blend_mode, mode, "{mode:?}");
        value += 1;
    }
}

/// Dissolve has no engine mode; a file using it must still import, as Normal, rather than fail.
#[test]
fn unmapped_blend_modes_fall_back_to_normal_rather_than_failing_the_import() {
    let mut doc = Document::new("p".into(), "t", 4, 4);
    doc.add_layer("Dissolved");
    let index = doc.layers.len() - 1;
    doc.layers[index].blend_mode = BlendMode::Multiply;
    let mut bytes = encode_psd(&doc);
    let at = bytes
        .windows(8)
        .position(|w| w == b"8BIMmul ")
        .expect("the multiply layer's blend key");
    bytes[at + 4..at + 8].copy_from_slice(b"diss");

    let decoded = decode_psd(&bytes).expect("an unknown blend key does not fail the import");
    assert_eq!(decoded.layers[index].blend_mode, BlendMode::Normal);
}

#[test]
fn decode_refuses_the_wrong_signature() {
    assert!(decode_psd(b"NOPE0000000000000000000000").is_none());
}

#[test]
fn decode_refuses_truncated_input_at_every_length_rather_than_panicking() {
    let doc = Document::new("p".into(), "t", 8, 8);
    let bytes = encode_psd(&doc);
    // A malformed or partially-downloaded file has to come back `None`, never a panic. Every
    // prefix length is worth checking rather than a handful of guesses, since an off-by-one
    // bounds check anywhere in the parser would only show up at one specific length.
    for len in 0..bytes.len() {
        let _ = decode_psd(&bytes[..len]);
    }
}

#[test]
fn decode_refuses_16_bit_depth_and_non_rgb_color_modes() {
    let doc = Document::new("p".into(), "t", 4, 4);
    let mut bytes = encode_psd(&doc);
    bytes[22..24].copy_from_slice(&be16(16));
    assert!(decode_psd(&bytes).is_none());

    let mut bytes = encode_psd(&doc);
    bytes[24..26].copy_from_slice(&be16(4));
    assert!(decode_psd(&bytes).is_none(), "CMYK");
}

#[test]
fn decode_refuses_the_psb_large_document_version() {
    let doc = Document::new("p".into(), "t", 4, 4);
    let mut bytes = encode_psd(&doc);
    bytes[4..6].copy_from_slice(&be16(2));
    assert!(decode_psd(&bytes).is_none());
}

#[test]
fn decode_places_a_layer_smaller_than_the_canvas_at_its_own_offset() {
    const CANVAS: u32 = 20;
    let (left, top, w, h) = (6, 4, 3u32, 5u32);
    let pixels = (w * h) as usize;
    let bytes = one_layer_psd(
        CANVAS,
        (left, top),
        (w, h),
        [
            raw_channel(200, pixels),
            raw_channel(0, pixels),
            raw_channel(0, pixels),
        ],
    );

    let decoded = decode_psd(&bytes).expect("a hand-built minimal PSD must still decode");
    assert_eq!(decoded.layers.len(), 1);
    let layer = &decoded.layers[0];
    assert_eq!(layer.name, "Patch");
    // No alpha channel was declared, so alpha defaults to fully opaque inside the rect.
    assert_eq!(
        pixel(&layer.rgba, CANVAS, left as u32 + 1, top as u32 + 1),
        [200, 0, 0, 255]
    );
    assert_eq!(pixel(&layer.rgba, CANVAS, 0, 0), [0, 0, 0, 0]);
}

/// PackBits' three control codes in one stream: a literal run, the `-128` no-op, and a repeat
/// run, stopping exactly at the channel's pixel count.
#[test]
fn packbits_channels_decode_literal_and_repeat_runs_and_skip_the_no_op() {
    const CANVAS: u32 = 8;
    let (w, h) = (3u32, 5u32);
    let red_stream = [2u8, 10, 20, 30, (-128i8) as u8, (-11i8) as u8, 7];
    let flat = [(-14i8) as u8, 0];
    let bytes = one_layer_psd(
        CANVAS,
        (0, 0),
        (w, h),
        [
            packbits_channel(h, &red_stream),
            packbits_channel(h, &flat),
            packbits_channel(h, &flat),
        ],
    );

    let decoded = decode_psd(&bytes).expect("PackBits channels decode");
    let rgba = &decoded.layers[0].rgba;
    assert_eq!(pixel(rgba, CANVAS, 0, 0), [10, 0, 0, 255]);
    assert_eq!(pixel(rgba, CANVAS, 1, 0), [20, 0, 0, 255]);
    assert_eq!(pixel(rgba, CANVAS, 2, 0), [30, 0, 0, 255]);
    for y in 1..h {
        for x in 0..w {
            assert_eq!(pixel(rgba, CANVAS, x, y), [7, 0, 0, 255], "({x}, {y})");
        }
    }
}
