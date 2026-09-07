use calumma_app::Engine;
use slint::{Image, SharedPixelBuffer};

const ROW_THUMB_SIDE: u32 = 40;
const PREVIEW_SIDE: u32 = 160;

pub struct LayerThumbCache {
    revisions: Vec<u64>,
    row_images: Vec<Image>,
    preview_images: Vec<Image>,
}

impl LayerThumbCache {
    pub fn new() -> Self {
        Self {
            revisions: Vec::new(),
            row_images: Vec::new(),
            preview_images: Vec::new(),
        }
    }

    pub fn row_image(&self, index: usize) -> Image {
        self.row_images
            .get(index)
            .cloned()
            .unwrap_or_else(placeholder_image)
    }

    pub fn preview_image(&self, index: usize) -> Image {
        self.preview_images
            .get(index)
            .cloned()
            .unwrap_or_else(placeholder_image)
    }

    pub fn sync(&mut self, engine: &Engine) {
        let layers = engine.list_layers();
        let count = layers.len();
        self.revisions.resize(count, 0);
        self.row_images.resize(count, placeholder_image());
        self.preview_images.resize(count, placeholder_image());

        for layer in layers {
            let index = layer.index;
            let revision = engine.layer_preview_revision(index);
            if self.revisions.get(index) == Some(&revision) {
                continue;
            }
            if let Some((w, h, rgba)) = engine.layer_thumbnail_rgba(index) {
                self.row_images[index] = rgba_image(&rgba, w, h);
                if let Some((pw, ph, preview)) = scaled_preview(&rgba, w, h, PREVIEW_SIDE) {
                    self.preview_images[index] = rgba_image(&preview, pw, ph);
                }
            }
            self.revisions[index] = revision;
        }
    }
}

fn scaled_preview(rgba: &[u8], w: u32, h: u32, max_side: u32) -> Option<(u32, u32, Vec<u8>)> {
    if w == 0 || h == 0 {
        return None;
    }
    let scale = (max_side as f32 / w.max(h) as f32).min(1.0);
    let pw = ((w as f32) * scale).round().max(1.0) as u32;
    let ph = ((h as f32) * scale).round().max(1.0) as u32;
    let mut out = vec![0u8; (pw * ph * 4) as usize];
    for y in 0..ph {
        for x in 0..pw {
            let sx = ((x as f32 / pw as f32) * w as f32) as u32;
            let sy = ((y as f32 / ph as f32) * h as f32) as u32;
            let src = ((sy * w + sx) * 4) as usize;
            let dst = ((y * pw + x) * 4) as usize;
            if src + 3 < rgba.len() && dst + 3 < out.len() {
                out[dst..dst + 4].copy_from_slice(&rgba[src..src + 4]);
            }
        }
    }
    Some((pw, ph, out))
}

fn rgba_image(rgba: &[u8], w: u32, h: u32) -> Image {
    let buffer = SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(rgba, w, h);
    Image::from_rgba8(buffer)
}

fn placeholder_image() -> Image {
    let side = ROW_THUMB_SIDE;
    let mut rgba = vec![0u8; (side * side * 4) as usize];
    for y in 0..side {
        for x in 0..side {
            let i = ((y * side + x) * 4) as usize;
            let v = if (x + y) % 8 < 4 { 48 } else { 40 };
            rgba[i] = v;
            rgba[i + 1] = v;
            rgba[i + 2] = v;
            rgba[i + 3] = 255;
        }
    }
    rgba_image(&rgba, side, side)
}
