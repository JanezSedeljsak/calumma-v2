use heif::{EncoderConfig, HeifEncoder, Preset};
use image::{DynamicImage, RgbaImage};

pub fn decode(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let img = heif::decode(bytes).ok()?.to_rgba8();
    let width = img.width();
    let height = img.height();
    Some((width, height, img.into_raw()))
}

pub fn encode(rgba: &[u8], width: u32, height: u32, quality: u8) -> Option<Vec<u8>> {
    let img = RgbaImage::from_raw(width, height, rgba.to_vec())?;
    let mut out = Vec::new();
    let encoder = HeifEncoder::new_with_config(
        &mut out,
        EncoderConfig {
            quality,
            preset: Preset::Fast,
            ..EncoderConfig::default()
        },
    );
    DynamicImage::ImageRgba8(img)
        .write_with_encoder(encoder)
        .ok()?;
    Some(out)
}
