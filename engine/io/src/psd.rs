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

/// One layer decoded from a PSD file, already placed on the full canvas — `rgba` is
/// `width * height * 4` bytes, transparent everywhere outside the layer's own on-disk bounds.
/// Doing the placement here rather than handing back the layer's own (possibly smaller,
/// possibly offset) rect keeps every caller's life the same shape as `layer_rgba`/`place_image`
/// already assume: one canvas-sized buffer per layer, no separate offset to thread through.
pub struct DecodedLayer {
    #[allow(dead_code)]
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub rgba: Vec<u8>,
}

pub struct DecodedPsd {
    pub width: u32,
    pub height: u32,
    /// Bottom to top, matching `Document::layers`' own order — the first entry is what a
    /// caller should paint onto the project's existing bottom layer, the rest are new layers
    /// stacked above it in order.
    pub layers: Vec<DecodedLayer>,
}

/// A checked cursor over the file bytes. Every read can fail — this is parsing a file nobody
/// asked the engine to trust — so nothing here indexes or slices without a bounds check first;
/// a truncated or hostile PSD is refused with `None` at the point it stops making sense, never
/// panics.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let slice = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    fn skip(&mut self, n: usize) -> Option<()> {
        self.take(n).map(|_| ())
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    fn i8(&mut self) -> Option<i8> {
        self.u8().map(|b| b as i8)
    }

    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_be_bytes([b[0], b[1]]))
    }

    fn i16(&mut self) -> Option<i16> {
        self.u16().map(|v| v as i16)
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn i32(&mut self) -> Option<i32> {
        self.u32().map(|v| v as i32)
    }
}

fn blend_mode_from_key(key: &[u8]) -> BlendMode {
    match key {
        b"mul " => BlendMode::Multiply,
        b"scrn" => BlendMode::Screen,
        // Every blend mode PSD supports beyond the three the engine models (per `AGENTS.md`
        // STRICT SCOPE) — darken, overlay, hard light, and the rest — lands here rather than
        // failing the import: a layer with the wrong blend mode is still the right pixels in
        // the right place, which is worth more than refusing the whole file over one knob the
        // engine cannot represent.
        _ => BlendMode::Normal,
    }
}

/// PackBits, the only compression PSD channel data uses besides raw. A control byte `n`: `n
/// >= 0` copies the next `n + 1` bytes literally; `n < 0` (and not the no-op `-128`) repeats
/// the following single byte `1 - n` times. Two's-complement `i8` reads that straight off the
/// wire, which is why `Reader::i8` exists.
///
/// PSD additionally prefixes each scanline's compressed bytes with its own byte count (read by
/// the caller, one `u16`/`u32` per row) so a reader can skip a row without decompressing it.
/// This decoder does not need that shortcut — it always wants the whole channel — so it runs
/// PackBits as one continuous stream across every row's bytes back to back rather than
/// resetting at each row boundary, which decodes identically since no control code ever
/// straddles what would have been a row's end (Photoshop never emits one that does).
fn unpack_bits(reader: &mut Reader, out_len: usize) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(out_len);
    while out.len() < out_len {
        let n = reader.i8()?;
        if n >= 0 {
            let count = n as usize + 1;
            out.extend_from_slice(reader.take(count)?);
        } else if n != -128 {
            let count = 1 - n as isize;
            let byte = reader.u8()?;
            out.resize(out.len() + count as usize, byte);
        }
    }
    out.truncate(out_len);
    Some(out)
}

/// One channel's worth of pixels, `width * height` bytes, raw or PackBits per its own leading
/// compression word. `channel_len` is the on-disk byte count `layer_record` already declared
/// for this channel (data length minus the 2-byte compression word), so raw data can be
/// range-checked without knowing `width`/`height` ahead of a short read.
fn decode_channel(
    reader: &mut Reader,
    channel_len: u32,
    width: u32,
    height: u32,
) -> Option<Vec<u8>> {
    let pixels = (width as usize).checked_mul(height as usize)?;
    if pixels == 0 {
        return Some(Vec::new());
    }
    let compression = reader.u16()?;
    let payload_len = (channel_len as usize).checked_sub(2)?;
    match compression {
        0 => {
            let raw = reader.take(payload_len)?;
            (raw.len() >= pixels).then(|| raw[..pixels].to_vec())
        }
        1 => {
            // One row-length word per scanline — `unpack_bits` decodes the concatenated
            // compressed stream directly, so these are only read here to advance past them.
            let row_lengths_len = (height as usize).checked_mul(2)?;
            reader.skip(row_lengths_len)?;
            unpack_bits(reader, pixels)
        }
        _ => None,
    }
}

