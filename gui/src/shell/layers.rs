use calumma_app::Engine;
use slint::{Image, SharedPixelBuffer};

const ROW_THUMB_W: u32 = 40;
const ROW_THUMB_H: u32 = 40;
const PREVIEW_SIDE: u32 = 160;
const CHECKER: u32 = 8;

type LayerMeta = (usize, String, bool, bool, bool);

pub struct LayerThumbCache {
    revisions: Vec<u64>,
    row_images: Vec<Image>,
    preview_images: Vec<Image>,
    meta: Vec<LayerMeta>,
}

impl LayerThumbCache {
    pub fn new() -> Self {
        Self {
            revisions: Vec::new(),
            row_images: Vec::new(),
            preview_images: Vec::new(),
            meta: Vec::new(),
        }
    }

    pub fn row_image(&self, index: usize) -> Image {
        self.row_images
            .get(index)
            .cloned()
            .unwrap_or_else(|| fit_on_checker(&[], 0, 0, ROW_THUMB_W, ROW_THUMB_H))
    }

    pub fn preview_image(&self, index: usize) -> Image {
        self.preview_images
            .get(index)
            .cloned()
            .unwrap_or_else(|| fit_on_checker(&[], 0, 0, PREVIEW_SIDE, PREVIEW_SIDE))
    }

    pub fn sync(&mut self, engine: &Engine) -> bool {
        let layers = engine.list_layers();
        let count = layers.len();
        let mut changed = self.revisions.len() != count;
        if changed {
            self.revisions.clear();
            self.row_images.clear();
            self.preview_images.clear();
        }
        self.revisions.resize(count, u64::MAX);
        self.row_images
            .resize(count, fit_on_checker(&[], 0, 0, ROW_THUMB_W, ROW_THUMB_H));
        self.preview_images
            .resize(count, fit_on_checker(&[], 0, 0, PREVIEW_SIDE, PREVIEW_SIDE));

        for layer in &layers {
            let index = layer.index;
            let revision = engine.layer_preview_revision(index);
            if self.revisions.get(index) == Some(&revision) {
                continue;
            }
            changed = true;
            let (w, h, rgba) = engine
                .layer_thumbnail_rgba(index)
                .unwrap_or((0, 0, Vec::new()));
            self.row_images[index] = fit_on_checker(&rgba, w, h, ROW_THUMB_W, ROW_THUMB_H);
            self.preview_images[index] = fit_on_checker(&rgba, w, h, PREVIEW_SIDE, PREVIEW_SIDE);
            self.revisions[index] = revision;
        }

        let meta: Vec<LayerMeta> = layers
            .iter()
            .map(|layer| {
                (
                    layer.index,
                    layer.name.clone(),
                    layer.visible,
                    layer.locked,
                    layer.active,
                )
            })
            .collect();
        if meta != self.meta {
            changed = true;
            self.meta = meta;
        }
        changed
    }
}

fn fit_on_checker(rgba: &[u8], w: u32, h: u32, dw: u32, dh: u32) -> Image {
    let mut out = checkerboard(dw, dh);
    if w == 0 || h == 0 || rgba.len() < (w * h * 4) as usize {
        return rgba_image(&out, dw, dh);
    }
    let scale = (dw as f32 / w as f32).min(dh as f32 / h as f32).min(1.0);
    let tw = ((w as f32) * scale).round().max(1.0) as u32;
    let th = ((h as f32) * scale).round().max(1.0) as u32;
    let ox = (dw.saturating_sub(tw)) / 2;
    let oy = (dh.saturating_sub(th)) / 2;
    for y in 0..th {
        let sy = ((y as f32 / th as f32) * h as f32) as u32;
        for x in 0..tw {
            let sx = ((x as f32 / tw as f32) * w as f32) as u32;
            let src = ((sy * w + sx) * 4) as usize;
            let dx = ox + x;
            let dy = oy + y;
            if src + 3 >= rgba.len() || dx >= dw || dy >= dh {
                continue;
            }
            let dst = ((dy * dw + dx) * 4) as usize;
            let a = f32::from(rgba[src + 3]) / 255.0;
            if a <= 0.0 {
                continue;
            }
            let inv = 1.0 - a;
            out[dst] = (f32::from(rgba[src]) * a + f32::from(out[dst]) * inv).round() as u8;
            out[dst + 1] =
                (f32::from(rgba[src + 1]) * a + f32::from(out[dst + 1]) * inv).round() as u8;
            out[dst + 2] =
                (f32::from(rgba[src + 2]) * a + f32::from(out[dst + 2]) * inv).round() as u8;
            out[dst + 3] = 255;
        }
    }
    rgba_image(&out, dw, dh)
}

fn checkerboard(w: u32, h: u32) -> Vec<u8> {
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let on = ((x / CHECKER) + (y / CHECKER)).is_multiple_of(2);
            let v = if on { 72 } else { 48 };
            rgba[i] = v;
            rgba[i + 1] = v;
            rgba[i + 2] = v;
            rgba[i + 3] = 255;
        }
    }
    rgba
}

fn rgba_image(rgba: &[u8], w: u32, h: u32) -> Image {
    let buffer = SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(rgba, w, h);
    Image::from_rgba8(buffer)
}
