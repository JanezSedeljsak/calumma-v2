use calumma_core::document::*;
use calumma_core::shape::{Shape, Tool};
use calumma_core::tile::DocRect;
use calumma_core::transform::LayerTransform;
use calumma_core::vector::{VectorItem, VectorShape};

const SIDE: u32 = 32;

fn pixel(doc: &Document, index: usize, x: i32, y: i32) -> [u8; 4] {
    doc.layers[index].tiles().unwrap().get_pixel(x, y)
}

/// A base and a source above it, neither of them Paper, so nothing here passes or fails on the
/// Paper guard by accident. The base is a solid square of `base_alpha` in the middle; the
/// source is solid red over the whole canvas, so every test can look at one pixel inside the
/// base and one outside it.
fn stacked(base_alpha: u8, src_alpha: u8) -> (Document, usize) {
    let mut doc = Document::new("p".into(), "t", SIDE, SIDE);
    doc.add_layer("Base");
    let base = doc.active_layer;
    doc.layers[base]
        .tiles_mut()
        .unwrap()
        .fill_uniform(DocRect::new(8, 8, 23, 23), [0, 0, 255, base_alpha]);
    doc.add_layer("Top");
    let top = doc.active_layer;
    doc.layers[top].tiles_mut().unwrap().fill_uniform(
        DocRect::new(0, 0, SIDE as i32 - 1, SIDE as i32 - 1),
        [255, 0, 0, src_alpha],
    );
    (doc, top)
}

#[test]
fn merge_layer_down_bakes_pixels_and_removes_source() {
    let mut doc = Document::new("p".into(), "t", 32, 32);
    doc.add_layer("Top");
    let top = doc.active_layer;
    doc.layers[top]
        .tiles_mut()
        .unwrap()
        .set_pixel(3, 3, [10, 20, 30, 255]);
    let before = doc.layers.len();
    assert!(doc.merge_layer_down(top));
    assert_eq!(doc.layers.len(), before - 1);
    assert_eq!(pixel(&doc, doc.active_layer, 3, 3), [10, 20, 30, 255]);
}

#[test]
fn merge_layer_down_bakes_transform_into_destination_pixels() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    doc.add_layer("Top");
    let top = doc.active_layer;
    doc.layers[top]
        .tiles_mut()
        .unwrap()
        .set_pixel(10, 10, [10, 20, 30, 255]);
    doc.layers[top].transform = Some(LayerTransform {
        offset_x: 15.0,
        offset_y: 0.0,
        ..LayerTransform::default()
    });
    assert!(doc.merge_layer_down(top));
    assert_eq!(pixel(&doc, doc.active_layer, 25, 10), [10, 20, 30, 255]);
    assert_eq!(pixel(&doc, doc.active_layer, 10, 10), [0, 0, 0, 0]);
}

#[test]
fn merge_layer_into_paper_is_disallowed() {
    let mut doc = Document::new("p".into(), "t", 16, 16);
    let paint_index = doc.active_layer;
    assert!(!doc.merge_layer_down(paint_index));
}

/// The source's mask used to be dropped on the way down — `apply_layer_effects` carries opacity
/// and the LUT but never the mask, while `composite_rgba` and `layer_rgba` both apply it — so a
/// masked layer merged as if it had never been masked.
#[test]
fn merge_layer_down_honours_the_sources_mask() {
    let (mut doc, top) = stacked(255, 255);
    let mut mask = vec![255u8; (SIDE * SIDE) as usize];
    mask[(12 * SIDE + 12) as usize] = 0;
    doc.layers[top].set_mask(Some(mask));
    assert!(doc.merge_layer_down(top));
    let base = doc.active_layer;
    assert_eq!(pixel(&doc, base, 12, 12), [0, 0, 255, 255], "masked out");
    assert_eq!(pixel(&doc, base, 13, 13), [255, 0, 0, 255], "masked in");
}

#[test]
fn clipping_against_an_opaque_base_is_merging() {
    let mut merged = full_base(255);
    let mut clipped = full_base(255);
    let top = merged.active_layer;
    assert!(merged.merge_layer_down(top));
    assert!(clipped.clip_layer_down(top));
    assert_eq!(merged.layers.len(), clipped.layers.len());
    for y in 0..SIDE as i32 {
        for x in 0..SIDE as i32 {
            assert_eq!(
                pixel(&merged, merged.active_layer, x, y),
                pixel(&clipped, clipped.active_layer, x, y),
                "at {x},{y}"
            );
        }
    }
}

fn full_base(base_alpha: u8) -> Document {
    let (mut doc, _) = stacked(base_alpha, 255);
    let base = doc.active_layer - 1;
    doc.layers[base].tiles_mut().unwrap().fill_uniform(
        DocRect::new(0, 0, SIDE as i32 - 1, SIDE as i32 - 1),
        [0, 0, 255, base_alpha],
    );
    doc
}

#[test]
fn clipping_keeps_only_what_the_base_covers() {
    let (mut doc, top) = stacked(255, 255);
    let before = doc.layers.len();
    assert!(doc.clip_layer_down(top));
    assert_eq!(doc.layers.len(), before - 1, "the source is gone");
    let base = doc.active_layer;
    assert_eq!(pixel(&doc, base, 12, 12), [255, 0, 0, 255], "inside");
    assert_eq!(pixel(&doc, base, 2, 2), [0, 0, 0, 0], "outside");
}