struct DecodedLayerRecord {
    name: String,
    visible: bool,
    opacity: f32,
    blend_mode: BlendMode,
    // Layer-space rect on the canvas; `None` bounds (zero width or height) means an empty
    // layer with nothing to place.
    rect: Option<(i32, i32, i32, i32)>,
    channels: Vec<(i16, u32)>,
}

fn read_layer_record(reader: &mut Reader) -> Option<DecodedLayerRecord> {
    let top = reader.i32()?;
    let left = reader.i32()?;
    let bottom = reader.i32()?;
    let right = reader.i32()?;
    let channel_count = reader.u16()? as usize;
    // A hand-crafted file could claim an enormous channel count purely to force a huge
    // allocation below; PSD layers have at most a handful of channels (RGB + alpha + a
    // spot/mask channel or two), so anything past a generous ceiling is not a real file.
    if channel_count > 56 {
        return None;
    }
    let mut channels = Vec::with_capacity(channel_count);
    for _ in 0..channel_count {
        let id = reader.i16()?;
        let len = reader.u32()?;
        channels.push((id, len));
    }
    reader.skip(4)?; // blend signature, "8BIM" — not re-validated, every writer sets it
    let blend_key = reader.take(4)?;
    let blend_mode = blend_mode_from_key(blend_key);
    let opacity = reader.u8()? as f32 / 255.0;
    reader.skip(1)?; // clipping
    let flags = reader.u8()?;
    reader.skip(1)?; // filler
    let extra_len = reader.u32()? as usize;
    let extra = reader.take(extra_len)?;
    let mut extra_reader = Reader::new(extra);

    let mask_len = extra_reader.u32()? as usize;
    extra_reader.skip(mask_len)?;
    let blend_ranges_len = extra_reader.u32()? as usize;
    extra_reader.skip(blend_ranges_len)?;
    let pascal_len = extra_reader.u8()? as usize;
    let pascal_bytes = extra_reader.take(pascal_len)?;
    let mut name = String::from_utf8_lossy(pascal_bytes).into_owned();
    let mut pascal_total = 1 + pascal_len;
    while pascal_total % 4 != 0 {
        extra_reader.skip(1)?;
        pascal_total += 1;
    }

    // Additional layer information: `8BIM`-tagged blocks, unpadded at this level (see
    // `docs/plans`' note on `'luni'` in the exporter — the same asymmetry applies on read).
    // `luni`, when present, is what Photoshop itself displays and always writes alongside the
    // legacy Pascal name, so it wins whenever it is there.
    while extra_reader.remaining() >= 12 {
        let Some(sig) = extra_reader.take(4) else {
            break;
        };
        if sig != BLEND_SIGNATURE {
            break;
        }
        let Some(key) = extra_reader.take(4) else {
            break;
        };
        let Some(len) = extra_reader.u32() else {
            break;
        };
        let Some(data) = extra_reader.take(len as usize) else {
            break;
        };
        if key == b"luni" {
            let mut unicode_reader = Reader::new(data);
            if let Some(unit_count) = unicode_reader.u32() {
                if let Some(text) = unicode_reader.take((unit_count as usize).saturating_mul(2)) {
                    let units: Vec<u16> = text
                        .chunks_exact(2)
                        .map(|c| u16::from_be_bytes([c[0], c[1]]))
                        .collect();
                    if let Ok(unicode_name) = String::from_utf16(&units) {
                        name = unicode_name;
                    }
                }
            }
        }
    }

    let rect = if right > left && bottom > top {
        Some((top, left, bottom, right))
    } else {
        None
    };

    Some(DecodedLayerRecord {
        name,
        visible: flags & 0x02 == 0,
        opacity,
        blend_mode,
        rect,
        channels,
    })
}

