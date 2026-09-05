use calumma_core::*;

const DOC: u32 = 200;

#[test]
fn content_bounds_shrinks_to_the_mask() {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.layers[1]
        .tiles_mut()
        .unwrap()
        .paint_rect(DocRect::new(10, 10, 90, 90), |_, _, _| {
            Some([255, 0, 0, 255])
        });
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
        .paint_rect(DocRect::new(10, 10, 90, 90), |_, _, _| {
            Some([255, 0, 0, 255])
        });
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
    assert_eq!(
        doc.layers[1].content_bounds().unwrap(),
        (40.0, 40.0, 60.0, 60.0)
    );

    doc.set_tool(Tool::Move);
    assert!(doc.begin_move_at(50.0, 50.0));
}

#[test]
fn remove_background_undo_restores_pixels_and_transform() {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.layers[1]
        .tiles_mut()
        .unwrap()
        .paint_rect(DocRect::new(10, 10, 90, 90), |_, _, _| {
            Some([255, 0, 0, 255])
        });
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
fn remove_background_rejects_bad_arguments_rather_than_panicking() {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.layers[1]
        .tiles_mut()
        .unwrap()
        .paint_rect(DocRect::new(10, 10, 90, 90), |_, _, _| {
            Some([255, 0, 0, 255])
        });
    let good_mask = vec![255u8; (DOC as usize) * (DOC as usize)];

    assert!(
        !doc.apply_remove_background_mask(1, vec![255u8; 4]),
        "a mask the wrong size for the document must be refused"
    );
    assert!(
        !doc.apply_remove_background_mask(99, good_mask.clone()),
        "an out-of-range layer index must be refused"
    );
    assert!(
        !doc.apply_remove_background_mask(0, good_mask),
        "Paper has no subject to cut out"
    );
}

#[test]
fn remove_background_refuses_a_layer_with_no_pixels() {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.add_vector_layer(
        "Shape",
        VectorItem::Shape(VectorShape {
            shape: Shape {
                tool: Tool::Rect,
                start: (10.0, 10.0),
                end: (50.0, 50.0),
                half_width: 1.0,
                fill: true,
                stroke: false,
            },
            color: [255, 0, 0, 255],
            stroke_color: [255, 0, 0, 255],
        }),
    );
    let index = doc.active_layer;
    let mask = vec![255u8; (DOC as usize) * (DOC as usize)];
    assert!(
        !doc.apply_remove_background_mask(index, mask),
        "a vector layer has no tile grid to bake a mask into"
    );
}

/// Every pixel the layer painted maps to a mask value of 0 — the subject the tool found is
/// entirely gone, which is different from the layer never having painted anything at all
/// (that case returns `true` and simply leaves the layer untouched).
#[test]
fn remove_background_with_an_entirely_transparent_mask_is_refused() {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.layers[1]
        .tiles_mut()
        .unwrap()
        .paint_rect(DocRect::new(10, 10, 90, 90), |_, _, _| {
            Some([255, 0, 0, 255])
        });
    let mask = vec![0u8; (DOC as usize) * (DOC as usize)];
    assert!(!doc.apply_remove_background_mask(1, mask));
    assert!(
        doc.layers[1].content_bounds().is_some(),
        "the layer is untouched"
    );
}

/// `remove_background_undo_restores_pixels_and_transform` covers a pure-offset transform,
/// where the new offset is just the old one shifted by how far the crop moved.
/// `preserve_doc_bounds`'s other branch — rotation or scale present — takes a different path
/// (project the whole transformed AABB and solve for the offset), exercised here; the exact
/// arithmetic that branch has to get right is pinned precisely by an in-crate unit test in
/// `mask_bake.rs`, which has access to `visible_doc_bounds_for_mask` to compute the expected
/// value the same way the code under test does rather than by hand.
#[test]
fn remove_background_shrinks_a_rotated_and_scaled_layer_without_losing_it() {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.layers[1]
        .tiles_mut()
        .unwrap()
        .paint_rect(DocRect::new(10, 10, 90, 90), |_, _, _| {
            Some([255, 0, 0, 255])
        });
    doc.layers[1].transform = Some(LayerTransform {
        offset_x: 20.0,
        offset_y: 5.0,
        scale_x: 1.5,
        scale_y: 1.5,
        rotation: 0.3,
    });
    let before_bounds = doc.layer_bounds(1).expect("full rect, transformed");

    let mut mask = vec![255u8; (DOC as usize) * (DOC as usize)];
    for y in 0..DOC {
        for x in 0..DOC {
            if !(40..60).contains(&x) || !(40..60).contains(&y) {
                mask[(y * DOC + x) as usize] = 0;
            }
        }
    }
    assert!(doc.apply_remove_background_mask(1, mask));
    let after_bounds = doc.layer_bounds(1).expect("cropped rect, transformed");
    let area = |b: (f32, f32, f32, f32)| (b.2 - b.0) * (b.3 - b.1);
    assert!(
        area(after_bounds) < area(before_bounds),
        "the crop should have shrunk the visible box: before={before_bounds:?} after={after_bounds:?}"
    );

    assert!(doc.undo());
    let restored = doc.layer_bounds(1).unwrap();
    let close = |a: f32, b: f32| (a - b).abs() < 0.01;
    assert!(
        close(restored.0, before_bounds.0)
            && close(restored.1, before_bounds.1)
            && close(restored.2, before_bounds.2)
            && close(restored.3, before_bounds.3),
        "undo must restore the exact pre-crop rotated/scaled bounds: {restored:?} vs {before_bounds:?}"
    );
}

#[test]
fn move_tool_picks_through_a_mask_hole_with_a_10x10_window() {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.resize_viewport(DOC as f32, DOC as f32, 1.0);
    doc.fit_to_view();
    doc.layers[1]
        .tiles_mut()
        .unwrap()
        .paint_rect(DocRect::new(10, 10, 90, 90), |_, _, _| {
            Some([255, 0, 0, 255])
        });
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
        .paint_rect(DocRect::new(10, 10, 90, 90), |_, _, _| {
            Some([255, 0, 0, 255])
        });
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
        .paint_rect(DocRect::new(10, 10, 90, 90), |_, _, _| {
            Some([0, 255, 0, 255])
        });
    doc.set_active_layer(1);
    assert!(doc.enter_transform());

    let (sx, sy) = doc.camera.to_screen(30.0, 30.0);
    doc.pointer_down(sx, sy);
    assert_eq!(
        doc.active_layer, 1,
        "inside the frame keeps the masked layer"
    );
    assert!(doc.transform_active);
    let (mx, my) = doc.camera.to_screen(40.0, 40.0);
    doc.pointer_move(mx, my);
    assert!(
        doc.layers[1].transform.is_some(),
        "the drag started even on a masked-out pixel"
    );
}
