use aom_decode::avif::{Avif, Image};
use aom_decode::Config;

pub fn decode(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut avif = Avif::decode(
        bytes,
        &Config {
            threads: std::thread::available_parallelism()
                .map(|n| n.get().min(32))
                .unwrap_or(4),
        },
    )
    .ok()?;
    Some(to_rgba8(avif.convert().ok()?))
}

fn to_rgba8(image: Image) -> (u32, u32, Vec<u8>) {
    match image {
        Image::RGBA8(buf) => pack(
            buf.width(),
            buf.height(),
            buf.pixels().map(|px| [px.r, px.g, px.b, px.a]),
        ),
        Image::RGB8(buf) => pack(
            buf.width(),
            buf.height(),
            buf.pixels().map(|px| [px.r, px.g, px.b, 255]),
        ),
        Image::Gray8(buf) => pack(
            buf.width(),
            buf.height(),
            buf.pixels().map(|v| [v, v, v, 255]),
        ),
        Image::RGBA16(buf) => pack(
            buf.width(),
            buf.height(),
            buf.pixels()
                .map(|px| [drop8(px.r), drop8(px.g), drop8(px.b), drop8(px.a)]),
        ),
        Image::RGB16(buf) => pack(
            buf.width(),
            buf.height(),
            buf.pixels()
                .map(|px| [drop8(px.r), drop8(px.g), drop8(px.b), 255]),
        ),
        Image::Gray16(buf) => pack(
            buf.width(),
            buf.height(),
            buf.pixels().map(|v| {
                let v = drop8(v);
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
