mod decode;
mod encode;
mod reader;

pub use decode::{decode, decode_flat, DecodedLayer, DecodedPsd};
pub use encode::encode;

use calumma_core::BlendMode;

const SIGNATURE: &[u8; 4] = b"8BPS";
const BLEND_SIGNATURE: &[u8; 4] = b"8BIM";
const CHANNEL_COUNT: u16 = 4;

/// Photoshop's four-character key for each mode, both ways.
const BLEND_KEYS: [(BlendMode, &[u8; 4]); 26] = [
    (BlendMode::Normal, b"norm"),
    (BlendMode::Multiply, b"mul "),
    (BlendMode::Screen, b"scrn"),
    (BlendMode::Darken, b"dark"),
    (BlendMode::ColorBurn, b"idiv"),
    (BlendMode::LinearBurn, b"lbrn"),
    (BlendMode::DarkerColor, b"dkCl"),
    (BlendMode::Lighten, b"lite"),
    (BlendMode::ColorDodge, b"div "),
    (BlendMode::LinearDodge, b"lddg"),
    (BlendMode::LighterColor, b"lgCl"),
    (BlendMode::Overlay, b"over"),
    (BlendMode::SoftLight, b"sLit"),
    (BlendMode::HardLight, b"hLit"),
    (BlendMode::VividLight, b"vLit"),
    (BlendMode::LinearLight, b"lLit"),
    (BlendMode::PinLight, b"pLit"),
    (BlendMode::HardMix, b"hMix"),
    (BlendMode::Difference, b"diff"),
    (BlendMode::Exclusion, b"smud"),
    (BlendMode::Subtract, b"fsub"),
    (BlendMode::Divide, b"fdiv"),
    (BlendMode::Hue, b"hue "),
    (BlendMode::Saturation, b"sat "),
    (BlendMode::Color, b"colr"),
    (BlendMode::Luminosity, b"lum "),
];

fn blend_key(mode: BlendMode) -> &'static [u8; 4] {
    BLEND_KEYS
        .iter()
        .find(|(m, _)| *m == mode)
        .map_or(b"norm", |(_, key)| *key)
}

/// A key the engine has no mode for (Dissolve, `pass` on a group) lands as Normal rather than
/// failing the import: the right pixels in the right place are worth more than one knob.
fn blend_mode_from_key(key: &[u8]) -> BlendMode {
    BLEND_KEYS
        .iter()
        .find(|(_, k)| k.as_slice() == key)
        .map_or(BlendMode::Normal, |(mode, _)| *mode)
}
