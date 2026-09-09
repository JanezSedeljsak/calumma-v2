use calumma_app::Engine;
use slint::{Image, SharedPixelBuffer};

const ROW_THUMB_W: u32 = 40;
const ROW_THUMB_H: u32 = 40;
const PREVIEW_SIDE: u32 = 160;
const CHECKER_CELL: u32 = 4;

type LayerMeta = (usize, String, bool, bool, bool, bool, bool);

pub struct LayerThumbCache {
    revisions: Vec<u64>,
    row_images: Vec<Image>,
    preview_images: Vec<Image>,
    meta: Vec<LayerMeta>,
    dark: Option<bool>,
}

impl LayerThumbCache {
    pub fn new() -> Self {
        Self {
            revisions: Vec::new(),
            row_images: Vec::new(),
            preview_images: Vec::new(),
            meta: Vec::new(),
            dark: None,
        }
    }

    pub fn row_image(&self, index: usize) -> Image {
        self.row_images
            .get(index)
            .cloned()
            .unwrap_or_else(|| fit_thumb(&[], 0, 0, ROW_THUMB_W, ROW_THUMB_H, true))
    }

    pub fn preview_image(&self, index: usize) -> Image {
        self.preview_images
            .get(index)
            .cloned()
            .unwrap_or_else(|| fit_thumb(&[], 0, 0, PREVIEW_SIDE, PREVIEW_SIDE, true))
    }

    pub fn sync(&mut self, engine: &Engine, dark: bool) -> bool {
        let layers = engine.list_layers();
        let count = layers.len();
        let mut changed = self.revisions.len() != count || self.dark != Some(dark);
        if changed {
            self.revisions.clear();
            self.row_images.clear();
            self.preview_images.clear();
        }
        self.dark = Some(dark);
        self.revisions.resize(count, u64::MAX);
        self.row_images
            .resize(count, fit_thumb(&[], 0, 0, ROW_THUMB_W, ROW_THUMB_H, dark));
        self.preview_images.resize(
            count,
            fit_thumb(&[], 0, 0, PREVIEW_SIDE, PREVIEW_SIDE, dark),
        );

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
            self.row_images[index] = fit_thumb(&rgba, w, h, ROW_THUMB_W, ROW_THUMB_H, dark);
            self.preview_images[index] = fit_thumb(&rgba, w, h, PREVIEW_SIDE, PREVIEW_SIDE, dark);
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
                    layer.clipped,
                    layer.clip_base,
                )
            })
            .collect();
        if meta != self.meta {
            changed = true;
            self.meta = meta;
        }
        changed
    }

    pub fn invalidate(&mut self) {
        for revision in &mut self.revisions {
            *revision = u64::MAX;
        }
    }
}

fn checker_rgb(x: u32, y: u32, dark: bool) -> (u8, u8, u8) {
    let on = ((x / CHECKER_CELL) + (y / CHECKER_CELL)) % 2 == 0;
    if dark {
        if on {
            (0x24, 0x2C, 0x32)
        } else {
            (0x2A, 0x32, 0x38)
        }
    } else if on {
        (0xE2, 0xE8, 0xEE)
    } else {
        (0xDA, 0xE0, 0xE6)
    }
}

fn fill_checker(out: &mut [u8], dw: u32, dh: u32, dark: bool) {
    for y in 0..dh {
        for x in 0..dw {
            let (r, g, b) = checker_rgb(x, y, dark);
            let i = ((y * dw + x) * 4) as usize;
            out[i] = r;
            out[i + 1] = g;
            out[i + 2] = b;
            out[i + 3] = 255;
        }
    }
}

fn blend_over(dst: &mut [u8], src: &[u8]) {
    let a = src[3] as u32;
    if a == 0 {
        return;
    }
    if a == 255 {
        dst[0] = src[0];
        dst[1] = src[1];
        dst[2] = src[2];
        dst[3] = 255;
        return;
    }
    let ia = 255 - a;
    dst[0] = ((src[0] as u32 * a + dst[0] as u32 * ia) / 255) as u8;
    dst[1] = ((src[1] as u32 * a + dst[1] as u32 * ia) / 255) as u8;
    dst[2] = ((src[2] as u32 * a + dst[2] as u32 * ia) / 255) as u8;
    dst[3] = 255;
}

fn fit_thumb(rgba: &[u8], w: u32, h: u32, dw: u32, dh: u32, dark: bool) -> Image {
    let mut out = vec![0u8; (dw * dh * 4) as usize];
    fill_checker(&mut out, dw, dh, dark);
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
            blend_over(&mut out[dst..dst + 4], &rgba[src..src + 4]);
        }
    }
    rgba_image(&out, dw, dh)
}

fn rgba_image(rgba: &[u8], w: u32, h: u32) -> Image {
    let buffer = SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(rgba, w, h);
    Image::from_rgba8(buffer)
}
