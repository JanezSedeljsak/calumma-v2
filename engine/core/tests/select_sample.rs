//! One answer to "what colour is this layer here", across every kind of layer.
//!
//! `LayerSelectSample` is what let the select tools stop caring whether they are pointed at
//! tiles or at parameters. The tests that matter are the ones where the naive answer differs
//! from the right one: a layer carrying a transform, a vector layer with no pixels at all, and
//! the copy that has to agree with the selection it was made from.

use calumma_core::select_sample::{simplify_lasso_points, LayerSelectSample};
use calumma_core::transform::LayerTransform;
use calumma_core::*;

const DOC: u32 = 128;
const RED: [u8; 4] = [200, 30, 30, 255];
const BLUE: [u8; 4] = [30, 30, 200, 255];

fn board() -> Document {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.resize_viewport(DOC as f32, DOC as f32, 1.0);
    doc.fit_to_view();
    doc
}

fn fill(doc: &mut Document, rect: DocRect, color: [u8; 4]) {
    let layer = doc.active_layer;
    doc.layers[layer]
        .tiles_mut()
        .expect("a raster layer")
        .fill_uniform(rect, color);
}

fn blue_rect_item() -> vector::VectorItem {
    vector::VectorItem::Shape(vector::VectorShape {
        shape: Shape {
            tool: Tool::Rect,
            start: (40.0, 40.0),
            end: (90.0, 90.0),
            half_width: 1.0,
            fill: true,
            stroke: false,
        },
        color: BLUE,
        stroke_color: BLUE,
    })
}

fn sample_of(doc: &Document) -> LayerSelectSample<'_> {
    LayerSelectSample::new(&doc.layers[doc.active_layer], doc.bounds()).expect("a sample")
}

#[test]
fn a_raster_layer_reads_straight_out_of_its_tiles() {
    let mut doc = board();
    fill(&mut doc, DocRect::new(10, 10, 30, 30), RED);
    let sample = sample_of(&doc);
    assert_eq!(sample.pixel(20, 20), RED);
    assert!(sample.opaque_enough(20, 20));
    assert!(!sample.opaque_enough(60, 60), "outside the ink is empty");
}

/// The scope is the painted box, not the canvas, so a walk over a small sketch on a large board
/// costs the sketch.
#[test]
fn the_scope_hugs_the_ink_rather_than_the_canvas() {
    let mut doc = board();
    fill(&mut doc, DocRect::new(10, 10, 30, 30), RED);
    let scope = sample_of(&doc).scope;
    // `content_bounds` reports the far edge exclusively, so the integer box it becomes reaches
    // one past the last painted pixel. That is the same box the transform frame draws.
    assert_eq!((scope.min_x, scope.min_y), (10, 10));
    assert_eq!((scope.max_x, scope.max_y), (31, 31));
}

#[test]
fn a_layer_with_nothing_painted_has_no_sample_at_all() {
    let doc = board();
    assert!(
        LayerSelectSample::new(&doc.layers[doc.active_layer], doc.bounds()).is_none(),
        "a fresh layer has no ink to sample"
    );
    assert!(calumma_core::select_sample::painted_scope(
        &doc.layers[doc.active_layer],
        doc.bounds()
    )
    .is_none());
}

/// A transform moves the layer in document space while its tiles stay in their own. Asking the
/// grid directly would read the pixel that *used* to be at that coordinate.
#[test]
fn a_transformed_layer_is_read_through_its_transform() {
    let mut doc = board();
    fill(&mut doc, DocRect::new(10, 10, 30, 30), RED);
    let layer = doc.active_layer;
    doc.layers[layer].transform = Some(LayerTransform {
        offset_x: 40.0,
        offset_y: 0.0,
        ..LayerTransform::default()
    });

    let sample = sample_of(&doc);
    assert_eq!(
        sample.pixel(60, 20),
        RED,
        "the ink moved with the transform"
    );
    assert_eq!(
        sample.pixel(20, 20),
        [0, 0, 0, 0],
        "and left nothing behind where it was"
    );
    let scope = sample.scope;
    assert_eq!(
        (scope.min_x, scope.max_x),
        (50, 71),
        "the box moved with it"
    );
}

