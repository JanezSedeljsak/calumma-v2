//! Layer blend modes as the W3C Compositing and Blending spec defines them — the same
//! formulas Photoshop's menu, PDF's `/BM` and CSS `mix-blend-mode` share. Every function takes
//! straight (unpremultiplied) colour in the stored gamma space, 0–1: that is where Photoshop
//! blends, and it is what a byte in a tile already is.
//!
//! `board.wgsl` evaluates the same functions (`blend_rgb` there) on the live board, so a
//! layer looks the same flattened, exported and on screen.

use crate::layer::BlendMode;

/// The blend function `B(Cb, Cs)` for a whole pixel. The four non-separable modes need all
/// three channels at once; every other mode is the same function applied per channel.
pub fn blend_rgb(mode: BlendMode, backdrop: [f32; 3], source: [f32; 3]) -> [f32; 3] {
    match mode {
        BlendMode::DarkerColor => {
            if lum(source) < lum(backdrop) {
                source
            } else {
                backdrop
            }
        }
        BlendMode::LighterColor => {
            if lum(source) > lum(backdrop) {
                source
            } else {
                backdrop
            }
        }
        BlendMode::Hue => set_lum(set_sat(source, sat(backdrop)), lum(backdrop)),
        BlendMode::Saturation => set_lum(set_sat(backdrop, sat(source)), lum(backdrop)),
        BlendMode::Color => set_lum(source, lum(backdrop)),
        BlendMode::Luminosity => set_lum(backdrop, lum(source)),
        separable => std::array::from_fn(|i| blend_channel(separable, backdrop[i], source[i])),
    }
}

fn blend_channel(mode: BlendMode, b: f32, s: f32) -> f32 {
    match mode {
        BlendMode::Normal => s,
        BlendMode::Multiply => b * s,
        BlendMode::Screen => screen(b, s),
        BlendMode::Darken => b.min(s),
        BlendMode::Lighten => b.max(s),
        BlendMode::ColorBurn => color_burn(b, s),
        BlendMode::ColorDodge => color_dodge(b, s),
        BlendMode::LinearBurn => (b + s - 1.0).max(0.0),
        BlendMode::LinearDodge => (b + s).min(1.0),
        BlendMode::Overlay => hard_light(s, b),
        BlendMode::HardLight => hard_light(b, s),
        BlendMode::SoftLight => soft_light(b, s),
        BlendMode::VividLight => {
            if s <= 0.5 {
                color_burn(b, 2.0 * s)
            } else {
                color_dodge(b, 2.0 * s - 1.0)
            }
        }
        BlendMode::LinearLight => (b + 2.0 * s - 1.0).clamp(0.0, 1.0),
        BlendMode::PinLight => {
            if s <= 0.5 {
                b.min(2.0 * s)
            } else {
                b.max(2.0 * s - 1.0)
            }
        }
        BlendMode::HardMix => {
            if b + s >= 1.0 {
                1.0
            } else {
                0.0
            }
        }
        BlendMode::Difference => (b - s).abs(),
        BlendMode::Exclusion => b + s - 2.0 * b * s,
        BlendMode::Subtract => (b - s).max(0.0),
        BlendMode::Divide => {
            if s <= 0.0 {
                if b > 0.0 {
                    1.0
                } else {
                    0.0
                }
            } else {
                (b / s).min(1.0)
            }
        }
        BlendMode::DarkerColor
        | BlendMode::LighterColor
        | BlendMode::Hue
        | BlendMode::Saturation
        | BlendMode::Color
        | BlendMode::Luminosity => s,
    }
}

fn screen(b: f32, s: f32) -> f32 {
    b + s - b * s
}

/// `backdrop` lit by `source`; Overlay is the same function with the two swapped.
fn hard_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        b * 2.0 * s
    } else {
        screen(b, 2.0 * s - 1.0)
    }
}

fn color_dodge(b: f32, s: f32) -> f32 {
    if b <= 0.0 {
        0.0
    } else if s >= 1.0 {
        1.0
    } else {
        (b / (1.0 - s)).min(1.0)
    }
}

fn color_burn(b: f32, s: f32) -> f32 {
    if b >= 1.0 {
        1.0
    } else if s <= 0.0 {
        0.0
    } else {
        1.0 - ((1.0 - b) / s).min(1.0)
    }
}

fn soft_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        return b - (1.0 - 2.0 * s) * b * (1.0 - b);
    }
    let d = if b <= 0.25 {
        ((16.0 * b - 12.0) * b + 4.0) * b
    } else {
        b.sqrt()
    };
    b + (2.0 * s - 1.0) * (d - b)
}

pub fn lum(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn clip_color(c: [f32; 3]) -> [f32; 3] {
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    let mut out = c;
    if n < 0.0 {
        out = out.map(|v| l + (v - l) * l / (l - n));
    }
    if x > 1.0 {
        out = out.map(|v| l + (v - l) * (1.0 - l) / (x - l));
    }
    out
}

fn set_lum(c: [f32; 3], l: f32) -> [f32; 3] {
    let d = l - lum(c);
    clip_color(c.map(|v| v + d))
}

pub fn sat(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

/// Stretch `c` so its spread is `s`: the lowest channel to 0, the highest to `s`, the middle one
/// kept in proportion — the spec's sort-based definition, written without the sort.
fn set_sat(c: [f32; 3], s: f32) -> [f32; 3] {
    let hi = c[0].max(c[1]).max(c[2]);
    let lo = c[0].min(c[1]).min(c[2]);
    if hi <= lo {
        return [0.0; 3];
    }
    c.map(|v| (v - lo) * s / (hi - lo))
}
