use super::reader::{unpack_bits, Reader};
use super::{blend_mode_from_key, BLEND_SIGNATURE, SIGNATURE};
use calumma_core::BlendMode;

/// One layer decoded from a PSD file, already placed on the full canvas — `rgba` is
/// `width * height * 4` bytes, transparent everywhere outside the layer's own on-disk bounds.
/// Doing the placement here rather than handing back the layer's own (possibly smaller,
/// possibly offset) rect keeps every caller's life the same shape as `layer_rgba`/`place_image`
/// already assume: one canvas-sized buffer per layer, no separate offset to thread through.
pub struct DecodedLayer {
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
/// flattened import instead of failing outright.
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
