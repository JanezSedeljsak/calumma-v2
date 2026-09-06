/// Flattened-composite PSD decoding: grayscale (mode 1) and RGB (mode 3), 8-bit only, version 1
/// only. `raster.rs::decode_native` only reaches this *after* `psd::decode_flat` (the layered
/// decoder) has already refused a file — today that means grayscale PSDs specifically, since
/// the layered decoder only understands RGB. Untested until now: nothing in the wider test
/// suite ever produces a PSD the layered decoder can't open, so this whole file read as 0%
/// covered despite being real, shipped fallback code.
pub fn decode(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    if bytes.len() < 26 || &bytes[0..4] != b"8BPS" {
        return None;
    }
    let version = u16::from_be_bytes(bytes[4..6].try_into().ok()?);
    if version != 1 {
        return None;
    }
    let channels = u16::from_be_bytes(bytes[12..14].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[14..18].try_into().ok()?);
    let width = u32::from_be_bytes(bytes[18..22].try_into().ok()?);
    let depth = u16::from_be_bytes(bytes[22..24].try_into().ok()?);
    let mode = u16::from_be_bytes(bytes[24..26].try_into().ok()?);
    if depth != 8 || width == 0 || height == 0 || channels == 0 || channels > 4 {
        return None;
    }
    if mode != 1 && mode != 3 {
        return None;
    }
    let mut offset = 26usize;
    offset = skip_block(bytes, offset)?;
    offset = skip_block(bytes, offset)?;
    offset = skip_block(bytes, offset)?;
    if offset + 2 > bytes.len() {
        return None;
    }
    let compression = u16::from_be_bytes(bytes[offset..offset + 2].try_into().ok()?);
    offset += 2;
    let pixel_count = (width as usize).checked_mul(height as usize)?;
    let planes = match compression {
        0 => read_raw_planes(&bytes[offset..], channels as usize, pixel_count)?,
        1 => read_rle_planes(&bytes[offset..], channels as usize, width, height)?,
        _ => return None,
    };
    Some((
        width,
        height,
        interleave(planes, channels as usize, pixel_count, mode),
    ))
}

fn skip_block(bytes: &[u8], offset: usize) -> Option<usize> {
    if offset + 4 > bytes.len() {
        return None;
    }
    let len = u32::from_be_bytes(bytes[offset..offset + 4].try_into().ok()?) as usize;
    offset.checked_add(4)?.checked_add(len)
}

fn read_raw_planes(bytes: &[u8], channels: usize, pixel_count: usize) -> Option<Vec<Vec<u8>>> {
    let needed = channels.checked_mul(pixel_count)?;
    if bytes.len() < needed {
        return None;
    }
    Some(
        (0..channels)
            .map(|channel| {
                let start = channel * pixel_count;
                bytes[start..start + pixel_count].to_vec()
            })
            .collect(),
    )
}

fn read_rle_planes(bytes: &[u8], channels: usize, width: u32, height: u32) -> Option<Vec<Vec<u8>>> {
    let rows = height as usize;
    let row_bytes = width as usize;
    let counts_len = channels.checked_mul(rows)?.checked_mul(2)?;
    if bytes.len() < counts_len {
        return None;
    }
    let mut offset = counts_len;
    let mut planes = Vec::with_capacity(channels);
    for channel in 0..channels {
        let mut plane = Vec::with_capacity(rows * row_bytes);
        for row in 0..rows {
            let count_at = (channel * rows + row) * 2;
            let packed_len =
                u16::from_be_bytes(bytes[count_at..count_at + 2].try_into().ok()?) as usize;
            let end = offset.checked_add(packed_len)?;
            if end > bytes.len() {
                return None;
            }
            unpack_packbits(&bytes[offset..end], row_bytes, &mut plane)?;
            offset = end;
        }
        planes.push(plane);
    }
    Some(planes)
}

fn unpack_packbits(src: &[u8], expected: usize, out: &mut Vec<u8>) -> Option<()> {
    let start = out.len();
    let mut i = 0usize;
    while i < src.len() {
        let header = src[i] as i8;
        i += 1;
        if header >= 0 {
            let count = header as usize + 1;
            if i + count > src.len() {
                return None;
            }
            out.extend_from_slice(&src[i..i + count]);
            i += count;
        } else if header != -128 {
            let count = (-header as usize) + 1;
            if i >= src.len() {
                return None;
            }
            out.extend(std::iter::repeat(src[i]).take(count));
            i += 1;
        }
    }
    if out.len() - start == expected {
        Some(())
    } else {
        None
    }
}

