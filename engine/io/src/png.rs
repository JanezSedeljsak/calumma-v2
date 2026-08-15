use png::{BitDepth, ColorType, Decoder, Encoder};

pub fn encode_png_rgba(
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Result<Vec<u8>, png::EncodingError> {
    let mut png = Vec::new();
    {
        let mut encoder = Encoder::new(&mut png, width, height);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(rgba)?;
    }
    Ok(png)
}

pub fn decode_png_rgba(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let decoder = Decoder::new(bytes);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    reader.next_frame(&mut buf).ok()?;
    let info = reader.info();
    Some((info.width, info.height, buf))
}
