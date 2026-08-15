use image::codecs::png::PngEncoder;
use image::{ColorType, ImageEncoder};

pub fn encode_png_rgba(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, std::io::Error> {
    let mut png = Vec::new();
    let encoder = PngEncoder::new(&mut png);
    encoder
        .write_image(rgba, width, height, ColorType::Rgba8.into())
        .map_err(std::io::Error::other)?;
    Ok(png)
}
