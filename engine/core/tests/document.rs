use calumma_core::document::*;
use calumma_core::*;

fn pixel(doc: &Document, index: usize, x: i32, y: i32) -> [u8; 4] {
    doc.layers[index].tiles().unwrap().get_pixel(x, y)
}

#[test]
fn pen_previews_then_commits() {
    let mut doc = Document::new("p".into(), "t", 256, 256);
    doc.resize_viewport(256.0, 256.0, 1.0);
    doc.fit_to_view();
    let (sx, sy) = doc.camera.to_screen(40.0, 40.0);
    doc.pointer_down(sx, sy);
    assert!(doc.stroke_active);
    assert!(!doc.stroke_points.is_empty());
    assert_eq!(pixel(&doc, doc.active_layer, 40, 40), [0, 0, 0, 0]);
    let (sx2, sy2) = doc.camera.to_screen(48.0, 40.0);
    doc.pointer_move(sx2, sy2);
    doc.pointer_up(sx2, sy2);
    assert!(!doc.stroke_active);
    assert!(doc.stroke_points.is_empty());
    assert_ne!(pixel(&doc, doc.active_layer, 40, 40), [0, 0, 0, 0]);
    assert!(doc.history.can_undo());
}

#[test]
fn shape_preview_then_commit() {
    let mut doc = Document::new("p".into(), "t", 256, 256);
    doc.tool = Tool::Rect;
    doc.fill = true;
    doc.resize_viewport(256.0, 256.0, 1.0);
    doc.fit_to_view();
    let (s0x, s0y) = doc.camera.to_screen(20.0, 20.0);
    let (s1x, s1y) = doc.camera.to_screen(60.0, 60.0);
    doc.pointer_down(s0x, s0y);
    assert!(doc.preview_shape.is_some());
    doc.pointer_move(s1x, s1y);
    doc.pointer_up(s1x, s1y);
    assert!(doc.preview_shape.is_none());
    assert!(!doc.layers[doc.active_layer].tiles().unwrap().is_empty());
}

#[test]
fn stamps_fill_gaps_between_points() {
    let points = [
        StrokePoint { x: 0.0, y: 0.0 },
        StrokePoint { x: 40.0, y: 0.0 },
    ];
    let stamps = stroke_stamps(&points, 2.0);
    assert!(stamps.len() > 40);
    for pair in stamps.windows(2) {
        let dx = pair[1].x - pair[0].x;
        let dy = pair[1].y - pair[0].y;
        assert!((dx * dx + dy * dy).sqrt() <= stamp_spacing(2.0) + 1e-3);
    }
    assert_eq!(stamps.last().map(|p| p.x), Some(40.0));
}

#[test]
fn fast_stroke_paints_across_tiles_and_undoes() {
    let mut doc = Document::new("p".into(), "t", 512, 512);
    doc.resize_viewport(512.0, 512.0, 1.0);
    doc.fit_to_view();
    let (sx0, sy0) = doc.camera.to_screen(20.0, 20.0);
    let (sx1, sy1) = doc.camera.to_screen(400.0, 400.0);
    doc.pointer_down(sx0, sy0);
    doc.pointer_up(sx1, sy1);
    assert_eq!(doc.stroke_points.len(), 0);
    assert_ne!(pixel(&doc, doc.active_layer, 200, 200), [0, 0, 0, 0]);
    assert_ne!(pixel(&doc, doc.active_layer, 300, 300), [0, 0, 0, 0]);
    assert!(doc.undo());
    assert_eq!(pixel(&doc, doc.active_layer, 200, 200), [0, 0, 0, 0]);
    assert_eq!(pixel(&doc, doc.active_layer, 300, 300), [0, 0, 0, 0]);
    assert!(doc.layers[doc.active_layer].tiles().unwrap().is_empty());
}

#[test]
fn shape_outside_board_paints_nothing() {
    let mut doc = Document::new("p".into(), "t", 128, 128);
    doc.tool = Tool::Rect;
    doc.resize_viewport(128.0, 128.0, 1.0);
    doc.fit_to_view();
    doc.fill = true;
    let (sx0, sy0) = doc.camera.to_screen(-900.0, -900.0);
    let (sx1, sy1) = doc.camera.to_screen(-500.0, -500.0);
    doc.pointer_down(sx0, sy0);
    doc.pointer_move(sx1, sy1);
    doc.pointer_up(sx1, sy1);
    assert!(doc.layers[doc.active_layer].tiles().unwrap().is_empty());
    assert!(!doc.history.can_undo());
}

