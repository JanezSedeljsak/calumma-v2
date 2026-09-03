use calumma_core::document::*;
use calumma_core::*;

const DOC: u32 = 200;

fn doc_with_viewport() -> Document {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.resize_viewport(DOC as f32, DOC as f32, 1.0);
    doc.fit_to_view();
    doc
}

fn paint(doc: &mut Document, index: usize, rect: DocRect, rgba: [u8; 4]) {
    doc.layers[index]
        .tiles_mut()
        .unwrap()
        .paint_rect(rect, |_, _, _| Some(rgba));
}

fn layer_aabb(doc: &Document, index: usize) -> (f32, f32, f32, f32) {
    let layer = &doc.layers[index];
    let raw = layer.content_bounds().unwrap();
    layer.transform.unwrap_or_default().transformed_aabb(raw)
}

#[test]
fn align_left_lines_up_three_layers() {
    let mut doc = doc_with_viewport();
    doc.add_layer("Layer 2");
    doc.add_layer("Layer 3");
    paint(&mut doc, 1, DocRect::new(30, 40, 50, 60), [255, 0, 0, 255]);
    paint(&mut doc, 2, DocRect::new(80, 50, 100, 70), [0, 255, 0, 255]);
    paint(
        &mut doc,
        3,
        DocRect::new(120, 20, 140, 40),
        [0, 0, 255, 255],
    );
    assert!(doc.align_layers(&[1, 2, 3], AlignEdge::Left));
    let left = layer_aabb(&doc, 1).0;
    assert!((layer_aabb(&doc, 2).0 - left).abs() < 0.01);
    assert!((layer_aabb(&doc, 3).0 - left).abs() < 0.01);
}

#[test]
fn align_center_h_meets_in_the_middle() {
    let mut doc = doc_with_viewport();
    doc.add_layer("Layer 2");
    paint(&mut doc, 1, DocRect::new(20, 20, 40, 40), [255, 0, 0, 255]);
    paint(
        &mut doc,
        2,
        DocRect::new(100, 60, 120, 80),
        [0, 255, 0, 255],
    );
    assert!(doc.align_layers(&[1, 2], AlignEdge::CenterH));
    let c1 = (layer_aabb(&doc, 1).0 + layer_aabb(&doc, 1).2) * 0.5;
    let c2 = (layer_aabb(&doc, 2).0 + layer_aabb(&doc, 2).2) * 0.5;
    assert!((c1 - c2).abs() < 0.01);
}

#[test]
fn align_top_skips_paper_and_needs_two_layers() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(20, 40, 40, 60), [255, 0, 0, 255]);
    assert!(!doc.align_layers(&[0, 1], AlignEdge::Top));
    doc.add_layer("Layer 2");
    paint(
        &mut doc,
        2,
        DocRect::new(80, 80, 100, 100),
        [0, 255, 0, 255],
    );
    assert!(doc.align_layers(&[0, 1, 2], AlignEdge::Top));
    let top = layer_aabb(&doc, 1).1;
    assert!((layer_aabb(&doc, 2).1 - top).abs() < 0.01);
}

#[test]
fn distribute_horizontally_equalizes_gaps_and_pins_the_extremes() {
    let mut doc = doc_with_viewport();
    doc.add_layer("Layer 2");
    doc.add_layer("Layer 3");
    doc.add_layer("Layer 4");
    paint(&mut doc, 1, DocRect::new(10, 10, 30, 30), [255, 0, 0, 255]);
    paint(&mut doc, 2, DocRect::new(40, 10, 50, 30), [0, 255, 0, 255]);
    paint(&mut doc, 3, DocRect::new(60, 10, 90, 30), [0, 0, 255, 255]);
    paint(
        &mut doc,
        4,
        DocRect::new(150, 10, 170, 30),
        [255, 255, 0, 255],
    );
    let first = layer_aabb(&doc, 1);
    let last = layer_aabb(&doc, 4);
    assert!(doc.distribute_layers(&[1, 2, 3, 4], DistributeAxis::Horizontal));
    assert!(
        (layer_aabb(&doc, 1).0 - first.0).abs() < 0.01,
        "first pinned"
    );
    assert!((layer_aabb(&doc, 4).2 - last.2).abs() < 0.01, "last pinned");
    let gaps: Vec<f32> = [1, 2, 3]
        .iter()
        .map(|&i| layer_aabb(&doc, i + 1).0 - layer_aabb(&doc, i).2)
        .collect();
    assert!((gaps[0] - gaps[1]).abs() < 0.01);
    assert!((gaps[1] - gaps[2]).abs() < 0.01);
}

#[test]
fn distribute_vertically_is_idempotent() {
    let mut doc = doc_with_viewport();
    doc.add_layer("Layer 2");
    doc.add_layer("Layer 3");
    paint(&mut doc, 1, DocRect::new(10, 10, 30, 30), [255, 0, 0, 255]);
    paint(&mut doc, 2, DocRect::new(10, 40, 30, 60), [0, 255, 0, 255]);
    paint(
        &mut doc,
        3,
        DocRect::new(10, 140, 30, 180),
        [0, 0, 255, 255],
    );
    assert!(doc.distribute_layers(&[1, 2, 3], DistributeAxis::Vertical));
    let settled: Vec<(f32, f32, f32, f32)> = (1..=3).map(|i| layer_aabb(&doc, i)).collect();
    // A second pass has nothing left to equalize, so it reports no movement at all.
    assert!(!doc.distribute_layers(&[1, 2, 3], DistributeAxis::Vertical));
    for (i, before) in settled.iter().enumerate() {
        let after = layer_aabb(&doc, i + 1);
        assert!((after.1 - before.1).abs() < 0.01);
    }
}

#[test]
fn distribute_needs_three_movable_layers_and_skips_paper() {
    let mut doc = doc_with_viewport();
    doc.add_layer("Layer 2");
    paint(&mut doc, 1, DocRect::new(10, 10, 30, 30), [255, 0, 0, 255]);
    paint(&mut doc, 2, DocRect::new(10, 90, 30, 110), [0, 255, 0, 255]);
    // Paper plus two layers is still only two boxes to spread.
    assert!(!doc.distribute_layers(&[0, 1, 2], DistributeAxis::Vertical));
    doc.add_layer("Layer 3");
    // An empty layer has no content bounds, so it is not a box either.
    assert!(!doc.distribute_layers(&[0, 1, 2, 3], DistributeAxis::Vertical));
    paint(&mut doc, 3, DocRect::new(10, 40, 30, 60), [0, 0, 255, 255]);
    assert!(doc.distribute_layers(&[0, 1, 2, 3], DistributeAxis::Vertical));
}
