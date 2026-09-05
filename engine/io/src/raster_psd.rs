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
