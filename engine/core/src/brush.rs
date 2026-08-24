//! What a stroke's ink looks like where it lands: edge falloff, how much ink a pass lays
//! down, and how much the paper's tooth bites into it.
//!
//! Every number here is reached by both the CPU commit (`coverage.rs`) and the GPU preview
//! (`board.wgsl`), which is why the profile travels to the shader as instance data rather
//! than being looked up there — one table, in the engine, and the shader never holds a second
//! copy that can drift from it.

use num_enum::{IntoPrimitive, TryFromPrimitive};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, IntoPrimitive, TryFromPrimitive)]
#[repr(u32)]
pub enum Brush {
    #[default]
    Pen = 0,
    Marker = 1,
    Crayon = 2,
    Airbrush = 3,
}

/// How a brush turns distance-from-the-stroke into ink.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrushProfile {
    /// 1.0 is a hard edge feathered over exactly one pixel — the pen Calumma has always had.
    /// Below that, the falloff reaches `radius * (1 - hardness)` back in from the edge, so a
    /// soft brush covers the same width as a hard one and the size slider keeps its meaning.
    pub hardness: f32,
    /// Ink laid down by one pass, as a fraction of the chosen color's alpha. What makes a
    /// marker translucent and an airbrush a whisper without touching the opacity slider.
    pub flow: f32,
    /// How deeply the paper's tooth bites into coverage. Zero for the smooth brushes.
    pub grain: f32,
    /// Tooth size in **document** pixels. Document-space on purpose: paper grain belongs to
    /// the paper, so it stays put under a stroke instead of sliding along with it.
    pub grain_scale: f32,
}

impl BrushProfile {
    /// The unshaped profile: a hard edge, full flow, no grain. What the eraser and every
    /// non-brush overlay (selection outlines, transform handles) draw with, and what makes
    /// `Brush::Pen` byte-identical to the pen that existed before brushes did.
    pub const HARD: Self = Self {
        hardness: 1.0,
        flow: 1.0,
        grain: 0.0,
        grain_scale: 1.0,
    };
}

impl Brush {
    pub fn from_u32(v: u32) -> Option<Self> {
        Self::try_from(v).ok()
    }

    pub fn profile(self) -> BrushProfile {
        match self {
            Self::Pen => BrushProfile::HARD,
            Self::Marker => BrushProfile {
                hardness: 0.92,
                flow: 0.62,
                grain: 0.0,
                grain_scale: 1.0,
            },
            Self::Crayon => BrushProfile {
                hardness: 0.55,
                flow: 0.78,
                grain: 0.62,
                grain_scale: 2.4,
            },
            Self::Airbrush => BrushProfile {
                hardness: 0.0,
                flow: 0.28,
                grain: 0.0,
                grain_scale: 1.0,
            },
        }
    }
}

/// Ink at one pixel, given its distance from the stroke's centre line.
///
/// The result is *coverage*, not color: a stroke accumulates the maximum coverage any part
/// of it reaches at each pixel and is composited once, so passing over a pixel twice within
/// one stroke cannot darken it. That is the whole reason a low-opacity stroke reads as one
/// even wash instead of a string of overlapping blobs.
pub fn stroke_coverage(profile: &BrushProfile, distance: f32, radius: f32, x: f32, y: f32) -> f32 {
    let feather = (radius * (1.0 - profile.hardness)).max(1.0);
    let ramp = ((radius + 0.5 - distance) / feather).clamp(0.0, 1.0);
    let shaped = if profile.hardness >= 1.0 {
        ramp
    } else {
        ramp * ramp * (3.0 - 2.0 * ramp)
    };
    if profile.grain <= 0.0 || shaped <= 0.0 {
        return shaped;
    }
    shaped * (1.0 - profile.grain * (1.0 - paper_grain(x, y, profile.grain_scale)))
}

/// Value noise standing in for paper tooth, in `0..=1`, keyed on document position so the
/// same spot on the board always has the same grain.
pub fn paper_grain(x: f32, y: f32, scale: f32) -> f32 {
    let s = scale.max(0.25);
    let gx = x / s;
    let gy = y / s;
    let cell_x = gx.floor();
    let cell_y = gy.floor();
    let fx = gx - cell_x;
    let fy = gy - cell_y;
    let ease_x = fx * fx * (3.0 - 2.0 * fx);
    let ease_y = fy * fy * (3.0 - 2.0 * fy);
    let ix = cell_x as i32;
    let iy = cell_y as i32;
    let top_left = grain_cell(ix, iy);
    let top_right = grain_cell(ix + 1, iy);
    let bottom_left = grain_cell(ix, iy + 1);
    let bottom_right = grain_cell(ix + 1, iy + 1);
    let top = top_left + (top_right - top_left) * ease_x;
    let bottom = bottom_left + (bottom_right - bottom_left) * ease_x;
    top + (bottom - top) * ease_y
}

/// Integer hash, mirrored bit-for-bit by `grain_cell` in `board.wgsl`. Rust reinterprets a
/// negative `i32` as `u32` on cast, which is why the shader side has to `bitcast` rather than
/// convert — a plain conversion of a negative value is indeterminate in WGSL, and the board
/// has plenty of negative document coordinates.
fn grain_cell(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x27d4_eb2d) ^ (y as u32).wrapping_mul(0x1656_67b1);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 13;
    ((h & 0xffff) as f32) / 65535.0
}

/// Distance from `p` to the segment `a`–`b`, the shape a stroke actually is between two
/// recorded points. Shared with the shader, which computes the same capsule.
pub fn segment_distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let pa = (p.0 - a.0, p.1 - a.1);
    let ba = (b.0 - a.0, b.1 - a.1);
    let baba = ba.0 * ba.0 + ba.1 * ba.1;
    let h = if baba > 0.0 {
        ((pa.0 * ba.0 + pa.1 * ba.1) / baba).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let dx = pa.0 - ba.0 * h;
    let dy = pa.1 - ba.1 * h;
    (dx * dx + dy * dy).sqrt()
}
