//! Whole-file PSD export, read back with a byte-level walk over the layer records rather than
//! the fixed-offset spot checks `psd::tests` does inline — this is what proves the *sequence*
//! of fields (Pascal name, then the `'luni'` Unicode-name block) actually lines up, not just
//! that each one exists somewhere in the file.

use calumma_core::Document;
use calumma_io::encode_psd;

fn u16be(bytes: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([bytes[at], bytes[at + 1]])
}

fn u32be(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

struct LayerView {
    visible: bool,
    opacity: u8,
    blend: [u8; 4],
    pascal_name: String,
    unicode_name: Option<String>,
}

/// Walks every layer record in sequence, exactly mirroring the field order `psd::layer_record`
/// writes, so a test can assert on one layer's flags without hardcoding byte offsets that shift
/// whenever an earlier layer's name changes length.
fn parse_layers(bytes: &[u8]) -> Vec<LayerView> {
    let count = u16be(bytes, 42) as usize;
    let mut pos = 44;
    let mut out = Vec::with_capacity(count);

    for _ in 0..count {
        pos += 16; // top, left, bottom, right
        let channel_count = u16be(bytes, pos) as usize;
        pos += 2;
        pos += channel_count * 6; // (id: i16, data length: u32) per channel

        assert_eq!(&bytes[pos..pos + 4], b"8BIM", "blend signature");
        pos += 4;
        let blend: [u8; 4] = bytes[pos..pos + 4].try_into().unwrap();
        pos += 4;
        let opacity = bytes[pos];
        pos += 1;
        pos += 1; // clipping
        let flags = bytes[pos];
        pos += 1;
        pos += 1; // filler
        let extra_len = u32be(bytes, pos) as usize;
        pos += 4;

        let extra_start = pos;
        let extra_end = pos + extra_len;
        let mut ep = extra_start + 8; // mask data length + blend ranges length, both 0

        let pascal_len = bytes[ep] as usize;
        let pascal_name = String::from_utf8_lossy(&bytes[ep + 1..ep + 1 + pascal_len]).into_owned();
        let mut pascal_total = 1 + pascal_len;
        while pascal_total % 4 != 0 {
            pascal_total += 1;
        }
        ep += pascal_total;

        let mut unicode_name = None;
        if ep + 8 <= extra_end && &bytes[ep..ep + 4] == b"8BIM" && &bytes[ep + 4..ep + 8] == b"luni"
        {
            // ep+8 is the additional-layer-info record's own Length field (the byte size of
            // everything that follows); the Unicode string's *character* count is one more
            // u32 in, right before the UTF-16BE text itself.
            let unit_count = u32be(bytes, ep + 12) as usize;
            let data = &bytes[ep + 16..ep + 16 + unit_count * 2];
            let units: Vec<u16> = data
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            unicode_name = String::from_utf16(&units).ok();
        }

        out.push(LayerView {
            visible: flags & 0x02 == 0,
            opacity,
            blend,
            pascal_name,
            unicode_name,
        });
        pos = extra_end;
    }
    out
}

#[test]
fn a_unicode_layer_name_round_trips_through_the_luni_block() {
    let mut doc = Document::new("p".into(), "t", 16, 16);
    doc.add_layer("日本語レイヤー");
    let layers = parse_layers(&encode_psd(&doc));

    assert_eq!(
        layers.last().unwrap().unicode_name.as_deref(),
        Some("日本語レイヤー"),
        "the Unicode block must carry the name faithfully"
    );
}

/// The legacy Pascal name is 8-bit, so non-ASCII text mangles there — that is exactly the gap
/// the `'luni'` block above closes. Plain ASCII, though, still has to round-trip through the
/// old field too: it is what a reader with no Unicode support falls back to.
#[test]
fn an_ascii_layer_name_still_carries_the_legacy_pascal_string() {
    let mut doc = Document::new("p".into(), "t", 16, 16);
    doc.add_layer("Sketch");
    let layers = parse_layers(&encode_psd(&doc));

    let sketch = layers.last().unwrap();
    assert_eq!(sketch.pascal_name, "Sketch");
    assert_eq!(sketch.unicode_name.as_deref(), Some("Sketch"));
}

#[test]
fn a_hidden_layer_sets_the_visibility_flag() {
    let mut doc = Document::new("p".into(), "t", 16, 16);
    doc.add_layer("Hideme");
    let index = doc.layers.len() - 1;
    doc.layers[index].visible = false;

    let layers = parse_layers(&encode_psd(&doc));
    assert!(!layers.last().unwrap().visible, "flag bit 0x02 was not set");
}

#[test]
fn opacity_is_packed_into_a_single_byte_out_of_two_fifty_five() {
    let mut doc = Document::new("p".into(), "t", 16, 16);
    doc.add_layer("Half");
    let index = doc.layers.len() - 1;
    doc.layers[index].opacity = 0.5;

    let layers = parse_layers(&encode_psd(&doc));
    let opacity = layers.last().unwrap().opacity;
    assert!(
        (120..=136).contains(&opacity),
        "0.5 opacity should round to ~128/255, got {opacity}"
    );
}

#[test]
fn blend_mode_writes_the_matching_four_byte_key() {
    use calumma_core::BlendMode;

    let mut doc = Document::new("p".into(), "t", 16, 16);
    doc.add_layer("Blended");
    let index = doc.layers.len() - 1;

    doc.layers[index].blend_mode = BlendMode::Multiply;
    assert_eq!(
        &parse_layers(&encode_psd(&doc)).last().unwrap().blend,
        b"mul "
    );

    doc.layers[index].blend_mode = BlendMode::Screen;
    assert_eq!(
        &parse_layers(&encode_psd(&doc)).last().unwrap().blend,
        b"scrn"
    );
}