/// Layered PSD import — the counterpart to `encode` above, and much less trusting of its
/// input: `encode` only ever has to produce bytes this reads back, but this has to survive
/// whatever a real copy of Photoshop (or a hand-crafted file) hands it. Supports the common
/// case — 8-bit, RGB or RGBA channels, raw or PackBits-compressed — and refuses cleanly
/// (`None`) outside that: CMYK/Lab/indexed/greyscale documents, 16/32-bit depth, or anything
/// truncated or structurally inconsistent. `None` is the caller's cue to fall back to a
/// flattened import instead of failing outright — see `calm_project_create_from_psd`.
pub fn decode(bytes: &[u8]) -> Option<DecodedPsd> {
    let mut r = Reader::new(bytes);
    if r.take(4)? != SIGNATURE {
        return None;
    }
    if r.u16()? != 1 {
        return None; // version 1 (classic .psd) only — not the .psb "large document" format
    }
    r.skip(6)?; // reserved
    r.u16()?; // channel count of the merged image — irrelevant once layers are read
    let height = r.u32()?;
    let width = r.u32()?;
    let depth = r.u16()?;
    let color_mode = r.u16()?;
    if depth != 8 || color_mode != 3 || width == 0 || height == 0 {
        return None; // 8-bit RGB only; see the doc comment above
    }

    let color_mode_data_len = r.u32()? as usize;
    r.skip(color_mode_data_len)?;
    let image_resources_len = r.u32()? as usize;
    r.skip(image_resources_len)?;

    let layer_mask_info_len = r.u32()? as usize;
    let mut lmi = Reader::new(r.take(layer_mask_info_len)?);
    let layer_info_len = lmi.u32()? as usize;
    let mut li = Reader::new(lmi.take(layer_info_len)?);

    let raw_count = li.i16()?;
    // A negative count means the first alpha channel is a transparency mask for the *merged*
    // preview image, not a real layer — the layer count itself is the absolute value.
    let layer_count = raw_count.unsigned_abs() as usize;
    // As with the channel-count guard above: refuse a claimed layer count large enough to be
    // an attempt at a huge allocation rather than a real document.
    if layer_count > 10_000 {
        return None;
    }

    let mut records = Vec::with_capacity(layer_count);
    for _ in 0..layer_count {
        records.push(read_layer_record(&mut li)?);
    }

    let mut layers = Vec::with_capacity(layer_count);
    for record in records {
        let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
        if let Some((top, left, bottom, right)) = record.rect {
            let layer_w = (right - left) as u32;
            let layer_h = (bottom - top) as u32;
            let mut planes: [Option<Vec<u8>>; 4] = [None, None, None, None];
            for &(id, len) in &record.channels {
                let plane = decode_channel(&mut li, len, layer_w, layer_h)?;
                match id {
                    0 => planes[0] = Some(plane),
                    1 => planes[1] = Some(plane),
                    2 => planes[2] = Some(plane),
                    -1 => planes[3] = Some(plane),
                    // A user-supplied layer mask (-2) or a spot channel: not modelled by the
                    // engine's layer type, so its bytes are decoded (to stay aligned with the
                    // rest of the channel stream) and then dropped.
                    _ => {}
                }
            }
            let (Some(rp), Some(gp), Some(bp)) = (&planes[0], &planes[1], &planes[2]) else {
                return None;
            };
            let opaque = vec![255u8; (layer_w as usize) * (layer_h as usize)];
            let ap = planes[3].as_ref().unwrap_or(&opaque);
            for ly in 0..layer_h as i32 {
                let doc_y = top + ly;
                if doc_y < 0 || doc_y as u32 >= height {
                    continue;
                }
                for lx in 0..layer_w as i32 {
                    let doc_x = left + lx;
                    if doc_x < 0 || doc_x as u32 >= width {
                        continue;
                    }
                    let src = (ly as usize) * (layer_w as usize) + lx as usize;
                    let dst = ((doc_y as usize) * (width as usize) + doc_x as usize) * 4;
                    rgba[dst] = rp[src];
                    rgba[dst + 1] = gp[src];
                    rgba[dst + 2] = bp[src];
                    rgba[dst + 3] = ap[src];
                }
            }
        }
        layers.push(DecodedLayer {
            name: record.name,
            visible: record.visible,
            opacity: record.opacity,
            blend_mode: record.blend_mode,
            rgba,
        });
    }

    Some(DecodedPsd {
        width,
        height,
        layers,
    })
}

pub fn decode_flat(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let decoded = decode(bytes)?;
    Some((decoded.width, decoded.height, flatten(&decoded)))
}

