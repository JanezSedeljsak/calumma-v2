use super::{overlay_rect_params, push_outlined_segment, rgba_unit, StrokeInstance};
use calumma_core::Document;

const TEXT_BOX_COLOR: [f32; 4] = [0.24, 0.78, 0.84, 0.45];
const TEXT_BOX_WIDTH_PX: f32 = 0.5;
const TEXT_CARET_WIDTH_PX: f32 = 1.0;
const TEXT_COMPOSITION_UNDERLINE_PX: f32 = 1.0;
/// The selection wash. Light enough that the glyphs read straight through it — the highlight
/// rides *over* the text (the overlay pass runs after the tile pass) rather than behind it, so
/// its alpha is the only thing keeping the words legible.
const TEXT_SELECTION_COLOR: [f32; 4] = [0.24, 0.78, 0.84, 0.28];
/// Seconds for one on-off caret cycle. The blink runs off the renderer clock rather than a
/// shell timer, so nothing outside the engine has to know a caret exists.
const TEXT_CARET_BLINK_SECONDS: f32 = 1.06;

/// Which half of the blink the caret is in at `elapsed`.
///
/// Split out of `text_overlay_instances` because the renderer's frame loop reads it too: a caret
/// is the only thing that asks for a frame with nothing about the document changing.
/// `Renderer::render` skips the frames where this answer has not moved since the
/// last one it drew, so the two have to be the *same* function — a gate that disagreed with the
/// drawing would drop the frame that was supposed to show the flip.
pub fn text_caret_visible(elapsed: f32) -> bool {
    (elapsed / TEXT_CARET_BLINK_SECONDS).fract() < 0.5
}

/// The board furniture for a live text session: a hairline box around the run's layout, an
/// underline under an input method's composition, and a caret that blinks. All of them are
/// stroke segments, the same primitive the transform overlay and the lasso already draw with —
/// no new pipeline, and nothing drawn by the shell.
pub fn text_overlay_instances(doc: &Document, elapsed: f32) -> Vec<StrokeInstance> {
    let Some((x0, y0, x1, y1)) = doc.text_box() else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(8);
    // First, so the box hairline and the caret paint over it: instances draw in order.
    let zoom = doc.camera.zoom.max(f32::MIN_POSITIVE);
    for row in doc.text_selection_rows() {
        let mid_y = row.y + row.height * 0.5;
        out.push(StrokeInstance {
            segment: [row.x, mid_y, row.x + row.width, mid_y],
            color: TEXT_SELECTION_COLOR,
            brush: overlay_rect_params(row.height * 0.5 * zoom),
        });
    }
    let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    for i in 0..4 {
        push_outlined_segment(
            &mut out,
            corners[i],
            corners[(i + 1) % 4],
            TEXT_BOX_COLOR,
            TEXT_BOX_WIDTH_PX,
        );
    }
    let ink = rgba_unit(doc.text_caret_color());
    for row in doc.text_composition_rows() {
        let base = row.y + row.height;
        push_outlined_segment(
            &mut out,
            (row.x, base),
            (row.x + row.width, base),
            ink,
            TEXT_COMPOSITION_UNDERLINE_PX,
        );
    }
    if let (true, Some((a, b))) = (text_caret_visible(elapsed), doc.text_caret_segment()) {
        push_outlined_segment(&mut out, a, b, ink, TEXT_CARET_WIDTH_PX);
    }
    out
}