#[test]
fn undo_stroke() {
    let mut doc = Document::new("p".into(), "t", 128, 128);
    doc.resize_viewport(128.0, 128.0, 1.0);
    doc.fit_to_view();
    let (sx, sy) = doc.camera.to_screen(16.0, 16.0);
    doc.pointer_down(sx, sy);
    doc.pointer_up(sx, sy);
    assert!(doc.undo());
    assert_eq!(pixel(&doc, doc.active_layer, 16, 16), [0, 0, 0, 0]);
}

#[test]
fn stroke_only_dirties_tiles_it_touched() {
    let mut doc = Document::new("p".into(), "t", 2048, 2048);
    doc.resize_viewport(2048.0, 2048.0, 1.0);
    doc.fit_to_view();
    doc.clear_layer_dirty(DirtyChannel::Render);
    let (sx, sy) = doc.camera.to_screen(100.0, 100.0);
    doc.pointer_down(sx, sy);
    doc.pointer_up(sx, sy);
    let dirty = doc.layers[doc.active_layer]
        .dirty_tiles(DirtyChannel::Render)
        .unwrap();
    assert_eq!(dirty.len(), 1);
    assert!(dirty.contains(&TileCoord { x: 0, y: 0 }));
}

#[test]
fn undo_after_clear_restores_pixels() {
    let mut doc = Document::new("p".into(), "t", 128, 128);
    let i = doc.active_layer;
    doc.layers[i]
        .tiles_mut()
        .unwrap()
        .set_pixel(5, 5, [1, 2, 3, 255]);
    doc.clear_active_layer();
    assert_eq!(pixel(&doc, doc.active_layer, 5, 5), [0, 0, 0, 0]);
    assert!(doc.undo());
    assert_eq!(pixel(&doc, doc.active_layer, 5, 5), [1, 2, 3, 255]);
}

#[test]
fn place_image_fills_the_first_paint_layer() {
    let mut doc = Document::new("p".into(), "art", 4, 2);
    let mut rgba = vec![0u8; 4 * 2 * 4];
    rgba[0..4].copy_from_slice(&[10, 20, 30, 255]);
    let last = (4 + 3) * 4;
    rgba[last..last + 4].copy_from_slice(&[40, 50, 60, 255]);
    assert!(doc.place_image(&rgba, 4, 2));
    assert_eq!(doc.active_layer, 1);
    assert!(doc.layers[0].is_paper());
    assert_eq!(pixel(&doc, 1, 0, 0), [10, 20, 30, 255]);
    assert_eq!(pixel(&doc, 1, 3, 1), [40, 50, 60, 255]);
}

#[test]
fn place_image_rejects_a_short_buffer() {
    let mut doc = Document::new("p".into(), "art", 4, 4);
    assert!(!doc.place_image(&[0u8; 8], 4, 4));
    assert!(doc.layers[1].tiles().unwrap().is_empty());
}

#[test]
fn clearing_empty_layer_pushes_no_history() {
    let mut doc = Document::new("p".into(), "t", 128, 128);
    doc.clear_active_layer();
    assert!(!doc.history.can_undo());
}

#[test]
fn rect_select_then_copy_extracts_only_selected_pixels() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    doc.layers[doc.active_layer]
        .tiles_mut()
        .unwrap()
        .set_pixel(10, 10, [1, 2, 3, 255]);
    doc.layers[doc.active_layer]
        .tiles_mut()
        .unwrap()
        .set_pixel(40, 40, [9, 9, 9, 255]);
    doc.tool = Tool::SelectRect;
    doc.resize_viewport(64.0, 64.0, 1.0);
    doc.fit_to_view();
    let (s0x, s0y) = doc.camera.to_screen(5.0, 5.0);
    let (s1x, s1y) = doc.camera.to_screen(20.0, 20.0);
    doc.pointer_down(s0x, s0y);
    doc.pointer_move(s1x, s1y);
    doc.pointer_up(s1x, s1y);
    assert!(doc.selection.is_some());

    let (w, h, rgba) = doc.selection_rgba().expect("selection copy");
    assert!((w as usize) * (h as usize) * 4 == rgba.len());
    let has_orange = rgba.chunks_exact(4).any(|px| px == [1, 2, 3, 255]);
    assert!(has_orange);
    let has_far_pixel = rgba.chunks_exact(4).any(|px| px == [9, 9, 9, 255]);
    assert!(!has_far_pixel);
}

