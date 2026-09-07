use image::{imageops, Rgba, RgbaImage};
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::io::Cursor;
use std::sync::OnceLock;

const APP_ICON_PNG: &[u8] = include_bytes!("../../design/icon.png");
const DOCK_SIDE: u32 = 256;
const DOCK_PAD: f32 = 0.14;
const BACKDROP: u8 = 22;

pub fn mark_image() -> Image {
    rgba_image(&mark_bitmap())
}

pub fn dock_png() -> &'static [u8] {
    static PNG: OnceLock<Vec<u8>> = OnceLock::new();
    PNG.get_or_init(|| encode_png(&dock_bitmap()))
}

fn mark_bitmap() -> RgbaImage {
    static BITMAP: OnceLock<RgbaImage> = OnceLock::new();
    BITMAP
        .get_or_init(|| {
            let mut src = image::load_from_memory(APP_ICON_PNG)
                .expect("app icon png")
                .to_rgba8();
            knockout_backdrop(&mut src);
            src
        })
        .clone()
}

fn dock_bitmap() -> RgbaImage {
    let mark = mark_bitmap();
    let mut canvas = RgbaImage::new(DOCK_SIDE, DOCK_SIDE);
    let inner = ((DOCK_SIDE as f32) * (1.0 - DOCK_PAD * 2.0)).round() as u32;
    let inner = inner.max(1);
    let resized = imageops::resize(&mark, inner, inner, imageops::FilterType::Lanczos3);
    let ox = (DOCK_SIDE - inner) / 2;
    let oy = (DOCK_SIDE - inner) / 2;
    imageops::overlay(&mut canvas, &resized, i64::from(ox), i64::from(oy));
    canvas
}

fn knockout_backdrop(img: &mut RgbaImage) {
    let (w, h) = img.dimensions();
    let mut seen = vec![false; (w * h) as usize];
    let mut stack: Vec<(u32, u32)> = Vec::new();
    for x in 0..w {
        stack.push((x, 0));
        if h > 1 {
            stack.push((x, h - 1));
        }
    }
    for y in 0..h {
        stack.push((0, y));
        if w > 1 {
            stack.push((w - 1, y));
        }
    }
    while let Some((x, y)) = stack.pop() {
        let i = (y * w + x) as usize;
        if seen[i] {
            continue;
        }
        seen[i] = true;
        let pixel = img.get_pixel(x, y).0;
        if !is_backdrop(pixel) {
            continue;
        }
        img.put_pixel(x, y, Rgba([0, 0, 0, 0]));
        if x > 0 {
            stack.push((x - 1, y));
        }
        if x + 1 < w {
            stack.push((x + 1, y));
        }
        if y > 0 {
            stack.push((x, y - 1));
        }
        if y + 1 < h {
            stack.push((x, y + 1));
        }
    }
}

fn is_backdrop(pixel: [u8; 4]) -> bool {
    pixel[3] > 0 && pixel[0] < BACKDROP && pixel[1] < BACKDROP && pixel[2] < BACKDROP
}

fn rgba_image(img: &RgbaImage) -> Image {
    let (w, h) = img.dimensions();
    let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(img.as_raw(), w, h);
    Image::from_rgba8(buffer)
}

fn encode_png(img: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::new();
    let dynimg = image::DynamicImage::ImageRgba8(img.clone());
    dynimg
        .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
        .expect("encoding dock icon");
    out
}
