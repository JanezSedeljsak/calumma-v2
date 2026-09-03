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
    layer
        .transform
        .unwrap_or_default()
        .transformed_aabb(raw)
}

#[test]
fn align_left_lines_up_three_layers() {
    let mut doc = doc_with_viewport();
    doc.add_layer("Layer 2");
    doc.add_layer("Layer 3");
    paint(&mut doc, 1, DocRect::new(30, 40, 50, 60), [255, 0, 0, 255]);
    paint(&mut doc, 2, DocRect::new(80, 50, 100, 70), [0, 255, 0, 255]);
    paint(&mut doc, 3, DocRect::new(120, 20, 140, 40), [0, 0, 255, 255]);
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
    paint(&mut doc, 2, DocRect::new(100, 60, 120, 80), [0, 255, 0, 255]);
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
    paint(&mut doc, 2, DocRect::new(80, 80, 100, 100), [0, 255, 0, 255]);
    assert!(doc.align_layers(&[0, 1, 2], AlignEdge::Top));
    let top = layer_aabb(&doc, 1).1;
    assert!((layer_aabb(&doc, 2).1 - top).abs() < 0.01);
}