#[test]
fn clear_selection_pixels_only_touches_selection_and_is_undoable() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    doc.layers[doc.active_layer]
        .tiles_mut()
        .unwrap()
        .set_pixel(10, 10, [1, 2, 3, 255]);
    doc.layers[doc.active_layer]
        .tiles_mut()
        .unwrap()
        .set_pixel(40, 40, [9, 9, 9, 255]);
    doc.selection = Some(Selection {
        shape: SelectionShape::Rect {
            start: (0.0, 0.0),
            end: (20.0, 20.0),
        },
    });
    assert!(doc.clear_selection_pixels());
    assert_eq!(pixel(&doc, doc.active_layer, 10, 10), [0, 0, 0, 0]);
    assert_eq!(pixel(&doc, doc.active_layer, 40, 40), [9, 9, 9, 255]);
    assert!(doc.undo());
    assert_eq!(pixel(&doc, doc.active_layer, 10, 10), [1, 2, 3, 255]);
}

#[test]
fn paste_image_creates_new_layer_at_selection_origin() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    doc.selection = Some(Selection {
        shape: SelectionShape::Rect {
            start: (10.0, 10.0),
            end: (12.0, 12.0),
        },
    });
    let rgba = vec![5u8, 6, 7, 255, 5, 6, 7, 255, 5, 6, 7, 255, 5, 6, 7, 255];
    let before = doc.layers.len();
    assert!(doc.paste_image_as_layer("Pasted", &rgba, 2, 2));
    assert_eq!(doc.layers.len(), before + 1);
    assert_eq!(pixel(&doc, doc.active_layer, 10, 10), [5, 6, 7, 255]);
    assert_eq!(pixel(&doc, doc.active_layer, 0, 0), [0, 0, 0, 0]);
}

#[test]
fn lasso_selection_commits_from_stroke_points() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    doc.tool = Tool::SelectLasso;
    doc.resize_viewport(64.0, 64.0, 1.0);
    doc.fit_to_view();
    for (x, y) in [(5.0, 5.0), (20.0, 5.0), (20.0, 20.0), (5.0, 20.0)] {
        let (sx, sy) = doc.camera.to_screen(x, y);
        if doc.stroke_active {
            doc.pointer_move(sx, sy);
        } else {
            doc.pointer_down(sx, sy);
        }
    }
    let (sx, sy) = doc.camera.to_screen(5.0, 20.0);
    doc.pointer_up(sx, sy);
    match &doc.selection {
        Some(Selection {
            shape: SelectionShape::Lasso { points },
        }) => assert!(points.len() >= 3),
        _ => panic!("expected a lasso selection"),
    }
}

#[test]
fn composite_rgba_blends_visible_layers_and_skips_hidden() {
    let mut doc = Document::new("p".into(), "t", 4, 4);
    doc.layers[0]
        .tiles_mut()
        .unwrap()
        .set_pixel(0, 0, [255, 255, 255, 255]);
    doc.layers[1]
        .tiles_mut()
        .unwrap()
        .set_pixel(1, 1, [10, 20, 30, 255]);
    doc.add_layer("Hidden");
    let hidden_index = doc.active_layer;
    doc.layers[hidden_index]
        .tiles_mut()
        .unwrap()
        .set_pixel(2, 2, [1, 1, 1, 255]);
    doc.set_layer_visible(hidden_index, false);

    let (w, h, rgba) = doc.composite_rgba();
    assert_eq!((w, h), (4, 4));
    let px = |x: usize, y: usize| -> [u8; 4] {
        let i = (y * w as usize + x) * 4;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    };
    assert_eq!(px(0, 0), [255, 255, 255, 255]);
    assert_eq!(px(1, 1), [10, 20, 30, 255]);
    // Paper is opaque white everywhere, so a skipped hidden layer still
    // shows paper through, not transparency — proves the hidden pixel
    // ([1, 1, 1, 255]) did not make it into the composite.
    assert_eq!(px(2, 2), [255, 255, 255, 255]);
}

#[test]
fn fill_tool_commits_on_pointer_down_and_undoes() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    doc.tool = Tool::Fill;
    doc.color = [200, 0, 0, 255];
    doc.resize_viewport(64.0, 64.0, 1.0);
    doc.fit_to_view();
    let (sx, sy) = doc.camera.to_screen(32.0, 32.0);
    doc.pointer_down(sx, sy);
    assert_eq!(pixel(&doc, doc.active_layer, 32, 32), [200, 0, 0, 255]);
    assert!(doc.history.can_undo());
    assert!(doc.undo());
    assert_eq!(pixel(&doc, doc.active_layer, 32, 32), [0, 0, 0, 0]);
}