/// Pins the alpha multiply exactly, without this test having to re-derive the blend that
/// follows it: clipping a source of alpha `s` against a base of alpha `b` must land on the same
/// pixel as merging a source whose alpha was already `s * b / 255`.
///
/// The bias in that division is the point of the second pair. `3 * 128 / 255` is 1.506, so a
/// truncating multiply would say 1 and this rounding one says 2 — and the round-trip through a
/// fully opaque base has to stay an exact no-op either way, which is the first assertion.
#[test]
fn the_base_alpha_multiplies_the_sources_and_rounds() {
    assert_eq!(clipped_pixel(255, 200), merged_pixel(255, 200));
    assert_eq!(clipped_pixel(128, 3), merged_pixel(128, 2));
    assert_ne!(clipped_pixel(128, 3), merged_pixel(128, 1));
}

fn clipped_pixel(base_alpha: u8, src_alpha: u8) -> [u8; 4] {
    let (mut doc, top) = stacked(base_alpha, src_alpha);
    assert!(doc.clip_layer_down(top));
    pixel(&doc, doc.active_layer, 12, 12)
}

fn merged_pixel(base_alpha: u8, src_alpha: u8) -> [u8; 4] {
    let (mut doc, top) = stacked(base_alpha, src_alpha);
    assert!(doc.merge_layer_down(top));
    pixel(&doc, doc.active_layer, 12, 12)
}

#[test]
fn clipping_against_a_transparent_base_leaves_it_alone_and_still_removes_the_source() {
    let (mut doc, top) = stacked(0, 255);
    let before = doc.layers.len();
    assert!(doc.clip_layer_down(top));
    assert_eq!(doc.layers.len(), before - 1);
    let base = doc.active_layer;
    for (x, y) in [(2, 2), (12, 12), (30, 30)] {
        assert_eq!(pixel(&doc, base, x, y)[3], 0, "at {x},{y}");
    }
}

#[test]
fn clipping_into_paper_is_refused() {
    let mut doc = Document::new("p".into(), "t", 16, 16);
    let paint_index = doc.active_layer;
    assert!(!doc.can_clip_layer_down(paint_index));
    assert!(!doc.clip_layer_down(paint_index));
    assert_eq!(doc.layers.len(), 2);
}

#[test]
fn clipping_the_bottom_layer_is_refused() {
    let (mut doc, _) = stacked(255, 255);
    assert!(!doc.can_clip_layer_down(0));
    assert!(!doc.clip_layer_down(0));
}

#[test]
fn clipping_onto_a_transformed_base_is_refused() {
    let (mut doc, top) = stacked(255, 255);
    let before = doc.layers.len();
    doc.layers[top - 1].transform = Some(LayerTransform {
        offset_x: 9.0,
        ..LayerTransform::default()
    });
    assert!(!doc.can_clip_layer_down(top));
    assert!(!doc.clip_layer_down(top));
    assert_eq!(doc.layers.len(), before, "nothing was merged");
}

#[test]
fn an_identity_transform_on_the_base_still_clips() {
    let (mut doc, top) = stacked(255, 255);
    doc.layers[top - 1].transform = Some(LayerTransform::default());
    assert!(doc.can_clip_layer_down(top));
    assert!(doc.clip_layer_down(top));
}

#[test]
fn clipping_onto_a_vector_base_is_refused() {
    let mut doc = Document::new("p".into(), "t", SIDE, SIDE);
    doc.add_vector_layer("Shape", filled_rect(8.0, 23.0, [0, 0, 255, 255]));
    doc.add_layer("Top");
    let top = doc.active_layer;
    assert!(!doc.can_clip_layer_down(top));
    assert!(!doc.clip_layer_down(top));
}

#[test]
fn a_vector_source_rasterizes_then_clips() {
    let mut doc = Document::new("p".into(), "t", SIDE, SIDE);
    doc.add_layer("Base");
    let base = doc.active_layer;
    doc.layers[base]
        .tiles_mut()
        .unwrap()
        .fill_uniform(DocRect::new(8, 8, 15, 15), [0, 0, 255, 255]);
    let before = doc.layers.len();
    let top = doc.add_vector_layer(
        "Shape",
        filled_rect(0.0, SIDE as f32 - 1.0, [255, 0, 0, 255]),
    );
    assert!(doc.clip_layer_down(top));
    let base = doc.active_layer;
    assert_eq!(doc.layers.len(), before);
    assert_eq!(pixel(&doc, base, 12, 12), [255, 0, 0, 255], "inside");
    assert_eq!(pixel(&doc, base, 24, 24), [0, 0, 0, 0], "outside");
}

fn filled_rect(from: f32, to: f32, color: [u8; 4]) -> VectorItem {
    VectorItem::Shape(VectorShape {
        shape: Shape {
            tool: Tool::Rect,
            start: (from, from),
            end: (to, to),
            half_width: 1.0,
            fill: true,
            stroke: false,
        },
        color,
        stroke_color: color,
    })
}

/// The source's own opacity is baked in, but the base's is *not* consumed by the clip — it
/// stays a property of the layer that survives, so it governs the merged result the way a
/// clipping group's base governs the group.
#[test]
fn the_bases_own_opacity_survives_the_clip() {
    let (mut doc, top) = stacked(255, 255);
    doc.set_layer_opacity(top - 1, 0.5);
    assert!(doc.clip_layer_down(top));
    let base = doc.active_layer;
    assert!((doc.layers[base].opacity - 0.5).abs() < 1e-6);
    assert_eq!(pixel(&doc, base, 12, 12), [255, 0, 0, 255]);
}
