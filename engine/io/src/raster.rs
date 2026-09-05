use crate::png::encode_png_rgba;
use crate::raster_heic;
use crate::raster_psd;
use crate::raster_svg;
use calumma_core::limits::IMPORT_MAX_SIDE;
use calumma_core::LOSSY_EXPORT_QUALITY;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::{self, FilterType};
use image::{ColorType, ImageEncoder, RgbaImage};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RasterFormat {
    Png = 0,
    Jpeg = 1,
    Webp = 2,
    Avif = 3,
    Heic = 4,
}

impl RasterFormat {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Png),
            1 => Some(Self::Jpeg),
            2 => Some(Self::Webp),
            3 => Some(Self::Avif),
            4 => Some(Self::Heic),
            _ => None,
        }
    }
}

pub fn decode_encoded(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let (width, height, rgba) = decode_native(bytes)?;
    Some(fit_import(width, height, rgba))
}

pub fn encode_rgba(rgba: &[u8], width: u32, height: u32, format: RasterFormat) -> Option<Vec<u8>> {
    encode_rgba_quality(rgba, width, height, format, LOSSY_EXPORT_QUALITY)
}

pub fn encode_rgba_quality(
    rgba: &[u8],
    width: u32,
    height: u32,
    format: RasterFormat,
    quality: f32,
) -> Option<Vec<u8>> {
    let expected = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    if expected != rgba.len() || width == 0 || height == 0 {
        return None;
    }
    let quality = quality_u8(quality);
    match format {
        RasterFormat::Png => encode_png_rgba(rgba, width, height).ok(),
        RasterFormat::Jpeg => encode_jpeg(rgba, width, height, quality),
        RasterFormat::Webp => encode_webp(rgba, width, height),
        RasterFormat::Avif => encode_avif(rgba, width, height, quality),
        RasterFormat::Heic => raster_heic::encode(rgba, width, height, quality),
    }
}

fn decode_native(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    if looks_like_svg(bytes) {
        return raster_svg::decode(bytes);
    }
    if bytes.starts_with(b"8BPS") {
        return crate::psd::decode_flat(bytes).or_else(|| raster_psd::decode(bytes));
    }
    if is_avif(bytes) {
        return crate::raster_avif::decode(bytes);
    }
    if is_heic(bytes) {
        return raster_heic::decode(bytes);
    }
    load_image(bytes)
}

fn load_image(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let rgba = image::load_from_memory(bytes).ok()?.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    Some((width, height, rgba.into_raw()))
}

fn fit_import(width: u32, height: u32, rgba: Vec<u8>) -> (u32, u32, Vec<u8>) {
    let long = width.max(height);
    if long <= IMPORT_MAX_SIDE {
        return (width, height, rgba);
    }
    let scale = IMPORT_MAX_SIDE as f32 / long as f32;
    let next_w = ((width as f32 * scale).round() as u32).max(1);
    let next_h = ((height as f32 * scale).round() as u32).max(1);
    let Some(src) = RgbaImage::from_raw(width, height, rgba) else {
        return (width, height, Vec::new());
    };
    let scaled = imageops::resize(&src, next_w, next_h, FilterType::Triangle);
    (next_w, next_h, scaled.into_raw())
}

fn encode_jpeg(rgba: &[u8], width: u32, height: u32, quality: u8) -> Option<Vec<u8>> {
    let rgb = flatten_on_white(rgba);
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality)
        .write_image(&rgb, width, height, ColorType::Rgb8.into())
        .ok()?;
    Some(out)
}

fn encode_webp(rgba: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    image::codecs::webp::WebPEncoder::new_lossless(&mut out)
        .write_image(rgba, width, height, ColorType::Rgba8.into())
        .ok()?;
    Some(out)
}

fn encode_avif(rgba: &[u8], width: u32, height: u32, quality: u8) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    image::codecs::avif::AvifEncoder::new_with_speed_quality(&mut out, 6, quality)
        .write_image(rgba, width, height, ColorType::Rgba8.into())
        .ok()?;
    Some(out)
}

fn flatten_on_white(rgba: &[u8]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
    for px in rgba.chunks_exact(4) {
        let alpha = px[3] as u16;
        if alpha == 255 {
            rgb.extend_from_slice(&px[..3]);
            continue;
        }
        if alpha == 0 {
            rgb.extend_from_slice(&[255, 255, 255]);
            continue;
        }
        let inv = 255 - alpha;
        rgb.push(((px[0] as u16 * alpha + 255 * inv + 127) / 255) as u8);
        rgb.push(((px[1] as u16 * alpha + 255 * inv + 127) / 255) as u8);
        rgb.push(((px[2] as u16 * alpha + 255 * inv + 127) / 255) as u8);
    }
    rgb
}

fn quality_u8(quality: f32) -> u8 {
    (quality * 100.0).round().clamp(1.0, 100.0) as u8
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    let text = text.trim_start();
    let text = text.strip_prefix('\u{feff}').unwrap_or(text).trim_start();
    text.starts_with("<svg")
        || text.starts_with("<?xml")
        || text.starts_with("<!DOCTYPE svg")
        || text.starts_with("<!doctype svg")
}

fn is_avif(bytes: &[u8]) -> bool {
    is_heif_brand(bytes, b"avif") || is_heif_brand(bytes, b"avis")
}

fn is_heic(bytes: &[u8]) -> bool {
    is_heif_brand(bytes, b"heic")
        || is_heif_brand(bytes, b"heix")
        || is_heif_brand(bytes, b"hevc")
        || is_heif_brand(bytes, b"hevx")
        || is_heif_brand(bytes, b"mif1")
}

fn is_heif_brand(bytes: &[u8], brand: &[u8; 4]) -> bool {
    if bytes.len() < 12 || &bytes[4..8] != b"ftyp" {
        return false;
    }
    let Ok(size_bytes) = bytes[0..4].try_into() else {
        return false;
    };
    let size = u32::from_be_bytes(size_bytes) as usize;
    let end = size.min(bytes.len()).max(12);
    bytes[8..end].chunks_exact(4).any(|chunk| chunk == brand)
}