#[test]
fn resize_grows_paper_and_preserves_content() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    doc.layers[doc.active_layer]
        .tiles_mut()
        .unwrap()
        .set_pixel(10, 10, [1, 2, 3, 255]);
    doc.resize(128, 96);
    assert_eq!((doc.width, doc.height), (128, 96));
    assert_eq!(pixel(&doc, doc.active_layer, 10, 10), [1, 2, 3, 255]);
    assert_eq!(pixel(&doc, 0, 100, 10), [255, 255, 255, 255]);
    assert_eq!(pixel(&doc, 0, 10, 80), [255, 255, 255, 255]);
    assert_eq!(pixel(&doc, 0, 10, 10), [255, 255, 255, 255]);
}

#[test]
fn resize_shrink_hides_but_does_not_delete_content() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    doc.layers[doc.active_layer]
        .tiles_mut()
        .unwrap()
        .set_pixel(50, 50, [9, 8, 7, 255]);
    doc.resize(20, 20);
    assert_eq!((doc.width, doc.height), (20, 20));
    let (w, _, rgba) = doc.composite_rgba();
    assert_eq!(w, 20);
    assert!(!rgba.chunks_exact(4).any(|px| px == [9, 8, 7, 255]));
    doc.resize(64, 64);
    assert_eq!(pixel(&doc, doc.active_layer, 50, 50), [9, 8, 7, 255]);
}

#[test]
fn resize_clamps_to_canvas_limits() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    doc.resize(0, 999_999);
    assert_eq!(doc.width, crate::limits::MIN_CANVAS_SIDE);
    assert_eq!(doc.height, crate::limits::MAX_CANVAS_SIDE);
}

fn paint_transform_target(doc: &mut Document) {
    let idx = doc.active_layer;
    doc.layers[idx]
        .tiles_mut()
        .unwrap()
        .paint_rect(DocRect::new(50, 50, 149, 149), |_, _, _| {
            Some([200, 30, 30, 255])
        });
}

#[test]
fn transform_corner_drag_scales_proportionally_without_shift() {
    let mut doc = Document::new("p".into(), "t", 200, 200);
    doc.resize_viewport(200.0, 200.0, 1.0);
    doc.fit_to_view();
    paint_transform_target(&mut doc);
    doc.tool = Tool::Transform;
    let (index, corners, _) = doc.transform_handles().expect("handles");
    assert_eq!(index, doc.active_layer);
    let br = corners[2];
    let (sx, sy) = doc.camera.to_screen(br.0, br.1);
    doc.pointer_down(sx, sy);
    let (sx2, sy2) = doc.camera.to_screen(br.0 + 25.0, br.1 + 25.0);
    doc.pointer_move(sx2, sy2);
    let t = doc.layer_transform(doc.active_layer);
    assert!(t.scale_x > 1.0);
    assert!((t.scale_x - t.scale_y).abs() < 1e-3);
    doc.pointer_up(sx2, sy2);
    assert_eq!(t, doc.layer_transform(doc.active_layer));
}

#[test]
fn transform_corner_drag_scales_freely_with_shift() {
    let mut doc = Document::new("p".into(), "t", 200, 200);
    doc.resize_viewport(200.0, 200.0, 1.0);
    doc.fit_to_view();
    paint_transform_target(&mut doc);
    doc.tool = Tool::Transform;
    doc.shift_held = true;
    let (_, corners, _) = doc.transform_handles().expect("handles");
    let br = corners[2];
    let (sx, sy) = doc.camera.to_screen(br.0, br.1);
    doc.pointer_down(sx, sy);
    let (sx2, sy2) = doc.camera.to_screen(br.0 + 60.0, br.1 + 5.0);
    doc.pointer_move(sx2, sy2);
    let t = doc.layer_transform(doc.active_layer);
    assert!((t.scale_x - t.scale_y).abs() > 0.1);
}

#[test]
fn transform_move_drag_updates_offset() {
    let mut doc = Document::new("p".into(), "t", 200, 200);
    doc.resize_viewport(200.0, 200.0, 1.0);
    doc.fit_to_view();
    paint_transform_target(&mut doc);
    doc.tool = Tool::Transform;
    let (sx, sy) = doc.camera.to_screen(100.0, 100.0);
    doc.pointer_down(sx, sy);
    let (sx2, sy2) = doc.camera.to_screen(120.0, 130.0);
    doc.pointer_move(sx2, sy2);
    let t = doc.layer_transform(doc.active_layer);
    assert!((t.offset_x - 20.0).abs() < 1.0);
    assert!((t.offset_y - 30.0).abs() < 1.0);
}

