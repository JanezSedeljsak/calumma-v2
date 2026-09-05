use avif_decode::{Decoder, Image};

pub fn decode(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let image = Decoder::from_avif(bytes).ok()?.to_image().ok()?;
    Some(to_rgba8(image))
}

fn to_rgba8(image: Image) -> (u32, u32, Vec<u8>) {
    match image {
        Image::Rgba8(buf) => pack(
            buf.width(),
            buf.height(),
            buf.pixels().map(|px| [px.r, px.g, px.b, px.a]),
        ),
        Image::Rgb8(buf) => pack(
            buf.width(),
            buf.height(),
            buf.pixels().map(|px| [px.r, px.g, px.b, 255]),
        ),
        Image::Gray8(buf) => pack(
            buf.width(),
            buf.height(),
            buf.pixels().map(|px| {
                let v = px.value();
                [v, v, v, 255]
            }),
        ),
        Image::Rgba16(buf) => pack(
            buf.width(),
            buf.height(),
            buf.pixels()
                .map(|px| [drop8(px.r), drop8(px.g), drop8(px.b), drop8(px.a)]),
        ),
        Image::Rgb16(buf) => pack(
            buf.width(),
            buf.height(),
            buf.pixels()
                .map(|px| [drop8(px.r), drop8(px.g), drop8(px.b), 255]),
        ),
        Image::Gray16(buf) => pack(
            buf.width(),
            buf.height(),
            buf.pixels().map(|px| {
                let v = drop8(px.value());
                [v, v, v, 255]
            }),
        ),
    }
}

fn pack(width: usize, height: usize, pixels: impl Iterator<Item = [u8; 4]>) -> (u32, u32, Vec<u8>) {
    let mut rgba = Vec::with_capacity(width.saturating_mul(height).saturating_mul(4));
    for px in pixels {
        rgba.extend_from_slice(&px);
    }
    (width as u32, height as u32, rgba)
}

fn drop8(v: u16) -> u8 {
    (v >> 8) as u8
}