fn flatten(decoded: &DecodedPsd) -> Vec<u8> {
    let pixel_bytes = (decoded.width as usize)
        .saturating_mul(decoded.height as usize)
        .saturating_mul(4);
    let mut out = vec![0u8; pixel_bytes];
    for layer in &decoded.layers {
        if !layer.visible {
            continue;
        }
        for (dst, src) in out.chunks_exact_mut(4).zip(layer.rgba.chunks_exact(4)) {
            let alpha = (src[3] as f32 * layer.opacity).round().clamp(0.0, 255.0) as u8;
            if alpha == 0 {
                continue;
            }
            let blended = calumma_core::blend_with_mode(
                [dst[0], dst[1], dst[2], dst[3]],
                [src[0], src[1], src[2], alpha],
                layer.blend_mode,
            );
            dst.copy_from_slice(&blended);
        }
    }
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

        doc.add_vector_layer(
            "Shapes",
            VectorItem::Shape(VectorShape {
                shape: Shape {
                    tool: Tool::Rect,
                    start: (4.0, 4.0),
                    end: (20.0, 20.0),
                    half_width: 1.0,
                    fill: true,
                    stroke: false,
                },
                color: [255, 0, 0, 255],
                stroke_color: [255, 0, 0, 255],
            }),
        );
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

    // --- decode ---

    #[test]
    fn decoding_this_modules_own_output_round_trips_every_layer() {
        use calumma_core::tile::DocRect;

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

        let bytes = encode(&doc);
        let decoded = decode(&bytes).expect("our own output must decode");
        assert_eq!(decoded.width, 12);
        assert_eq!(decoded.height, 8);
        // Document::new seeds Paper *and* a default "Layer 1" on its own, and encode() doesn't
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
        let bytes = encode(&doc);
        let decoded = decode(&bytes).expect("decodes");
        // Index 1 is the default "Layer 1" Document::new seeds on its own; ours is index 2.
        assert_eq!(decoded.layers[2].name, "日本語レイヤー");
    }

    #[test]
    fn decode_refuses_the_wrong_signature() {
        assert!(decode(b"NOPE0000000000000000000000").is_none());
    }

    #[test]
    fn decode_refuses_truncated_input_at_every_length_rather_than_panicking() {
        let doc = Document::new("p".into(), "t", 8, 8);
        let bytes = encode(&doc);
        // A malformed or partially-downloaded file has to come back `None`, never a panic —
        // this is parsing bytes nobody promised were well-formed. Every prefix length is worth
        // checking rather than a handful of guesses, since an off-by-one bounds check anywhere
        // in the parser would only show up at one specific length.
        for len in 0..bytes.len() {
            let _ = decode(&bytes[..len]);
        }
    }

    #[test]
    fn decode_refuses_16_bit_depth_and_non_rgb_color_modes() {
        let doc = Document::new("p".into(), "t", 4, 4);
        let mut bytes = encode(&doc);
        bytes[22] = 0;
        bytes[23] = 16; // depth -> 16
        assert!(decode(&bytes).is_none());

        let mut bytes = encode(&doc);
        bytes[24] = 0;
        bytes[25] = 4; // color mode -> CMYK
        assert!(decode(&bytes).is_none());
    }

    #[test]
    fn decode_refuses_the_psb_large_document_version() {
        let doc = Document::new("p".into(), "t", 4, 4);
        let mut bytes = encode(&doc);
        bytes[4] = 0;
        bytes[5] = 2; // version -> 2 (.psb)
        assert!(decode(&bytes).is_none());
    }

    #[test]
    fn unmapped_blend_modes_fall_back_to_normal_rather_than_failing_the_import() {
        assert_eq!(blend_mode_from_key(b"lddg"), BlendMode::Normal);
        assert_eq!(blend_mode_from_key(b"norm"), BlendMode::Normal);
        assert_eq!(blend_mode_from_key(b"mul "), BlendMode::Multiply);
        assert_eq!(blend_mode_from_key(b"scrn"), BlendMode::Screen);
    }

    #[test]
    fn unpack_bits_decodes_literal_and_repeat_runs_and_skips_the_no_op() {
        // Literal run: copy 3 bytes as-is.
        let literal = [2u8, 10, 20, 30];
        let mut r = Reader::new(&literal);
        assert_eq!(unpack_bits(&mut r, 3), Some(vec![10, 20, 30]));

        // Repeat run: header -3 means repeat the next byte (1 - (-3)) = 4 times.
        let repeat = [(-3i8) as u8, 7];
        let mut r = Reader::new(&repeat);
        assert_eq!(unpack_bits(&mut r, 4), Some(vec![7, 7, 7, 7]));

        // The -128 no-op contributes nothing and is simply skipped.
        let with_noop = [(-128i8) as u8, 0u8, 99];
        let mut r = Reader::new(&with_noop);
        assert_eq!(unpack_bits(&mut r, 1), Some(vec![99]));

        // A mix, back to back, stopping exactly at the requested length.
        let mixed = [1u8, 5, 6, (-2i8) as u8, 9];
        let mut r = Reader::new(&mixed);
        assert_eq!(unpack_bits(&mut r, 5), Some(vec![5, 6, 9, 9, 9]));
    }

    /// A hand-built file (our own `encode` always writes full-canvas layers, so it can never
    /// exercise this) with a layer smaller than the canvas and offset within it — the shape
    /// every layer in a real Photoshop file actually has.
    #[test]
    fn decode_places_a_layer_smaller_than_the_canvas_at_its_own_offset() {
        const CANVAS: u32 = 20;
        const TOP: i32 = 4;
        const LEFT: i32 = 6;
        const H: u32 = 5;
        const W: u32 = 3;
        let pixel_count = (W * H) as usize;

        let mut layer_info = Vec::new();
        layer_info.extend_from_slice(&u16be(1)); // one layer
        let mut record = Vec::new();
        record.extend_from_slice(&i32be(TOP));
        record.extend_from_slice(&i32be(LEFT));
        record.extend_from_slice(&i32be(TOP + H as i32));
        record.extend_from_slice(&i32be(LEFT + W as i32));
        record.extend_from_slice(&u16be(3)); // R, G, B — no alpha, opaque layer
        let channel_len = 2 + pixel_count as u32;
        for id in [0i16, 1, 2] {
            record.extend_from_slice(&(id as u16).to_be_bytes());
            record.extend_from_slice(&u32be(channel_len));
        }
        record.extend_from_slice(BLEND_SIGNATURE);
        record.extend_from_slice(b"norm");
        record.push(255); // opacity
        record.push(0); // clipping
        record.push(0); // flags: visible
        record.push(0); // filler
        let mut extra = Vec::new();
        extra.extend_from_slice(&u32be(0)); // mask data
        extra.extend_from_slice(&u32be(0)); // blending ranges
        extra.extend_from_slice(&pascal_name("Patch"));
        record.extend_from_slice(&u32be(extra.len() as u32));
        record.extend_from_slice(&extra);
        layer_info.extend_from_slice(&record);
        // One solid colour, [200, 0, 0] — R constant 200, G and B constant 0 — each channel
        // its own raw-compressed (method 0) plane of `pixel_count` bytes.
        for value in [200u8, 0, 0] {
            layer_info.extend_from_slice(&u16be(0)); // compression: raw
            layer_info.extend(vec![value; pixel_count]);
        }
        if layer_info.len() % 2 != 0 {
            layer_info.push(0);
        }

        let mut layer_mask_info = Vec::new();
        layer_mask_info.extend_from_slice(&u32be(layer_info.len() as u32));
        layer_mask_info.extend_from_slice(&layer_info);
        layer_mask_info.extend_from_slice(&u32be(0));

        let mut bytes = Vec::new();
        bytes.extend_from_slice(SIGNATURE);
        bytes.extend_from_slice(&u16be(1));
        bytes.extend_from_slice(&[0u8; 6]);
        bytes.extend_from_slice(&u16be(3));
        bytes.extend_from_slice(&u32be(CANVAS));
        bytes.extend_from_slice(&u32be(CANVAS));
        bytes.extend_from_slice(&u16be(8));
        bytes.extend_from_slice(&u16be(3));
        bytes.extend_from_slice(&u32be(0));
        bytes.extend_from_slice(&u32be(0));
        bytes.extend_from_slice(&u32be(layer_mask_info.len() as u32));
        bytes.extend_from_slice(&layer_mask_info);
        // Merged image data — irrelevant to this test, but the format still expects it.
        bytes.extend_from_slice(&u16be(0));
        bytes.extend(vec![0u8; (CANVAS * CANVAS * 4) as usize]);

        let decoded = decode(&bytes).expect("a hand-built minimal PSD must still decode");
        assert_eq!(decoded.layers.len(), 1);
        let layer = &decoded.layers[0];
        assert_eq!(layer.name, "Patch");

        // Inside the layer's rect, the pixel is opaque red-ish [200,0,0] with alpha defaulted
        // to fully opaque (no alpha channel was declared).
        let idx = ((TOP as u32 + 1) * CANVAS + (LEFT as u32 + 1)) as usize * 4;
        assert_eq!(&layer.rgba[idx..idx + 4], &[200, 0, 0, 255]);

        // Outside the layer's rect entirely, nothing was painted.
        assert_eq!(&layer.rgba[0..4], &[0, 0, 0, 0]);
    }
}
