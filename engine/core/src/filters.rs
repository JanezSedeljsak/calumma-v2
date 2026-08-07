#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Adjustments {
    pub brightness: f32,
    pub contrast: f32,
    pub vibrance: f32,
    pub saturation: f32,
    pub levels_black: f32,
    pub levels_white: f32,
    pub levels_gamma: f32,
}

impl Default for Adjustments {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 0.0,
            vibrance: 0.0,
            saturation: 0.0,
            levels_black: 0.0,
            levels_white: 1.0,
            levels_gamma: 1.0,
        }
    }
}

impl Adjustments {
    pub fn clamped(self) -> Self {
        Self {
            brightness: self.brightness.clamp(-1.0, 1.0),
            contrast: self.contrast.clamp(-1.0, 1.0),
            vibrance: self.vibrance.clamp(-1.0, 1.0),
            saturation: self.saturation.clamp(-1.0, 1.0),
            levels_black: self.levels_black.clamp(0.0, 1.0),
            levels_white: self.levels_white.clamp(0.0, 1.0),
            levels_gamma: self.levels_gamma.clamp(0.1, 4.0),
        }
    }

    pub fn is_neutral(&self) -> bool {
        self.brightness == 0.0
            && self.contrast == 0.0
            && self.vibrance == 0.0
            && self.saturation == 0.0
            && self.levels_black == 0.0
            && self.levels_white == 1.0
            && self.levels_gamma == 1.0
    }
}

fn rgb_to_hsl(rgb: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = rgb;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) * 0.5;
    if (max - min).abs() < 1e-6 {
        return [0.0, 0.0, l];
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        ((g - b) / d) % 6.0
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [(h / 6.0).rem_euclid(1.0), s, l]
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 0.5 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}

fn hsl_to_rgb(hsl: [f32; 3]) -> [f32; 3] {
    let [h, s, l] = hsl;
    if s <= 1e-6 {
        return [l, l, l];
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    [
        hue_to_rgb(p, q, h + 1.0 / 3.0),
        hue_to_rgb(p, q, h),
        hue_to_rgb(p, q, h - 1.0 / 3.0),
    ]
}

pub fn apply(rgb: [u8; 3], adj: &Adjustments) -> [u8; 3] {
    if adj.is_neutral() {
        return rgb;
    }
    let mut v = [
        rgb[0] as f32 / 255.0,
        rgb[1] as f32 / 255.0,
        rgb[2] as f32 / 255.0,
    ];

    let white = adj.levels_white.max(adj.levels_black + 1e-4);
    for c in &mut v {
        *c = ((*c - adj.levels_black) / (white - adj.levels_black)).clamp(0.0, 1.0);
        *c = c.powf(1.0 / adj.levels_gamma.max(1e-4));
    }

    let contrast_factor = (1.0 + adj.contrast).max(0.0);
    for c in &mut v {
        *c = ((*c - 0.5) * contrast_factor + 0.5 + adj.brightness).clamp(0.0, 1.0);
    }

    if adj.saturation != 0.0 {
        let [h, s, l] = rgb_to_hsl(v);
        let s = (s * (1.0 + adj.saturation)).clamp(0.0, 1.0);
        v = hsl_to_rgb([h, s, l]);
    }

    if adj.vibrance != 0.0 {
        let [h, s, l] = rgb_to_hsl(v);
        let boost = adj.vibrance * (1.0 - s);
        let s = (s + boost).clamp(0.0, 1.0);
        v = hsl_to_rgb([h, s, l]);
    }

    [
        (v[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (v[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (v[2].clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_adjustments_are_a_no_op() {
        let adj = Adjustments::default();
        assert!(adj.is_neutral());
        assert_eq!(apply([12, 200, 88], &adj), [12, 200, 88]);
    }

    #[test]
    fn brightness_lightens_every_channel() {
        let adj = Adjustments {
            brightness: 0.2,
            ..Adjustments::default()
        };
        let out = apply([100, 100, 100], &adj);
        assert!(out[0] > 100 && out[1] > 100 && out[2] > 100);
    }

    #[test]
    fn contrast_pushes_values_away_from_midpoint() {
        let adj = Adjustments {
            contrast: 0.5,
            ..Adjustments::default()
        };
        let bright = apply([200, 200, 200], &adj);
        let dark = apply([50, 50, 50], &adj);
        assert!(bright[0] > 200);
        assert!(dark[0] < 50);
    }

    #[test]
    fn saturation_minus_one_desaturates_to_gray() {
        let adj = Adjustments {
            saturation: -1.0,
            ..Adjustments::default()
        };
        let out = apply([200, 50, 50], &adj);
        assert_eq!(out[0], out[1]);
        assert_eq!(out[1], out[2]);
    }

    #[test]
    fn levels_black_white_stretch_clips_range() {
        let adj = Adjustments {
            levels_black: 0.25,
            levels_white: 0.75,
            ..Adjustments::default()
        };
        let low = apply([40, 40, 40], &adj);
        let high = apply([220, 220, 220], &adj);
        assert_eq!(low, [0, 0, 0]);
        assert_eq!(high, [255, 255, 255]);
    }

    #[test]
    fn clamped_keeps_values_in_sane_ranges() {
        let adj = Adjustments {
            brightness: 5.0,
            contrast: -9.0,
            levels_gamma: 0.0,
            ..Adjustments::default()
        }
        .clamped();
        assert_eq!(adj.brightness, 1.0);
        assert_eq!(adj.contrast, -1.0);
        assert_eq!(adj.levels_gamma, 0.1);
    }
}