#[test]
fn a_vector_layer_is_sampled_from_its_parameters() {
    let mut doc = board();
    doc.add_vector_layer("V", blue_rect_item());
    let sample = sample_of(&doc);
    assert_eq!(sample.pixel(60, 60), BLUE, "inside the shape");
    assert_eq!(sample.pixel(20, 20)[3], 0, "outside it");
    assert!(
        doc.layers[doc.active_layer].content.item().is_some(),
        "reading a vector layer must never bake it"
    );
}

#[test]
fn a_text_layer_is_sampled_from_its_tile_cache() {
    let mut doc = board();
    doc.tool = Tool::Text;
    doc.text_style.size = 64.0;
    let (sx, sy) = doc.camera.to_screen(20.0, 60.0);
    doc.pointer_down(sx, sy);
    doc.pointer_up(sx, sy);
    doc.text_insert("H");
    doc.commit_text();

    let sample = sample_of(&doc);
    let bounds = sample.scope;
    let inked = (bounds.min_y..=bounds.max_y)
        .flat_map(|y| (bounds.min_x..=bounds.max_x).map(move |x| (x, y)))
        .any(|(x, y)| sample.opaque_enough(x, y));
    assert!(inked, "the glyph's pixels are readable");
    assert!(
        doc.layers[doc.active_layer].run().is_some(),
        "the run stays editable"
    );
}

/// Copy has to read what the selection was built from. Reading the grid in document space
/// answered a transformed layer wrong and a vector layer not at all.
#[test]
fn copying_a_selection_on_a_vector_layer_yields_its_pixels() {
    let mut doc = board();
    doc.add_vector_layer("V", blue_rect_item());
    doc.tool = Tool::MagicWand;
    let (sx, sy) = doc.camera.to_screen(60.0, 60.0);
    doc.pointer_down(sx, sy);
    doc.pointer_up(sx, sy);

    let (w, h, buf) = doc.selection_rgba().expect("copied pixels");
    assert!(w > 0 && h > 0);
    assert!(
        buf.chunks_exact(4).any(|px| px[3] > 0),
        "a vector selection copied nothing"
    );
}

#[test]
fn copying_a_selection_on_a_transformed_layer_follows_the_transform() {
    let mut doc = board();
    fill(&mut doc, DocRect::new(10, 10, 30, 30), RED);
    let layer = doc.active_layer;
    doc.layers[layer].transform = Some(LayerTransform {
        offset_x: 40.0,
        offset_y: 0.0,
        ..LayerTransform::default()
    });
    doc.tool = Tool::MagicWand;
    let (sx, sy) = doc.camera.to_screen(60.0, 20.0);
    doc.pointer_down(sx, sy);
    doc.pointer_up(sx, sy);

    let (_, _, buf) = doc.selection_rgba().expect("copied pixels");
    assert!(
        buf.chunks_exact(4).any(|px| px[3] > 0),
        "the copy read the layer's own space instead of the document's"
    );
}

#[test]
fn a_repeated_point_is_dropped_and_the_corners_are_kept() {
    let square = vec![
        (0.0, 0.0),
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 10.0),
        (0.0, 10.0),
    ];
    let out = simplify_lasso_points(square);
    assert_eq!(
        out,
        vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]
    );
}

#[test]
fn points_on_a_straight_run_collapse_to_its_ends() {
    let line = vec![
        (0.0, 0.0),
        (2.0, 0.0),
        (4.0, 0.0),
        (6.0, 0.0),
        (6.0, 8.0),
        (0.0, 8.0),
    ];
    let out = simplify_lasso_points(line);
    assert_eq!(out, vec![(0.0, 0.0), (6.0, 0.0), (6.0, 8.0), (0.0, 8.0)]);
}

#[test]
fn a_bend_survives_simplification() {
    let bend = vec![(0.0, 0.0), (5.0, 1.0), (10.0, 0.0)];
    assert_eq!(simplify_lasso_points(bend.clone()), bend);
}

#[test]
fn a_non_finite_point_is_thrown_away_rather_than_poisoning_the_polygon() {
    let out = simplify_lasso_points(vec![
        (0.0, 0.0),
        (f32::NAN, 5.0),
        (10.0, 0.0),
        (5.0, f32::INFINITY),
        (10.0, 10.0),
    ]);
    assert!(out.iter().all(|p| p.0.is_finite() && p.1.is_finite()));
    assert_eq!(out.len(), 3);
}

#[test]
fn simplifying_nothing_is_nothing() {
    assert!(simplify_lasso_points(Vec::new()).is_empty());
    assert_eq!(simplify_lasso_points(vec![(1.0, 1.0)]).len(), 1);
}
