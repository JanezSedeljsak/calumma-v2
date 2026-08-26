use calumma_core::*;

const DOC: u32 = 200;

#[test]
fn content_bounds_shrinks_to_the_mask() {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.layers[1]
        .tiles_mut()
        .unwrap()
        .paint_rect(DocRect::new(10, 10, 90, 90), |_, _, _| Some([255, 0, 0, 255]));
    let before = doc.layers[1].content_bounds().expect("full rect");
    assert_eq!(before, (10.0, 10.0, 91.0, 91.0));

    let mut mask = vec![255u8; (DOC as usize) * (DOC as usize)];
    for y in 0..DOC {
        for x in 0..DOC {
            let inside_subject = (40..60).contains(&x) && (40..60).contains(&y);
            if !inside_subject {
                mask[(y * DOC + x) as usize] = 0;
            }
        }
    }
    doc.layers[1].set_mask(Some(mask));
    let after = doc.layers[1].content_bounds().expect("visible rect");
    assert_eq!(after, (40.0, 40.0, 60.0, 60.0));
}

#[test]
fn remove_background_bakes_and_keeps_document_position() {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.resize_viewport(DOC as f32, DOC as f32, 1.0);
    doc.fit_to_view();
    doc.layers[1]
        .tiles_mut()
        .unwrap()
        .paint_rect(DocRect::new(10, 10, 90, 90), |_, _, _| Some([255, 0, 0, 255]));
    let before = doc.layer_bounds(1).expect("full rect");
    assert_eq!(before, (10.0, 10.0, 91.0, 91.0));

    let mut mask = vec![255u8; (DOC as usize) * (DOC as usize)];
    for y in 0..DOC {
        for x in 0..DOC {
            let inside_subject = (40..60).contains(&x) && (40..60).contains(&y);
            if !inside_subject {
                mask[(y * DOC + x) as usize] = 0;
            }
        }
    }
    assert!(doc.apply_remove_background_mask(1, mask));
    assert!(doc.layers[1].mask().is_none());
    let after = doc.layer_bounds(1).expect("visible rect");
    assert_eq!(after, (40.0, 40.0, 60.0, 60.0));
    assert_eq!(doc.layers[1].content_bounds().unwrap(), (40.0, 40.0, 60.0, 60.0));

    doc.set_tool(Tool::Move);
    assert!(doc.begin_move_at(50.0, 50.0));
}

#[test]
fn remove_background_undo_restores_pixels_and_transform() {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.layers[1]
        .tiles_mut()
        .unwrap()
        .paint_rect(DocRect::new(10, 10, 90, 90), |_, _, _| Some([255, 0, 0, 255]));
    doc.layers[1].transform = Some(LayerTransform {
        offset_x: 12.0,
        offset_y: -8.0,
        ..LayerTransform::default()
    });
    let before_bounds = doc.layer_bounds(1).unwrap();
    let subject_doc = LayerTransform {
        offset_x: 12.0,
        offset_y: -8.0,
        ..LayerTransform::default()
    }
    .transformed_aabb((40.0, 40.0, 60.0, 60.0));
    assert_eq!(
        before_bounds,
        LayerTransform {
            offset_x: 12.0,
            offset_y: -8.0,
            ..LayerTransform::default()
        }
        .transformed_aabb((10.0, 10.0, 91.0, 91.0))
    );
    let mut mask = vec![255u8; (DOC as usize) * (DOC as usize)];
    for y in 0..DOC {
        for x in 0..DOC {
            if !(52..72).contains(&x) || !(32..52).contains(&y) {
                mask[(y * DOC + x) as usize] = 0;
            }
        }
    }
    assert!(doc.apply_remove_background_mask(1, mask));
    assert_eq!(doc.layer_bounds(1).unwrap(), subject_doc);
    assert_eq!(
        doc.layers[1].transform,
        Some(LayerTransform {
            offset_x: 12.0,
            offset_y: -8.0,
            ..LayerTransform::default()
        })
    );
    assert!(doc.undo());
    assert_eq!(doc.layer_bounds(1).unwrap(), before_bounds);
    assert!(doc.layers[1].mask().is_none());
}

#[test]
fn move_tool_picks_through_a_mask_hole_with_a_10x10_window() {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.resize_viewport(DOC as f32, DOC as f32, 1.0);
    doc.fit_to_view();
    doc.layers[1]
        .tiles_mut()
        .unwrap()
        .paint_rect(DocRect::new(10, 10, 90, 90), |_, _, _| Some([255, 0, 0, 255]));
    let mut mask = vec![255u8; (DOC as usize) * (DOC as usize)];
    for y in 25..=35u32 {
        for x in 25..=35u32 {
            mask[(y * DOC + x) as usize] = 0;
        }
    }
    doc.layers[1].set_mask(Some(mask));
    doc.set_tool(Tool::Move);

    assert!(
        !doc.begin_move_at(30.0, 30.0),
        "a 10x10 centred on the hole is all transparent"
    );
    assert!(
        doc.begin_move_at(70.0, 70.0),
        "visible pixels still grab the layer"
    );
}

#[test]
fn transform_keeps_the_layer_on_transparent_pixels_inside_the_box() {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.resize_viewport(DOC as f32, DOC as f32, 1.0);
    doc.fit_to_view();
    doc.layers[1]
        .tiles_mut()
        .unwrap()
        .paint_rect(DocRect::new(10, 10, 90, 90), |_, _, _| Some([255, 0, 0, 255]));
    let mut mask = vec![255u8; (DOC as usize) * (DOC as usize)];
    for y in 28..=32u32 {
        for x in 28..=32u32 {
            mask[(y * DOC + x) as usize] = 0;
        }
    }
    doc.layers[1].set_mask(Some(mask));
    doc.add_layer("Below");
    let below = doc.active_layer;
    doc.layers[below]
        .tiles_mut()
        .unwrap()
        .paint_rect(DocRect::new(10, 10, 90, 90), |_, _, _| Some([0, 255, 0, 255]));
    doc.set_active_layer(1);
    assert!(doc.enter_transform());

    let (sx, sy) = doc.camera.to_screen(30.0, 30.0);
    doc.pointer_down(sx, sy);
    assert_eq!(doc.active_layer, 1, "inside the frame keeps the masked layer");
    assert!(doc.transform_active);
    let (mx, my) = doc.camera.to_screen(40.0, 40.0);
    doc.pointer_move(mx, my);
    assert!(
        doc.layers[1].transform.is_some(),
        "the drag started even on a masked-out pixel"
    );
}
