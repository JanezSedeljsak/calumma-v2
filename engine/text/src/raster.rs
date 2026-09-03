use crate::buffer::build_buffer;
use crate::fonts::with_engine;
use crate::limits::TEXT_RASTER_MAX_SIDE;
use crate::run::TextRun;
use cosmic_text::Color;

/// A laid-out run turned into pixels. `origin` is the document-space position of the
/// bitmap's top-left corner — glyphs overhang the run origin (descenders, italic side
/// bearings, diacritics), so it is not the same point as `TextRun::origin`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextRaster {
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Copy)]
struct Bounds {
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
}

impl Bounds {
    fn empty() -> Self {
        Self {
            min_x: i32::MAX,
            min_y: i32::MAX,
            max_x: i32::MIN,
            max_y: i32::MIN,
        }
    }

    fn add(&mut self, x: i32, y: i32, w: u32, h: u32) {
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x + w as i32);
        self.max_y = self.max_y.max(y + h as i32);
    }

    fn size(&self) -> Option<(u32, u32)> {
        if self.min_x >= self.max_x || self.min_y >= self.max_y {
            return None;
        }
        let w = (self.max_x - self.min_x) as u32;
        let h = (self.max_y - self.min_y) as u32;
        if w > TEXT_RASTER_MAX_SIDE || h > TEXT_RASTER_MAX_SIDE {
            return None;
        }
        Some((w, h))
    }
}

fn over(dst: [u8; 4], src: [u8; 4]) -> [u8; 4] {
    let sa = src[3] as u32;
    if sa == 0 {
        return dst;
    }
    if sa == 255 {
        return src;
    }
    let da = dst[3] as u32;
    let inv = 255 - sa;
    let out_a = sa * 255 + da * inv;
    if out_a == 0 {
        return [0; 4];
    }
    let bias = out_a / 2;
    let channel =
        |i: usize| (((src[i] as u32 * sa * 255) + (dst[i] as u32 * da * inv) + bias) / out_a) as u8;
    [
        channel(0),
        channel(1),
        channel(2),
        ((out_a + 127) / 255) as u8,
    ]
}

/// Draws the run twice: once to learn the exact pixel extent the glyphs cover, once to fill
/// it. The second pass is nearly free — the first has already warmed every glyph in the
/// swash cache — and it is what keeps overhanging ink from being clipped by a box guessed
/// from advance widths.
pub fn rasterize(run: &TextRun) -> Option<TextRaster> {
    if run.is_empty() {
        return None;
    }
    let color = Color::rgba(run.color[0], run.color[1], run.color[2], run.color[3]);
    with_engine(|engine| {
        let mut buffer = build_buffer(engine, run);

        let mut bounds = Bounds::empty();
        buffer.draw(
            &mut engine.font_system,
            &mut engine.swash_cache,
            color,
            |x, y, w, h, pixel| {
                if pixel.a() > 0 {
                    bounds.add(x, y, w, h);
                }
            },
        );
        let (width, height) = bounds.size()?;

        let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
        let (min_x, min_y) = (bounds.min_x, bounds.min_y);
        buffer.draw(
            &mut engine.font_system,
            &mut engine.swash_cache,
            color,
            |x, y, w, h, pixel| {
                if pixel.a() == 0 {
                    return;
                }
                let src = [pixel.r(), pixel.g(), pixel.b(), pixel.a()];
                for dy in 0..h as i32 {
                    for dx in 0..w as i32 {
                        let px = x + dx - min_x;
                        let py = y + dy - min_y;
                        if px < 0 || py < 0 || px >= width as i32 || py >= height as i32 {
                            continue;
                        }
                        let i = ((py as usize) * (width as usize) + px as usize) * 4;
                        let dst = [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]];
                        rgba[i..i + 4].copy_from_slice(&over(dst, src));
                    }
                }
            },
        );

        Some(TextRaster {
            origin_x: run.origin.0.floor() as i32 + min_x,
            origin_y: run.origin.1.floor() as i32 + min_y,
            width,
            height,
            rgba,
        })
    })
}
