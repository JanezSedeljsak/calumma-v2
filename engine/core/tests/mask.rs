use calumma_core::{Document, Layer};

fn painted_doc() -> Document {
    let mut doc = Document::new("mask".into(), "Mask", 64, 64);
    doc.layers.clear();
    doc.layers.push(Layer::paper(64, 64));
    let mut paint = Layer::new("Paint", 64, 64);
    paint.tiles_mut().unwrap().set_pixel(10, 10, [255, 0, 0, 255]);
    paint.tiles_mut().unwrap().set_pixel(20, 20, [255, 0, 0, 255]);
    doc.layers.push(paint);
    doc.active_layer = 1;
    doc
}

fn pixel(rgba: &[u8], x: i32, y: i32) -> [u8; 4] {
    let i = ((y * 64 + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

#[test]
fn create_layer_mask_inserts_a_knockout_below() {
    let mut doc = painted_doc();
    assert!(doc.create_layer_mask(1));
    assert_eq!(doc.layers.len(), 3);
    assert_eq!(doc.layers[1].name, "Mask");
    assert!(doc.layers[2].clip_invert);
    assert_eq!(
        doc.layers[2].clips_to.as_deref(),
        Some(doc.layers[1].id.as_str())
    );
    assert!(doc.is_layer_masked(2));
    assert!(doc.is_layer_mask_base(1));
    assert_eq!(doc.active_layer, 2);
}

#[test]
fn an_empty_mask_leaves_the_layer_fully_visible() {
    let mut doc = painted_doc();
    assert!(doc.create_layer_mask(1));
    let (_, _, rgba) = doc.composite_rgba();
    assert!(pixel(&rgba, 10, 10)[0] > 200);
    assert!(pixel(&rgba, 20, 20)[0] > 200);
}

#[test]
fn opaque_mask_pixels_punch_holes_and_do_not_draw() {
    let mut doc = painted_doc();
    assert!(doc.create_layer_mask(1));
    doc.layers[1]
        .tiles_mut()
        .unwrap()
        .set_pixel(10, 10, [0, 0, 0, 255]);
    let (_, _, rgba) = doc.composite_rgba();
    assert_eq!(pixel(&rgba, 10, 10)[1], 255, "paper shows through the hole");
    assert!(pixel(&rgba, 20, 20)[0] > 200, "unmasked red stays");
}

#[test]
fn hiding_the_mask_disables_the_punch() {
    let mut doc = painted_doc();
    assert!(doc.create_layer_mask(1));
    doc.layers[1]
        .tiles_mut()
        .unwrap()
        .set_pixel(10, 10, [0, 0, 0, 255]);
    doc.set_layer_visible(1, false);
    let (_, _, rgba) = doc.composite_rgba();
    assert!(pixel(&rgba, 10, 10)[0] > 200);
}

#[test]
fn release_layer_mask_removes_the_extra_layer() {
    let mut doc = painted_doc();
    assert!(doc.create_layer_mask(1));
    assert!(doc.release_layer_mask(2));
    assert_eq!(doc.layers.len(), 2);
    assert!(doc.layers[1].clips_to.is_none());
    assert!(!doc.layers[1].clip_invert);
}

#[test]
fn apply_layer_mask_bakes_the_holes() {
    let mut doc = painted_doc();
    assert!(doc.create_layer_mask(1));
    doc.layers[1]
        .tiles_mut()
        .unwrap()
        .set_pixel(10, 10, [0, 0, 0, 255]);
    assert!(doc.apply_layer_mask(2));
    assert_eq!(doc.layers.len(), 2);
    let layer = &doc.layers[1];
    assert!(layer.clips_to.is_none());
    assert_eq!(layer.tiles().unwrap().get_pixel(10, 10)[3], 0);
    assert_eq!(layer.tiles().unwrap().get_pixel(20, 20)[3], 255);
}

#[test]
fn undo_create_layer_mask_restores_the_stack() {
    let mut doc = painted_doc();
    assert!(doc.create_layer_mask(1));
    assert!(doc.undo());
    assert_eq!(doc.layers.len(), 2);
    assert!(doc.layers[1].clips_to.is_none());
}

#[test]
fn a_masked_layer_cannot_also_clip() {
    let mut doc = painted_doc();
    assert!(doc.create_layer_mask(1));
    assert!(!doc.can_create_clipping_mask(2));
    assert!(!doc.can_create_layer_mask(2));
}

#[test]
fn paper_refuses_a_layer_mask() {
    let doc = painted_doc();
    assert!(!doc.can_create_layer_mask(0));
}

#[test]
fn can_release_layer_mask_needs_the_pair() {
    let mut doc = painted_doc();
    assert!(!doc.can_release_layer_mask(1));
    assert!(doc.create_layer_mask(1));
    assert!(doc.can_release_layer_mask(2));
}
