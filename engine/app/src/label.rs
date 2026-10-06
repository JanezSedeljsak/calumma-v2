use calumma_text::{default_family, family_exists, rasterize, TextRun};

const LABEL_FAMILY_PREFERENCE: &[&str] = &["System Font", "Segoe UI"];

/// A chrome label already turned into device pixels by the engine's own text stack, so the
/// shell can show it as an unrotated image on whole pixels. `ink_x`/`ink_y` place the bitmap's
/// top-left corner relative to where the line starts — the top-left of the line box in
/// reading orientation — because glyph ink never starts exactly there.
pub struct LabelBitmap {
    pub width: u32,
    pub height: u32,
    pub ink_x: i32,
    pub ink_y: i32,
    pub rgba: Vec<u8>,
}

fn label_family() -> String {
    LABEL_FAMILY_PREFERENCE
        .iter()
        .find(|family| family_exists(family))
        .map(|family| family.to_string())
        .unwrap_or_else(default_family)
}

/// `turned` reads bottom to top: the bitmap is rotated a quarter turn counter-clockwise by
/// moving whole pixels, never resampled, and the ink offsets follow it — the line's start is
/// then its bottom-left corner on screen, with the glyph tops facing left.
pub fn label_bitmap(text: &str, size_px: f32, color: [u8; 4], turned: bool) -> Option<LabelBitmap> {
    let run = TextRun {
        text: text.to_string(),
        family: label_family(),
        size: size_px,
        color,
        ..TextRun::default()
    };
    let raster = rasterize(&run)?;
    if !turned {
        return Some(LabelBitmap {
            width: raster.width,
            height: raster.height,
            ink_x: raster.origin_x,
            ink_y: raster.origin_y,
            rgba: raster.rgba,
        });
    }
    let (w, h) = (raster.width as usize, raster.height as usize);
    let mut rgba = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let src = (y * w + x) * 4;
            let dst = ((w - 1 - x) * h + y) * 4;
            rgba[dst..dst + 4].copy_from_slice(&raster.rgba[src..src + 4]);
        }
    }
    Some(LabelBitmap {
        width: raster.height,
        height: raster.width,
        ink_x: raster.origin_y,
        ink_y: -(raster.origin_x + raster.width as i32),
        rgba,
    })
}