#[test]
fn transform_rotate_handle_updates_rotation() {
    let mut doc = Document::new("p".into(), "t", 200, 200);
    doc.resize_viewport(200.0, 200.0, 1.0);
    doc.fit_to_view();
    paint_transform_target(&mut doc);
    doc.tool = Tool::Transform;
    let (_, _, rotate_handle) = doc.transform_handles().expect("handles");
    let (sx, sy) = doc.camera.to_screen(rotate_handle.0, rotate_handle.1);
    doc.pointer_down(sx, sy);
    let (sx2, sy2) = doc.camera.to_screen(180.0, 100.0);
    doc.pointer_move(sx2, sy2);
    let t = doc.layer_transform(doc.active_layer);
    assert!(t.rotation.abs() > 0.1);
}

#[test]
fn reset_layer_transform_clears_it() {
    let mut doc = Document::new("p".into(), "t", 200, 200);
    doc.resize_viewport(200.0, 200.0, 1.0);
    doc.fit_to_view();
    paint_transform_target(&mut doc);
    doc.tool = Tool::Transform;
    let (_, corners, _) = doc.transform_handles().expect("handles");
    let br = corners[2];
    let (sx, sy) = doc.camera.to_screen(br.0, br.1);
    doc.pointer_down(sx, sy);
    let (sx2, sy2) = doc.camera.to_screen(br.0 + 25.0, br.1 + 25.0);
    doc.pointer_move(sx2, sy2);
    assert_ne!(
        doc.layer_transform(doc.active_layer),
        LayerTransform::default()
    );
    doc.reset_layer_transform(doc.active_layer);
    assert_eq!(
        doc.layer_transform(doc.active_layer),
        LayerTransform::default()
    );
}

#[test]
fn duplicate_layer_inserts_a_copy_above_and_selects_it() {
    let mut doc = Document::new("p".into(), "t", 32, 32);
    doc.layers[doc.active_layer]
        .tiles_mut()
        .unwrap()
        .set_pixel(5, 5, [1, 2, 3, 255]);
    let before = doc.layers.len();
    assert!(doc.duplicate_layer(doc.active_layer));
    assert_eq!(doc.layers.len(), before + 1);
    assert_eq!(pixel(&doc, doc.active_layer, 5, 5), [1, 2, 3, 255]);
    assert_ne!(
        doc.layers[doc.active_layer - 1].id,
        doc.layers[doc.active_layer].id
    );
}

#[test]
fn set_layer_opacity_fades_composite() {
    let mut doc = Document::new("p".into(), "t", 4, 4);
    doc.layers[doc.active_layer]
        .tiles_mut()
        .unwrap()
        .set_pixel(0, 0, [255, 0, 0, 255]);
    doc.set_layer_opacity(doc.active_layer, 0.5);
    let (_, _, rgba) = doc.composite_rgba();
    // Paint is red over an opaque white Paper layer: at half opacity the
    // red channel stays saturated (255 mixed with 255), but green/blue
    // move partway from 0 toward white's 255.
    assert!(rgba[1] > 50 && rgba[1] < 220);
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
fn composite_respects_layer_transform_offset() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    let idx = doc.active_layer;
    doc.layers[idx]
        .tiles_mut()
        .unwrap()
        .set_pixel(10, 10, [200, 30, 30, 255]);
    doc.layers[idx].transform = Some(LayerTransform {
        offset_x: 20.0,
        offset_y: 0.0,
        ..LayerTransform::default()
    });
    let (w, _, rgba) = doc.composite_rgba();
    let at = |x: i32, y: i32| -> [u8; 4] {
        let i = ((y as usize) * (w as usize) + (x as usize)) * 4;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    };
    assert_eq!(at(30, 10), [200, 30, 30, 255]);
    assert_eq!(at(10, 10), [255, 255, 255, 255]);
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

#[test]
fn fill_tool_stays_within_active_selection() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    doc.tool = Tool::Fill;
    doc.color = [1, 2, 3, 255];
    doc.selection = Some(Selection {
        shape: SelectionShape::Rect {
            start: (0.0, 0.0),
            end: (16.0, 64.0),
        },
    });
    doc.resize_viewport(64.0, 64.0, 1.0);
    doc.fit_to_view();
    let (sx, sy) = doc.camera.to_screen(8.0, 8.0);
    doc.pointer_down(sx, sy);
    assert_eq!(pixel(&doc, doc.active_layer, 8, 8), [1, 2, 3, 255]);
    assert_eq!(pixel(&doc, doc.active_layer, 40, 8), [0, 0, 0, 0]);
}