fn interleave(planes: Vec<Vec<u8>>, channels: usize, pixel_count: usize, mode: u16) -> Vec<u8> {
    let mut rgba = vec![0u8; pixel_count * 4];
    for i in 0..pixel_count {
        let o = i * 4;
        match (mode, channels) {
            (1, 1) => {
                let v = planes[0][i];
                rgba[o] = v;
                rgba[o + 1] = v;
                rgba[o + 2] = v;
                rgba[o + 3] = 255;
            }
            (1, _) => {
                let v = planes[0][i];
                rgba[o] = v;
                rgba[o + 1] = v;
                rgba[o + 2] = v;
                rgba[o + 3] = *planes.get(1).and_then(|p| p.get(i)).unwrap_or(&255);
            }
            (_, 3) => {
                rgba[o] = planes[0][i];
                rgba[o + 1] = planes[1][i];
                rgba[o + 2] = planes[2][i];
                rgba[o + 3] = 255;
            }
            _ => {
                rgba[o] = planes[0][i];
                rgba[o + 1] = planes[1][i];
                rgba[o + 2] = planes[2][i];
                rgba[o + 3] = *planes.get(3).and_then(|p| p.get(i)).unwrap_or(&255);
            }
        }
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(
        channels: u16,
        width: u32,
        height: u32,
        depth: u16,
        mode: u16,
        version: u16,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"8BPS");
        out.extend_from_slice(&version.to_be_bytes());
        out.extend_from_slice(&[0u8; 6]);
        out.extend_from_slice(&channels.to_be_bytes());
        out.extend_from_slice(&height.to_be_bytes());
        out.extend_from_slice(&width.to_be_bytes());
        out.extend_from_slice(&depth.to_be_bytes());
        out.extend_from_slice(&mode.to_be_bytes());
        out
    }

    fn empty_block(out: &mut Vec<u8>) {
        out.extend_from_slice(&0u32.to_be_bytes());
    }

    /// One PackBits run per row: a literal-copy header (`row_bytes - 1`, since `n >= 0` means
    /// "copy `n + 1` bytes") followed by the row itself. Every test image here is small enough
    /// that a row fits in one run (`n` is an `i8`, so at most 128 literal bytes).
    fn rle_row(row: &[u8], out: &mut Vec<u8>) {
        assert!(row.len() <= 128, "row too long for a single PackBits run");
        out.push((row.len() - 1) as u8);
        out.extend_from_slice(row);
    }

    fn build_raw(channels: u16, width: u32, height: u32, mode: u16, planes: &[Vec<u8>]) -> Vec<u8> {
        let mut out = header(channels, width, height, 8, mode, 1);
        empty_block(&mut out); // color mode data
        empty_block(&mut out); // image resources
        empty_block(&mut out); // layer & mask info
        out.extend_from_slice(&0u16.to_be_bytes()); // compression: raw
        for plane in planes {
            out.extend_from_slice(plane);
        }
        out
    }

    fn build_rle(channels: u16, width: u32, height: u32, mode: u16, planes: &[Vec<u8>]) -> Vec<u8> {
        let row_bytes = width as usize;
        let rows = height as usize;
        let mut out = header(channels, width, height, 8, mode, 1);
        empty_block(&mut out);
        empty_block(&mut out);
        empty_block(&mut out);
        out.extend_from_slice(&1u16.to_be_bytes()); // compression: RLE

        // Each row's packed length is one byte (the control byte) plus the row itself.
        let packed_len = (row_bytes + 1) as u16;
        for _channel in 0..channels {
            for _row in 0..rows {
                out.extend_from_slice(&packed_len.to_be_bytes());
            }
        }
        for plane in planes {
            for row in plane.chunks_exact(row_bytes) {
                rle_row(row, &mut out);
            }
        }
        out
    }

    #[test]
    fn decodes_a_raw_rgb_image() {
        let (w, h) = (2u32, 2u32);
        let r = vec![255, 0, 128, 64];
        let g = vec![0, 255, 128, 64];
        let b = vec![0, 0, 128, 64];
        let bytes = build_raw(3, w, h, 3, &[r, g, b]);
        let (dw, dh, rgba) = decode(&bytes).expect("a minimal raw RGB PSD decodes");
        assert_eq!((dw, dh), (w, h));
        assert_eq!(
            &rgba[0..4],
            &[255, 0, 0, 255],
            "no alpha channel means opaque"
        );
        assert_eq!(&rgba[4..8], &[0, 255, 0, 255]);
        assert_eq!(&rgba[8..12], &[128, 128, 128, 255]);
        assert_eq!(&rgba[12..16], &[64, 64, 64, 255]);
    }

    #[test]
    fn decodes_a_raw_rgba_image_with_its_own_alpha() {
        let (w, h) = (2u32, 1u32);
        let planes = vec![vec![10, 20], vec![30, 40], vec![50, 60], vec![0, 255]];
        let bytes = build_raw(4, w, h, 3, &planes);
        let (_, _, rgba) = decode(&bytes).expect("RGBA decodes");
        assert_eq!(
            &rgba[0..4],
            &[10, 30, 50, 0],
            "alpha channel is respected, not assumed"
        );
        assert_eq!(&rgba[4..8], &[20, 40, 60, 255]);
    }

    #[test]
    fn decodes_an_rle_rgb_image() {
        let (w, h) = (3u32, 2u32);
        let r = vec![1, 2, 3, 4, 5, 6];
        let g = vec![10, 20, 30, 40, 50, 60];
        let b = vec![100, 110, 120, 130, 140, 150];
        let raw = decode(&build_raw(3, w, h, 3, &[r.clone(), g.clone(), b.clone()])).unwrap();
        let rle = decode(&build_rle(3, w, h, 3, &[r, g, b])).expect("RLE RGB decodes");
        assert_eq!(raw, rle, "raw and RLE encodings of the same pixels agree");
    }

    #[test]
    fn decodes_a_raw_grayscale_image() {
        let (w, h) = (2u32, 2u32);
        let gray = vec![0, 64, 128, 255];
        let bytes = build_raw(1, w, h, 1, &[gray]);
        let (dw, dh, rgba) = decode(&bytes).expect("grayscale decodes");
        assert_eq!((dw, dh), (w, h));
        assert_eq!(
            &rgba[0..4],
            &[0, 0, 0, 255],
            "gray value copied to R, G and B"
        );
        assert_eq!(&rgba[4..8], &[64, 64, 64, 255]);
        assert_eq!(&rgba[8..12], &[128, 128, 128, 255]);
        assert_eq!(&rgba[12..16], &[255, 255, 255, 255]);
    }

    #[test]
    fn decodes_a_grayscale_image_with_alpha() {
        let (w, h) = (2u32, 1u32);
        let gray = vec![200, 50];
        let alpha = vec![255, 0];
        let bytes = build_raw(2, w, h, 1, &[gray, alpha]);
        let (_, _, rgba) = decode(&bytes).expect("grayscale+alpha decodes");
        assert_eq!(&rgba[0..4], &[200, 200, 200, 255]);
        assert_eq!(&rgba[4..8], &[50, 50, 50, 0]);
    }

    /// `rle_row` above only ever emits a literal-copy control byte. `unpack_packbits` has a
    /// second shape — a negative header repeating the next byte — that a real "flat colour
    /// row" PSD would actually produce, and it needs its own row-builder to reach at all.
    #[test]
    fn decodes_an_rle_row_encoded_as_a_repeat_run() {
        let (w, h) = (5u32, 1u32);
        let mut out = header(1, w, h, 8, 1, 1);
        empty_block(&mut out);
        empty_block(&mut out);
        empty_block(&mut out);
        out.extend_from_slice(&1u16.to_be_bytes());
        out.extend_from_slice(&2u16.to_be_bytes()); // one control byte + one value byte
        out.push((1i8 - 5) as u8); // header -4: repeat the next byte 1-(-4) = 5 times
        out.push(200);
        let (_, _, rgba) = decode(&out).expect("a repeat-run row decodes");
        for px in rgba.chunks_exact(4) {
            assert_eq!(px, [200, 200, 200, 255]);
        }
    }

    #[test]
    fn rle_refuses_a_truncated_count_table() {
        let (w, h) = (2u32, 2u32);
        let mut out = header(1, w, h, 8, 1, 1);
        empty_block(&mut out);
        empty_block(&mut out);
        empty_block(&mut out);
        out.extend_from_slice(&1u16.to_be_bytes());
        // Needs channels*rows*2 = 4 bytes of row-length table; only 2 are present.
        out.extend_from_slice(&3u16.to_be_bytes());
        assert!(decode(&out).is_none());
    }

    #[test]
    fn rle_refuses_a_row_whose_packed_data_runs_past_the_end() {
        let (w, h) = (2u32, 1u32);
        let mut out = header(1, w, h, 8, 1, 1);
        empty_block(&mut out);
        empty_block(&mut out);
        empty_block(&mut out);
        out.extend_from_slice(&1u16.to_be_bytes());
        // Claims a 10-byte packed row, then provides none of it.
        out.extend_from_slice(&10u16.to_be_bytes());
        assert!(decode(&out).is_none());
    }

    #[test]
    fn unpack_packbits_refuses_a_run_that_decodes_to_the_wrong_length() {
        // A complete, valid run — header 0 ("copy 1 literal byte") plus that one byte — that
        // fully consumes its input but produces fewer bytes than `expected`. Every earlier
        // guard passes; only the trailing length check catches this.
        let mut out = Vec::new();
        assert!(unpack_packbits(&[0u8, 42u8], 2, &mut out).is_none());
    }

    #[test]
    fn decodes_an_rle_grayscale_image() {
        let (w, h) = (4u32, 1u32);
        let gray = vec![9, 18, 27, 36];
        let raw = decode(&build_raw(1, w, h, 1, std::slice::from_ref(&gray))).unwrap();
        let rle = decode(&build_rle(1, w, h, 1, &[gray])).expect("RLE grayscale decodes");
        assert_eq!(raw, rle);
    }

    #[test]
    fn refuses_the_wrong_signature() {
        let mut bytes = build_raw(3, 2, 2, 3, &[vec![0; 4], vec![0; 4], vec![0; 4]]);
        bytes[0] = b'X';
        assert!(decode(&bytes).is_none());
    }

    #[test]
    fn refuses_the_psb_large_document_version() {
        let mut bytes = build_raw(3, 2, 2, 3, &[vec![0; 4], vec![0; 4], vec![0; 4]]);
        bytes[4..6].copy_from_slice(&2u16.to_be_bytes());
        assert!(decode(&bytes).is_none());
    }

    #[test]
    fn refuses_16_bit_depth() {
        let mut out = header(3, 2, 2, 16, 3, 1);
        empty_block(&mut out);
        empty_block(&mut out);
        empty_block(&mut out);
        out.extend_from_slice(&0u16.to_be_bytes());
        for _ in 0..3 {
            out.extend_from_slice(&[0u8; 8]); // 4 pixels x 2 bytes each
        }
        assert!(decode(&out).is_none());
    }

    #[test]
    fn refuses_unsupported_color_modes() {
        // CMYK (4) and Lab (9) are neither grayscale nor RGB, the only two this decoder models.
        for mode in [4u16, 9] {
            let planes: Vec<Vec<u8>> = (0..4).map(|_| vec![0u8; 4]).collect();
            let bytes = build_raw(4, 2, 2, mode, &planes);
            assert!(decode(&bytes).is_none(), "mode {mode}");
        }
    }

    #[test]
    fn refuses_zero_and_excess_channel_counts() {
        let zero = build_raw(0, 2, 2, 3, &[]);
        assert!(decode(&zero).is_none());
        let planes: Vec<Vec<u8>> = (0..5).map(|_| vec![0u8; 4]).collect();
        let five = build_raw(5, 2, 2, 3, &planes);
        assert!(decode(&five).is_none(), "more channels than RGBA has slots");
    }

    #[test]
    fn refuses_zero_width_or_height() {
        let zero_w = build_raw(3, 0, 2, 3, &[vec![], vec![], vec![]]);
        assert!(decode(&zero_w).is_none());
        let zero_h = build_raw(3, 2, 0, 3, &[vec![], vec![], vec![]]);
        assert!(decode(&zero_h).is_none());
    }

    /// A malformed or truncated file has to be refused at whichever length it stops making
    /// sense, never panic — the same defensive contract `psd.rs`'s layered decoder holds itself
    /// to, pinned here the same way: every prefix length of a valid file.
    #[test]
    fn refuses_truncated_input_at_every_length_rather_than_panicking() {
        let full = build_raw(3, 2, 2, 3, &[vec![1; 4], vec![2; 4], vec![3; 4]]);
        for len in 0..full.len() {
            let _ = decode(&full[..len]);
        }
        assert!(
            decode(&full).is_some(),
            "the untruncated file still decodes"
        );
    }

    #[test]
    fn an_unrecognised_compression_method_is_refused() {
        let mut out = header(1, 2, 1, 8, 1, 1);
        empty_block(&mut out);
        empty_block(&mut out);
        empty_block(&mut out);
        out.extend_from_slice(&99u16.to_be_bytes());
        out.extend_from_slice(&[0u8; 2]);
        assert!(decode(&out).is_none());
    }
}
