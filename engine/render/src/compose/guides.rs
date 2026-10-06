use bytemuck::{Pod, Zeroable};
use calumma_core::{Document, GuideAxis};

/// One guide rule, as the guide pass wants it: a document-space segment plus a color. Width
/// is not per-instance because every guide is the same hairline — `board.wgsl`'s
/// `GUIDE_HALF_WIDTH_PX` owns it, in screen pixels.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct GuideInstance {
    pub segment: [f32; 4],
    pub color: [f32; 4],
}

/// How solid a guide is drawn. The *color* is the guide's own (`Guide::color`, picked in the
/// guides card and defaulting to `calumma_core::default_guide_color`); alpha stays here because
/// it is not a choice — it is how the board says which rule is the one under the pointer.
const GUIDE_ALPHA: f32 = 0.85;
const GUIDE_DRAGGED_ALPHA: f32 = 1.0;

/// Guides span the *view*, not the paper — a rule you can only see where there is paper cannot
/// be lined up against a layer hanging off it, and never meets the ruler it was pulled from.
/// This is the one thing the board draws over the desk, which is why `Renderer::render` lifts
/// the paper scissor around the guide pass alone.
pub fn guide_instances(doc: &Document) -> Vec<GuideInstance> {
    let dragged = doc.dragged_guide();
    // Edge to edge of the *view*, not of the paper. A guide is an alignment reference for the
    // whole board, and one that stopped at the paper could not be lined up against anything
    // hanging off it — nor did it meet the ruler it was pulled from, which is where the eye
    // goes to read its position.
    let (min_x, min_y, max_x, max_y) = doc.camera.viewport_doc_bounds();
    doc.guides()
        .iter()
        .enumerate()
        .map(|(index, guide)| GuideInstance {
            segment: match guide.axis {
                GuideAxis::Horizontal => [min_x, guide.position, max_x, guide.position],
                GuideAxis::Vertical => [guide.position, min_y, guide.position, max_y],
            },
            color: guide_color(
                guide.color,
                if dragged == Some(index) {
                    GUIDE_DRAGGED_ALPHA
                } else {
                    GUIDE_ALPHA
                },
            ),
        })
        .collect()
}

fn guide_color(rgb: [u8; 3], alpha: f32) -> [f32; 4] {
    [
        rgb[0] as f32 / 255.0,
        rgb[1] as f32 / 255.0,
        rgb[2] as f32 / 255.0,
        alpha,
    ]
}
