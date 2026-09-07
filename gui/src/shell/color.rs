use calumma_core::{format_hex_rgb, parse_hex_rgb};
use slint::Color;

pub struct Hsb {
    pub hue: f32,
    pub saturation: f32,
    pub brightness: f32,
}

pub struct QuickColors {
    pub slots: [[u8; 4]; 3],
    pub active: usize,
    pub hsb: Hsb,
}

impl QuickColors {
    pub fn new() -> Self {
        let slots = [
            [26, 26, 26, 255],
            [255, 255, 255, 255],
            [128, 128, 128, 255],
        ];
        Self {
            slots,
            active: 0,
            hsb: rgba_to_hsb(slots[0]),
        }
    }

    pub fn load_from_engine(ink: [u8; 4], stroke: [u8; 4], fill: [u8; 4], select: [u8; 4]) -> Self {
        let slots = [stroke, fill, select];
        let active = slots.iter().position(|slot| *slot == ink).unwrap_or(0);
        let mut colors = Self {
            slots,
            active,
            hsb: rgba_to_hsb(slots[active]),
        };
        if colors.slots[active] != ink {
            colors.slots[active] = ink;
            colors.hsb = rgba_to_hsb(ink);
        }
        colors
    }

    pub fn select(&mut self, index: usize) {
        if index >= self.slots.len() {
            return;
        }
        self.active = index;
        self.hsb = rgba_to_hsb(self.slots[index]);
    }

    pub fn set_hsb(&mut self, hue: f32, saturation: f32, brightness: f32) {
        self.hsb = Hsb {
            hue: hue.clamp(0.0, 1.0),
            saturation: saturation.clamp(0.0, 1.0),
            brightness: brightness.clamp(0.0, 1.0),
        };
        self.slots[self.active] = hsb_to_rgba(&self.hsb);
    }

    pub fn set_saturation_brightness(&mut self, saturation: f32, brightness: f32) {
        self.set_hsb(self.hsb.hue, saturation, brightness);
    }

    pub fn set_hue(&mut self, hue: f32) {
        self.set_hsb(hue, self.hsb.saturation, self.hsb.brightness);
    }

    pub fn commit_hex(&mut self, text: &str) -> String {
        let trimmed = text.trim().trim_start_matches('#');
        let parsed = parse_hex_rgb(trimmed).map(|rgb| [rgb[0], rgb[1], rgb[2], 255]);
        if let Some(rgba) = parsed {
            self.slots[self.active] = rgba;
            self.hsb = rgba_to_hsb(rgba);
        }
        hex_for_slot(self.slots[self.active])
    }

    pub fn hex_text(&self) -> String {
        hex_for_slot(self.slots[self.active])
    }

    pub fn ink_rgba(&self) -> [u8; 4] {
        self.slots[self.active]
    }
}

pub fn hex_for_slot(rgba: [u8; 4]) -> String {
    format_hex_rgb([rgba[0], rgba[1], rgba[2]])
}

pub fn slint_color(rgba: [u8; 4]) -> Color {
    Color::from_rgb_u8(rgba[0], rgba[1], rgba[2])
}

pub fn hue_color(hue: f32) -> Color {
    let rgba = hsb_to_rgba(&Hsb {
        hue: hue.clamp(0.0, 1.0),
        saturation: 1.0,
        brightness: 1.0,
    });
    slint_color(rgba)
}

pub fn rgba_to_hsb(rgba: [u8; 4]) -> Hsb {
    let r = rgba[0] as f32 / 255.0;
    let g = rgba[1] as f32 / 255.0;
    let b = rgba[2] as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let hue = if delta <= f32::EPSILON {
        0.0
    } else if max == r {
        ((g - b) / delta).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / delta + 2.0) / 6.0
    } else {
        ((r - g) / delta + 4.0) / 6.0
    };
    let saturation = if max <= f32::EPSILON {
        0.0
    } else {
        delta / max
    };
    Hsb {
        hue,
        saturation,
        brightness: max,
    }
}

pub fn hsb_to_rgba(hsb: &Hsb) -> [u8; 4] {
    let hue = (hsb.hue * 6.0).fract() * 6.0;
    let c = hsb.brightness * hsb.saturation;
    let x = c * (1.0 - ((hue % 2.0) - 1.0).abs());
    let m = hsb.brightness - c;
    let (r, g, b) = match hue as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [
        ((r + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        255,
    ]
}
