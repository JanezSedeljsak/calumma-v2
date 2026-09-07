use calumma_core::{Document, Layer, LayerTransform};

fn two_layer_doc() -> Document {
    let mut doc = Document::new("clip".into(), "Clip", 64, 64);
    doc.layers.clear();
    doc.layers.push(Layer::paper(64, 64));
    let mut base = Layer::new("Base", 64, 64);
    base.tiles_mut().unwrap().set_pixel(10, 10, [0, 0, 0, 255]);
    doc.layers.push(base);
    let mut top = Layer::new("Top", 64, 64);
    top.tiles_mut().unwrap().set_pixel(10, 10, [255, 0, 0, 255]);
    top.tiles_mut().unwrap().set_pixel(20, 20, [255, 0, 0, 255]);
    doc.layers.push(top);
    doc.active_layer = 2;
    doc
}

#[test]
fn create_clipping_mask_links_to_the_layer_below() {
    let mut doc = two_layer_doc();
    assert!(doc.create_clipping_mask(2));
    let base_id = doc.layers[1].id.clone();
    assert_eq!(doc.layers[2].clips_to.as_deref(), Some(base_id.as_str()));
}

#[test]
fn live_clip_hides_pixels_outside_the_base_ink() {
    let mut doc = two_layer_doc();
    let i20 = (20 * 64 + 20) * 4;
    let (_, _, before) = doc.composite_rgba();
    assert_eq!(before[i20 + 1], 0, "uncovered top pixel is red, not paper white");
    assert!(doc.create_clipping_mask(2));
    let (_, _, after) = doc.composite_rgba();
    assert_eq!(after[i20 + 1], 255, "clipped away — paper white shows through");
    let i10 = (10 * 64 + 10) * 4;
    assert!(after[i10 + 3] > 0, "inside the base silhouette");
}

#[test]
fn release_clipping_mask_restores_full_texture() {
    let mut doc = two_layer_doc();
    let i20 = (20 * 64 + 20) * 4;
    assert!(doc.create_clipping_mask(2));
    assert!(doc.release_clipping_mask(2));
    let (_, _, rgba) = doc.composite_rgba();
    assert_eq!(rgba[i20 + 1], 0, "top red returns outside the silhouette");
}

#[test]
fn reorder_breaks_a_clip_that_is_no_longer_adjacent() {
    let mut doc = two_layer_doc();
    assert!(doc.create_clipping_mask(2));
    assert!(doc.move_layer(2, 1));
    assert!(doc.layers[1].clips_to.is_none());
}

#[test]
fn undo_create_clipping_mask_restores_independence() {
    let mut doc = two_layer_doc();
    assert!(doc.create_clipping_mask(2));
    assert!(doc.undo());
    assert!(doc.layers[2].clips_to.is_none());
}

#[test]
fn is_layer_clip_base_matches_the_silhouette_row() {
    let mut doc = two_layer_doc();
    assert!(!doc.is_layer_clip_base(1));
    assert!(doc.create_clipping_mask(2));
    assert!(doc.is_layer_clip_base(1));
    assert!(!doc.is_layer_clip_base(2));
}

#[test]
fn clip_on_composite_follows_layer_transform() {
    let mut doc = Document::new("clip".into(), "Clip", 64, 64);
    doc.layers.clear();
    doc.layers.push(Layer::paper(64, 64));
    let mut base = Layer::new("Base", 64, 64);
    base.tiles_mut().unwrap().set_pixel(30, 30, [0, 0, 0, 255]);
    doc.layers.push(base);
    let mut top = Layer::new("Top", 64, 64);
    top.tiles_mut().unwrap().set_pixel(10, 10, [255, 0, 0, 255]);
    top.transform = Some(LayerTransform {
        offset_x: 20.0,
        offset_y: 20.0,
        scale_x: 1.0,
        scale_y: 1.0,
        rotation: 0.0,
    });
    doc.layers.push(top);
    assert!(doc.create_clipping_mask(2));
    let (_, _, rgba) = doc.composite_rgba();
    let i30 = (30 * 64 + 30) * 4;
    let i10 = (10 * 64 + 10) * 4;
    assert!(rgba[i30] > 200 && rgba[i30 + 1] < 10, "ink lands where the transform puts it");
    assert_eq!(rgba[i10 + 1], 255, "untouched spot stays paper white");
}

#[test]
fn transform_queues_clip_recalc_for_the_clipped_layer() {
    use calumma_core::tile::DirtyChannel;

    let mut doc = two_layer_doc();
    assert!(doc.create_clipping_mask(2));
    doc.layers[2]
        .tiles_mut()
        .unwrap()
        .clear_dirty(DirtyChannel::Render);
    doc.offset_layers(&[2], 5.0, 0.0);
    assert!(
        !doc.layers[2]
            .tiles()
            .unwrap()
            .dirty_tiles(DirtyChannel::Render)
            .is_empty(),
        "moving a clipped layer rebakes its clip mask"
    );
}

#[test]
fn moving_the_clip_base_rebakes_the_texture_layer() {
    use calumma_core::tile::DirtyChannel;

    let mut doc = two_layer_doc();
    assert!(doc.create_clipping_mask(2));
    doc.layers[2]
        .tiles_mut()
        .unwrap()
        .clear_dirty(DirtyChannel::Render);
    doc.offset_layers(&[1], 3.0, 0.0);
    assert!(
        !doc.layers[2]
            .tiles()
            .unwrap()
            .dirty_tiles(DirtyChannel::Render)
            .is_empty(),
        "moving the silhouette rebakes the clipped texture"
    );
}
