pub const TEXT_SIZE_MIN: f32 = 4.0;
pub const TEXT_SIZE_MAX: f32 = 512.0;
pub const TEXT_SIZE_DEFAULT: f32 = 48.0;

pub const TEXT_LINE_HEIGHT_MIN: f32 = 0.5;
pub const TEXT_LINE_HEIGHT_MAX: f32 = 4.0;
pub const TEXT_LINE_HEIGHT_DEFAULT: f32 = 1.25;

/// A click-placed text box wraps at this width unless the caller drags a box, and no box
/// narrower than this is honoured — below it every word lands on its own line.
pub const TEXT_WRAP_MIN_WIDTH: f32 = 16.0;
/// Slack added around the measured glyph box before rasterizing, so italic overhang and
/// antialiased edges are not clipped by the coverage pass.
pub const TEXT_WRAP_PADDING: f32 = 4.0;

/// Guard on the coverage bitmap a single run may produce. A 512pt run wrapped over a wide
/// board is still far below this; it exists so a malformed run cannot ask for gigabytes.
pub const TEXT_RASTER_MAX_SIDE: u32 = 16384;
