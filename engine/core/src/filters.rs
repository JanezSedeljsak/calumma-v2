use crate::limits::{ADJUSTMENT_NUDGE_STEP, GAMMA_NUDGE_STEP};
use num_enum::{IntoPrimitive, TryFromPrimitive};

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u32)]
pub enum AdjustmentKind {
    Brightness = 0,
    Contrast = 1,
    Vibrance = 2,
    Saturation = 3,
    LevelsGamma = 4,
}

impl AdjustmentKind {
    pub const ALL: [AdjustmentKind; 5] = [
        Self::Brightness,
        Self::Contrast,
        Self::Vibrance,
        Self::Saturation,
        Self::LevelsGamma,
    ];

    pub fn from_u32(value: u32) -> Option<Self> {
        Self::try_from(value).ok()
    }

    pub fn as_u32(self) -> u32 {
        self.into()
    }

    pub fn step(self) -> f32 {
        match self {
            Self::LevelsGamma => GAMMA_NUDGE_STEP,
            _ => ADJUSTMENT_NUDGE_STEP,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Adjustments {
    pub brightness: f32,
    pub contrast: f32,
    pub vibrance: f32,
    pub saturation: f32,
    pub levels_gamma: f32,
}

impl Default for Adjustments {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 0.0,
            vibrance: 0.0,
            saturation: 0.0,
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
            levels_gamma: self.levels_gamma.clamp(0.1, 4.0),
        }
    }

    pub fn is_neutral(&self) -> bool {
        self.brightness == 0.0
            && self.contrast == 0.0
            && self.vibrance == 0.0
            && self.saturation == 0.0
            && self.levels_gamma == 1.0
    }

    pub fn value(&self, kind: AdjustmentKind) -> f32 {
        match kind {
            AdjustmentKind::Brightness => self.brightness,
            AdjustmentKind::Contrast => self.contrast,
            AdjustmentKind::Vibrance => self.vibrance,
            AdjustmentKind::Saturation => self.saturation,
            AdjustmentKind::LevelsGamma => self.levels_gamma,
        }
    }

    pub fn nudged(self, kind: AdjustmentKind, steps: f32) -> Self {
        let delta = kind.step() * steps;
        let mut next = self;
        match kind {
            AdjustmentKind::Brightness => next.brightness += delta,
            AdjustmentKind::Contrast => next.contrast += delta,
            AdjustmentKind::Vibrance => next.vibrance += delta,
            AdjustmentKind::Saturation => next.saturation += delta,
            AdjustmentKind::LevelsGamma => next.levels_gamma += delta,
        }
        next.clamped()
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

/// Gamma → contrast → brightness for one channel. Depends only on that channel's byte,
/// which is what makes the tone stage tabulatable.
fn tone(byte: u8, adj: &Adjustments) -> f32 {
    let c = (byte as f32 / 255.0)
        .clamp(0.0, 1.0)
        .powf(1.0 / adj.levels_gamma.max(1e-4));
    let contrast_factor = (1.0 + adj.contrast).max(0.0);
    ((c - 0.5) * contrast_factor + 0.5 + adj.brightness).clamp(0.0, 1.0)
}

/// Saturation and vibrance both only move `s`, so they share one HSL round trip instead
/// of one each. Besides halving the trigonometry-ish work, this keeps the hue: going out
/// to RGB between the two stages loses it whenever saturation lands a pixel on gray, and
/// vibrance would then re-saturate that pixel toward red.
fn hsl_stage(v: [f32; 3], adj: &Adjustments) -> [f32; 3] {
    if adj.saturation == 0.0 && adj.vibrance == 0.0 {
        return v;
    }
    let [h, mut s, l] = rgb_to_hsl(v);
    if adj.saturation != 0.0 {
        s = (s * (1.0 + adj.saturation)).clamp(0.0, 1.0);
    }
    if adj.vibrance != 0.0 {
        s = (s + adj.vibrance * (1.0 - s)).clamp(0.0, 1.0);
    }
    hsl_to_rgb([h, s, l])
}

fn quantize(v: [f32; 3]) -> [u8; 3] {
    [
        (v[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (v[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (v[2].clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

pub fn apply(rgb: [u8; 3], adj: &Adjustments) -> [u8; 3] {
    if adj.is_neutral() {
        return rgb;
    }
    let v = [tone(rgb[0], adj), tone(rgb[1], adj), tone(rgb[2], adj)];
    quantize(hsl_stage(v, adj))
}

/// Precomputed form of [`Adjustments`] for bulk pixel work — build it once per buffer
/// instead of paying `powf` per channel per pixel. Results are identical to [`apply`]:
/// the tone stage is a pure per-channel function of the input byte, so tabulating it
/// over all 256 inputs is exact rather than an approximation.
///
/// When saturation and vibrance are both neutral there is no channel coupling at all
/// and the whole filter collapses to one `[u8; 256]` lookup per channel.
#[derive(Clone)]
pub struct AdjustmentLut {
    tone: [f32; 256],
    direct: Option<[u8; 256]>,
    adj: Adjustments,
    neutral: bool,
}

impl AdjustmentLut {
    pub fn new(adj: &Adjustments) -> Self {
        let neutral = adj.is_neutral();
        let mut tone_lut = [0.0f32; 256];
        for (byte, slot) in tone_lut.iter_mut().enumerate() {
            *slot = tone(byte as u8, adj);
        }
        let direct = (adj.saturation == 0.0 && adj.vibrance == 0.0).then(|| {
            let mut out = [0u8; 256];
            for (byte, slot) in out.iter_mut().enumerate() {
                *slot = (tone_lut[byte].clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            out
        });
        Self {
            tone: tone_lut,
            direct,
            adj: *adj,
            neutral,
        }
    }

    /// True when the filter is a no-op and callers can skip the buffer walk entirely.
    pub fn is_neutral(&self) -> bool {
        self.neutral
    }

    /// True when saturation and vibrance are both neutral, so the whole filter is three
    /// independent per-channel lookups with no HSL round trip — `Self::direct`'s condition,
    /// exposed for the GPU path (`LayerData.lut_mode` in `render/src/renderer.rs`) to pick the
    /// cheaper of `fs_tile`'s two branches without re-deriving it from `Adjustments` itself.
    pub fn is_tone_only(&self) -> bool {
        self.direct.is_some()
    }

    /// The per-byte tone table alone — gamma → contrast → brightness, channel-agnostic — for a
    /// caller that applies the HSL stage itself. `fs_tile` reads this as `LayerData.tone` and
    /// mirrors `hsl_stage` in WGSL by hand: there is no code generation between the two, so a
    /// change here has to be carried into `board.wgsl`'s copy by whoever makes it — the render
    /// crate's `layer_table_tests` byte-cube tests are what catch a drift.
    pub fn tone_table(&self) -> &[f32; 256] {
        &self.tone
    }

    #[inline]
    pub fn apply(&self, rgb: [u8; 3]) -> [u8; 3] {
        if self.neutral {
            return rgb;
        }
        if let Some(direct) = &self.direct {
            return [
                direct[rgb[0] as usize],
                direct[rgb[1] as usize],
                direct[rgb[2] as usize],
            ];
        }
        let v = [
            self.tone[rgb[0] as usize],
            self.tone[rgb[1] as usize],
            self.tone[rgb[2] as usize],
        ];
        quantize(hsl_stage(v, &self.adj))
    }

    /// Filter the RGB of every pixel in a tightly packed RGBA buffer, leaving alpha alone.
    pub fn apply_rgba(&self, rgba: &mut [u8]) {
        if self.neutral {
            return;
        }
        for px in rgba.chunks_exact_mut(4) {
            let out = self.apply([px[0], px[1], px[2]]);
            px[0..3].copy_from_slice(&out);
        }
    }
}

impl Adjustments {
    pub fn lut(&self) -> AdjustmentLut {
        AdjustmentLut::new(self)
    }
}
