use crate::layer::{Layer, LayerContent};
use calumma_text::{measure, rasterize, TextRun};

/// Rebuilds a text layer's tile cache from its run.
///
/// The grid is cleared first rather than diffed. A text layer holds one run and nothing
/// else, so its tiles are only ever the glyphs — dropping them all and re-blitting is both
/// correct for shrinking text (no stale ink left behind from the longer string) and cheap,
/// since a run covers a handful of tiles and `TileGrid::clear` already marks exactly those
/// dirty for the GPU and the store.
pub fn resync(layer: &mut Layer) -> bool {
    let LayerContent::Text { run, tiles } = &mut layer.content else {
        return false;
    };
    tiles.clear();
    let Some(raster) = rasterize(run) else {
        return true;
    };
    tiles.blit_rgba_at(
        &raster.rgba,
        raster.width,
        raster.height,
        raster.origin_x,
        raster.origin_y,
    );
    true
}

/// The run's layout box in document space: where the caret starts, how wide the wrap is (or
/// how wide the longest line came out), and how tall the block is. This is the rectangle the
/// board outlines while editing and the one a click hit-tests against.
pub fn run_box(run: &TextRun) -> (f32, f32, f32, f32) {
    let (measured_w, measured_h) = measure(run);
    let width = run.wrap_width.unwrap_or(measured_w).max(1.0);
    let height = measured_h.max(run.line_spacing());
    (
        run.origin.0,
        run.origin.1,
        run.origin.0 + width,
        run.origin.1 + height,
    )
}

/// Slack around the layout box when deciding whether a click re-enters an existing text
/// layer. Clicking just past the last glyph should still land in the text, the way it does
/// in every text editor.
const HIT_PADDING: f32 = 6.0;

pub fn hits_run(run: &TextRun, x: f32, y: f32) -> bool {
    let (x0, y0, x1, y1) = run_box(run);
    x >= x0 - HIT_PADDING && x <= x1 + HIT_PADDING && y >= y0 - HIT_PADDING && y <= y1 + HIT_PADDING
}
