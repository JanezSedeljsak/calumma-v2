//! The composition guides Crop draws over its rectangle while dragging — rule of thirds, a
//! grid, diagonals, the golden section. Display only: nothing here feeds back into the commit.

use crate::document::Document;
use num_enum::{IntoPrimitive, TryFromPrimitive};

/// Which composition guide the crop overlay draws over the rect while dragging. Pure display —
/// none of these feed back into the commit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u32)]
pub enum CropOverlayStyle {
    Off = 0,
    #[default]
    RuleOfThirds = 1,
    Grid = 2,
    Diagonal = 3,
    GoldenRatio = 4,
}

impl CropOverlayStyle {
    pub fn from_u32(value: u32) -> Option<Self> {
        Self::try_from(value).ok()
    }
}

impl Document {
    /// The guide segments the overlay draws over the current rect for `crop_overlay_style`.
    pub fn crop_overlay_lines(&self) -> Vec<((f32, f32), (f32, f32))> {
        let Some(rect) = self.crop_rect else {
            return Vec::new();
        };
        overlay_lines_for(rect, self.crop_overlay_style)
    }
}

fn overlay_lines_for(
    rect: (f32, f32, f32, f32),
    style: CropOverlayStyle,
) -> Vec<((f32, f32), (f32, f32))> {
    let (x0, y0, x1, y1) = rect;
    let (w, h) = (x1 - x0, y1 - y0);
    let fractions: Vec<f32> = match style {
        CropOverlayStyle::Off => return Vec::new(),
        CropOverlayStyle::Diagonal => {
            return vec![((x0, y0), (x1, y1)), ((x1, y0), (x0, y1))];
        }
        CropOverlayStyle::RuleOfThirds => vec![1.0 / 3.0, 2.0 / 3.0],
        CropOverlayStyle::Grid => vec![0.25, 0.5, 0.75],
        CropOverlayStyle::GoldenRatio => {
            // 1/φ, the golden section — the same split on both axes, mirrored, the way
            // Photoshop's own Golden Ratio overlay divides the rect.
            let inv_phi = 2.0 / (1.0 + 5f32.sqrt());
            vec![1.0 - inv_phi, inv_phi]
        }
    };
    fractions
        .into_iter()
        .flat_map(|f| {
            [
                ((x0 + w * f, y0), (x0 + w * f, y1)),
                ((x0, y0 + h * f), (x1, y0 + h * f)),
            ]
        })
        .collect()
}
