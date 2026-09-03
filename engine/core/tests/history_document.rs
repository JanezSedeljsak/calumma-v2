use calumma_core::document::*;
use calumma_core::filters::Adjustments;
use calumma_core::history::History;
use calumma_core::layer::BlendMode;
use calumma_core::layer_align::{AlignEdge, DistributeAxis};
use calumma_core::limits;
use calumma_core::paste::PasteOutcome;
use calumma_core::transform::LayerTransform;
use calumma_core::vector::{VectorItem, VectorShape};
use calumma_core::*;

const DOC: u32 = 128;

fn fresh_doc() -> Document {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.resize_viewport(DOC as f32, DOC as f32, 1.0);
    doc.fit_to_view();
    doc
}

fn clear_history(doc: &mut Document) {
    doc.history = History::default();
}

fn paint(doc: &mut Document, index: usize, x: i32, y: i32, rgba: [u8; 4]) {
    doc.layers[index].tiles_mut().unwrap().set_pixel(x, y, rgba);
}

fn paint_rect(doc: &mut Document, index: usize, rect: DocRect, rgba: [u8; 4]) {
    doc.layers[index]
        .tiles_mut()
        .unwrap()
        .paint_rect(rect, |_, _, _| Some(rgba));
}

fn pixel(doc: &Document, index: usize, x: i32, y: i32) -> [u8; 4] {
    doc.layers[index].tiles().unwrap().get_pixel(x, y)
}

fn drag(doc: &mut Document, from: (f32, f32), to: (f32, f32)) {
    let down = doc.camera.to_screen(from.0, from.1);
    let up = doc.camera.to_screen(to.0, to.1);
    doc.pointer_down(down.0, down.1);
    doc.pointer_move(up.0, up.1);
    doc.pointer_up(up.0, up.1);
}

fn rect_item(start: (f32, f32), end: (f32, f32)) -> VectorItem {
    VectorItem::Shape(VectorShape {
        shape: Shape {
            tool: Tool::Rect,
            start,
            end,
            half_width: 1.0,
            fill: true,
            stroke: false,
        },
        color: [255, 0, 0, 255],
        stroke_color: [255, 0, 0, 255],
    })
}

fn item_bounds(doc: &Document, layer: usize) -> (f32, f32, f32, f32) {
    doc.layers[layer].content.item().unwrap().bounds().unwrap()
}

fn layer_aabb(doc: &Document, index: usize) -> (f32, f32, f32, f32) {
    let layer = &doc.layers[index];
    let raw = layer.content_bounds().unwrap();
    layer.transform.unwrap_or_default().transformed_aabb(raw)
}

#[test]
fn undo_add_layer_removes_it() {
    let mut doc = fresh_doc();
    clear_history(&mut doc);
    let before = doc.layers.len();
    doc.add_layer("Extra");
    assert_eq!(doc.layers.len(), before + 1);
    assert!(doc.history.can_undo());
    assert!(doc.undo());
    assert_eq!(doc.layers.len(), before);
}

#[test]
fn redo_restores_a_removed_layer_add() {
    let mut doc = fresh_doc();
    clear_history(&mut doc);
    let before = doc.layers.len();
    doc.add_layer("Extra");
    assert!(doc.undo());
    assert_eq!(doc.layers.len(), before);
    assert!(doc.redo());
    assert_eq!(doc.layers.len(), before + 1);
    assert_eq!(doc.layers.last().unwrap().name, "Extra");
}

#[test]
fn undo_remove_layer_brings_it_back() {
    let mut doc = fresh_doc();
    doc.add_layer("Gone");
    let index = doc.active_layer;
    paint(&mut doc, index, 8, 8, [9, 8, 7, 255]);
    let name = doc.layers[index].name.clone();
    clear_history(&mut doc);
    let before = doc.layers.len();
    assert!(doc.remove_layer(index));
    assert_eq!(doc.layers.len(), before - 1);
    assert!(doc.undo());
    assert_eq!(doc.layers.len(), before);
    let restored_index = doc.layers.iter().position(|l| l.name == name).unwrap();
    assert_eq!(pixel(&doc, restored_index, 8, 8), [9, 8, 7, 255]);
}

#[test]
fn undo_duplicate_layer() {
    let mut doc = fresh_doc();
    clear_history(&mut doc);
    let before = doc.layers.len();
    assert!(doc.duplicate_layer(doc.active_layer));
    assert_eq!(doc.layers.len(), before + 1);
    assert!(doc.undo());
    assert_eq!(doc.layers.len(), before);
}

#[test]
fn undo_duplicate_restores_the_previous_active_layer() {
    let mut doc = fresh_doc();
    let paint = doc.active_layer;
    clear_history(&mut doc);
    assert!(doc.duplicate_layer(paint));
    assert_ne!(doc.active_layer, paint);
    assert!(doc.undo());
    assert_eq!(doc.active_layer, paint);
}

#[test]
fn undo_move_layer_reorders_the_stack() {
    let mut doc = fresh_doc();
    doc.add_layer("A");
    doc.add_layer("B");
    let top = doc.active_layer;
    let names_before: Vec<_> = doc.layers.iter().map(|l| l.name.clone()).collect();
    clear_history(&mut doc);
    assert!(doc.move_layer(top, top - 1));
    assert_ne!(
        doc.layers
            .iter()
            .map(|l| l.name.as_str())
            .collect::<Vec<_>>(),
        names_before.iter().map(|s| s.as_str()).collect::<Vec<_>>()
    );
    assert!(doc.undo());
    assert_eq!(
        doc.layers
            .iter()
            .map(|l| l.name.clone())
            .collect::<Vec<_>>(),
        names_before
    );
}

#[test]
fn undo_layer_opacity() {
    let mut doc = fresh_doc();
    let index = doc.active_layer;
    let original = doc.layers[index].opacity;
    clear_history(&mut doc);
    doc.set_layer_opacity(index, 0.25);
    assert!((doc.layers[index].opacity - 0.25).abs() < 1e-6);
    assert!(doc.undo());
    assert!((doc.layers[index].opacity - original).abs() < 1e-6);
}

#[test]
fn setting_opacity_to_its_current_value_records_nothing() {
    let mut doc = fresh_doc();
    clear_history(&mut doc);
    doc.set_layer_opacity(doc.active_layer, doc.layers[doc.active_layer].opacity);
    assert!(!doc.history.can_undo());
}

#[test]
fn undo_blend_mode() {
    let mut doc = fresh_doc();
    let index = doc.active_layer;
    clear_history(&mut doc);
    doc.set_layer_blend_mode(index, BlendMode::Multiply);
    assert_eq!(doc.layers[index].blend_mode, BlendMode::Multiply);
    assert!(doc.undo());
    assert_eq!(doc.layers[index].blend_mode, BlendMode::Normal);
}

#[test]
fn undo_adjustments() {
    let mut doc = fresh_doc();
    let index = doc.active_layer;
    clear_history(&mut doc);
    doc.set_layer_adjustments(
        index,
        Adjustments {
            brightness: 0.4,
            contrast: -0.2,
            ..Adjustments::default()
        },
    );
    assert!(doc.layers[index].adjustments.is_some());
    assert!(doc.undo());
    assert!(doc.layers[index].adjustments.is_none());
}

#[test]
fn undo_reset_transform() {
    let mut doc = fresh_doc();
    let index = doc.active_layer;
    paint(&mut doc, index, 4, 4, [255, 0, 0, 255]);
    doc.layers[index].transform = Some(LayerTransform {
        offset_x: 12.0,
        offset_y: -6.0,
        ..LayerTransform::default()
    });
    clear_history(&mut doc);
    doc.reset_layer_transform(index);
    assert!(doc.layers[index].transform.is_none());
    assert!(doc.undo());
    assert_eq!(doc.layers[index].transform.unwrap().offset_x, 12.0);
}

#[test]
fn undo_layer_transform_drag() {
    let mut doc = fresh_doc();
    let layer = doc.active_layer;
    paint(&mut doc, layer, 8, 8, [255, 0, 0, 255]);
    clear_history(&mut doc);
    doc.set_tool(Tool::Move);
    assert!(doc.begin_move_at(8.0, 8.0));
    doc.update_move_drag(20.0, 20.0);
    doc.end_move_drag();
    assert!(doc.layers[doc.active_layer].transform.is_some());
    assert!(doc.undo());
    assert!(doc.layers[doc.active_layer].transform.is_none());
}

fn paint_transform_target(doc: &mut Document) {
    let layer = doc.active_layer;
    paint_rect(doc, layer, DocRect::new(50, 50, 99, 99), [255, 0, 0, 255]);
}

#[test]
fn undo_transform_mode_corner_drag() {
    let mut doc = fresh_doc();
    paint_transform_target(&mut doc);
    clear_history(&mut doc);
    assert!(doc.enter_transform());
    let (_, corners, _) = doc.transform_handles().expect("handles");
    let br = corners[2];
    let before = doc.layer_transform(doc.active_layer).scale_x;
    let (sx, sy) = doc.camera.to_screen(br.0, br.1);
    doc.pointer_down(sx, sy);
    let (sx2, sy2) = doc.camera.to_screen(br.0 + 25.0, br.1 + 25.0);
    doc.pointer_move(sx2, sy2);
    doc.pointer_up(sx2, sy2);
    let after = doc.layer_transform(doc.active_layer).scale_x;
    assert!((after - before).abs() > 0.01);
    assert!(doc.undo());
    assert!((doc.layer_transform(doc.active_layer).scale_x - before).abs() < 0.01);
}

#[test]
fn undo_nudge_move_target() {
    let mut doc = fresh_doc();
    let layer = doc.active_layer;
    paint(&mut doc, layer, 10, 10, [255, 0, 0, 255]);
    clear_history(&mut doc);
    doc.set_tool(Tool::Move);
    assert!(doc.nudge_move_target(2.0, 0.0));
    let offset = doc.layers[doc.active_layer].transform.unwrap().offset_x;
    assert!(offset > 0.0);
    assert!(doc.undo());
    assert!(doc.layers[doc.active_layer].transform.is_none());
}

#[test]
fn undo_resize_document() {
    let mut doc = fresh_doc();
    clear_history(&mut doc);
    doc.resize(96, 80);
    assert_eq!((doc.width, doc.height), (96, 80));
    assert!(doc.undo());
    assert_eq!((doc.width, doc.height), (DOC, DOC));
}

#[test]
fn undo_merge_layer_down() {
    let mut doc = fresh_doc();
    doc.add_layer("Top");
    let top = doc.active_layer;
    paint(&mut doc, top, 4, 4, [255, 0, 0, 255]);
    clear_history(&mut doc);
    let before = doc.layers.len();
    assert!(doc.merge_layer_down(top));
    assert_eq!(doc.layers.len(), before - 1);
    assert!(doc.undo());
    assert_eq!(doc.layers.len(), before);
    assert_eq!(pixel(&doc, top, 4, 4), [255, 0, 0, 255]);
}

#[test]
fn undo_clip_layer_down() {
    let mut doc = Document::new("p".into(), "t", 32, 32);
    doc.add_layer("Base");
    let base = doc.active_layer;
    doc.layers[base]
        .tiles_mut()
        .unwrap()
        .fill_uniform(DocRect::new(8, 8, 23, 23), [0, 0, 255, 255]);
    doc.add_layer("Top");
    let top = doc.active_layer;
    doc.layers[top]
        .tiles_mut()
        .unwrap()
        .fill_uniform(DocRect::new(0, 0, 31, 31), [255, 0, 0, 255]);
    clear_history(&mut doc);
    let before = doc.layers.len();
    assert!(doc.clip_layer_down(top));
    assert_eq!(doc.layers.len(), before - 1);
    assert!(doc.undo());
    assert_eq!(doc.layers.len(), before);
}

#[test]
fn undo_align_layers() {
    let mut doc = fresh_doc();
    doc.add_layer("B");
    let a = doc.active_layer;
    doc.add_layer("C");
    let b = doc.active_layer;
    paint(&mut doc, a, 10, 10, [255, 0, 0, 255]);
    paint(&mut doc, b, 40, 10, [0, 255, 0, 255]);
    doc.layers[b].transform = Some(LayerTransform {
        offset_x: 20.0,
        ..LayerTransform::default()
    });
    clear_history(&mut doc);
    doc.set_layer_selection(&[a, b]);
    assert!(doc.align_layers(&[a, b], AlignEdge::Left));
    let aligned = doc.layers[b].transform.unwrap().offset_x;
    assert!(doc.undo());
    assert_ne!(doc.layers[b].transform.unwrap().offset_x, aligned);
}

#[test]
fn undo_distribute_layers() {
    let mut doc = fresh_doc();
    doc.add_layer("Layer 2");
    doc.add_layer("Layer 3");
    doc.add_layer("Layer 4");
    paint_rect(&mut doc, 1, DocRect::new(10, 10, 30, 30), [255, 0, 0, 255]);
    paint_rect(&mut doc, 2, DocRect::new(35, 10, 45, 30), [0, 255, 0, 255]);
    paint_rect(&mut doc, 3, DocRect::new(55, 10, 75, 30), [0, 0, 255, 255]);
    paint_rect(
        &mut doc,
        4,
        DocRect::new(100, 10, 115, 30),
        [255, 255, 0, 255],
    );
    clear_history(&mut doc);
    let before = layer_aabb(&doc, 2).0;
    assert!(doc.distribute_layers(&[1, 2, 3, 4], DistributeAxis::Horizontal));
    assert!((layer_aabb(&doc, 2).0 - before).abs() > 0.5);
    assert!(doc.undo());
    assert!((layer_aabb(&doc, 2).0 - before).abs() < 0.01);
}

#[test]
fn undo_vector_shape_add_in_vector_mode() {
    let mut doc = fresh_doc();
    clear_history(&mut doc);
    doc.set_vector_mode(true);
    doc.tool = Tool::Rect;
    let before = doc.layers.len();
    drag(&mut doc, (10.0, 10.0), (40.0, 40.0));
    assert_eq!(doc.layers.len(), before + 1);
    assert!(doc.history.can_undo());
    assert!(doc.undo());
    assert_eq!(doc.layers.len(), before);
}

#[test]
fn undo_vector_item_drag() {
    let mut doc = fresh_doc();
    let layer = doc.add_vector_layer("V", rect_item((10.0, 10.0), (40.0, 40.0)));
    clear_history(&mut doc);
    let before = item_bounds(&doc, layer);
    doc.set_tool(Tool::Move);
    assert!(doc.begin_vector_item_drag(25.0, 25.0));
    doc.update_vector_item_drag(55.0, 45.0);
    doc.end_vector_item_drag();
    let after = item_bounds(&doc, layer);
    assert!((after.0 - before.0).abs() > 1.0);
    assert!(doc.undo());
    let restored = item_bounds(&doc, layer);
    assert!((restored.0 - before.0).abs() < 0.01);
}

#[test]
fn undo_delete_vector_item() {
    let mut doc = fresh_doc();
    let layer = doc.add_vector_layer("V", rect_item((10.0, 10.0), (40.0, 40.0)));
    let before = doc.layers.len();
    doc.select_vector_item_at(25.0, 25.0);
    clear_history(&mut doc);
    assert!(doc.delete_selected_vector_item());
    assert_eq!(doc.layers.len(), before - 1);
    assert!(doc.undo());
    assert_eq!(doc.layers.len(), before);
    assert!(doc.layers[layer].content.item().is_some());
}

#[test]
fn undo_paste_image_as_layer() {
    let mut doc = fresh_doc();
    clear_history(&mut doc);
    let rgba = vec![5u8, 6, 7, 255, 5, 6, 7, 255, 5, 6, 7, 255, 5, 6, 7, 255];
    let before = doc.layers.len();
    assert_eq!(
        doc.paste_image_as_layer("Pasted", &rgba, 2, 2),
        PasteOutcome::Native
    );
    assert_eq!(doc.layers.len(), before + 1);
    assert_eq!(pixel(&doc, doc.active_layer, 0, 0), [5, 6, 7, 255]);
    assert!(doc.undo());
    assert_eq!(doc.layers.len(), before);
}

#[test]
fn undo_rasterize_text_layer() {
    let mut doc = fresh_doc();
    doc.tool = Tool::Text;
    let (sx, sy) = doc.camera.to_screen(40.0, 40.0);
    doc.pointer_down(sx, sy);
    doc.text_insert("Hi");
    let layer = doc.active_layer;
    assert!(doc.layers[layer].is_text());
    clear_history(&mut doc);
    assert!(doc.rasterize_text_layer(layer));
    assert!(!doc.layers[layer].is_text());
    assert!(doc.undo());
    assert!(doc.layers[layer].is_text());
}

#[test]
fn undo_rasterize_vector_layer() {
    let mut doc = fresh_doc();
    let layer = doc.add_vector_layer("V", rect_item((10.0, 10.0), (50.0, 50.0)));
    clear_history(&mut doc);
    assert!(doc.rasterize_vector_layer(layer));
    assert!(!doc.layers[layer].content.is_vector());
    assert!(doc.undo());
    assert!(doc.layers[layer].content.item().is_some());
}

#[test]
fn paint_and_document_undo_steps_stack_independently() {
    let mut doc = fresh_doc();
    clear_history(&mut doc);
    let layer = doc.active_layer;
    let coord = TileCoord { x: 0, y: 0 };
    let before = doc.layers[layer].tiles().unwrap().snapshot_tiles(&[coord]);
    paint(&mut doc, layer, 5, 5, [255, 0, 0, 255]);
    doc.history
        .push_layer_tiles(doc.layers[layer].id.clone(), before, Some(layer));
    doc.add_layer("Extra");
    assert_eq!(doc.history.undo_depth(), 2);
    assert!(doc.undo());
    assert_eq!(doc.layers.len(), 2);
    assert!(doc.undo());
    assert_eq!(pixel(&doc, layer, 5, 5), [0, 0, 0, 0]);
}

#[test]
fn a_no_op_transform_drag_records_nothing() {
    let mut doc = fresh_doc();
    let layer = doc.active_layer;
    paint(&mut doc, layer, 8, 8, [255, 0, 0, 255]);
    clear_history(&mut doc);
    doc.set_tool(Tool::Move);
    assert!(doc.begin_move_at(8.0, 8.0));
    doc.end_move_drag();
    assert!(!doc.history.can_undo());
}

#[test]
fn redo_merge_layer_down() {
    let mut doc = fresh_doc();
    doc.add_layer("Top");
    let top = doc.active_layer;
    paint(&mut doc, top, 4, 4, [255, 0, 0, 255]);
    clear_history(&mut doc);
    let before = doc.layers.len();
    assert!(doc.merge_layer_down(top));
    assert_eq!(doc.layers.len(), before - 1);
    assert!(doc.undo());
    assert_eq!(doc.layers.len(), before);
    assert!(doc.redo());
    assert_eq!(doc.layers.len(), before - 1);
}

#[test]
fn redo_resize_document() {
    let mut doc = fresh_doc();
    clear_history(&mut doc);
    doc.resize(96, 80);
    assert!(doc.undo());
    assert_eq!((doc.width, doc.height), (DOC, DOC));
    assert!(doc.redo());
    assert_eq!((doc.width, doc.height), (96, 80));
}

#[test]
fn redo_blend_mode() {
    let mut doc = fresh_doc();
    let index = doc.active_layer;
    clear_history(&mut doc);
    doc.set_layer_blend_mode(index, BlendMode::Screen);
    assert!(doc.undo());
    assert_eq!(doc.layers[index].blend_mode, BlendMode::Normal);
    assert!(doc.redo());
    assert_eq!(doc.layers[index].blend_mode, BlendMode::Screen);
}

#[test]
fn redo_remove_layer() {
    let mut doc = fresh_doc();
    doc.add_layer("Gone");
    let index = doc.active_layer;
    paint(&mut doc, index, 3, 3, [1, 2, 3, 255]);
    clear_history(&mut doc);
    let before = doc.layers.len();
    assert!(doc.remove_layer(index));
    assert_eq!(doc.layers.len(), before - 1);
    assert!(doc.undo());
    assert_eq!(doc.layers.len(), before);
    assert!(doc.redo());
    assert_eq!(doc.layers.len(), before - 1);
}

#[test]
fn undo_move_layer_up() {
    let mut doc = fresh_doc();
    doc.add_layer("A");
    let a = doc.active_layer;
    doc.add_layer("B");
    let b = doc.active_layer;
    let names_before: Vec<_> = doc.layers.iter().map(|l| l.name.clone()).collect();
    clear_history(&mut doc);
    assert!(doc.move_layer_up(a));
    assert_ne!(
        doc.layers
            .iter()
            .map(|l| l.name.as_str())
            .collect::<Vec<_>>(),
        names_before.iter().map(|s| s.as_str()).collect::<Vec<_>>()
    );
    assert!(doc.undo());
    assert_eq!(
        doc.layers
            .iter()
            .map(|l| l.name.clone())
            .collect::<Vec<_>>(),
        names_before
    );
    assert_eq!(doc.layers[b].name, "B");
}

#[test]
fn undo_stack_op_restores_multi_layer_selection() {
    let mut doc = fresh_doc();
    doc.add_layer("B");
    paint_rect(&mut doc, 1, DocRect::new(10, 10, 30, 30), [255, 0, 0, 255]);
    paint_rect(&mut doc, 2, DocRect::new(50, 50, 70, 70), [0, 255, 0, 255]);
    doc.set_layer_selection(&[1, 2]);
    clear_history(&mut doc);
    doc.add_layer("C");
    assert!(doc.undo());
    doc.set_tool(Tool::Move);
    assert!(doc.nudge_move_target(1.0, 0.0));
    let step = limits::LAYER_NUDGE_STEP;
    let t1 = doc.layers[1].transform.expect("layer 1 nudged");
    let t2 = doc.layers[2].transform.expect("layer 2 nudged");
    assert!((t1.offset_x - step).abs() < 0.01);
    assert!((t2.offset_x - step).abs() < 0.01);
}

#[test]
fn undo_vector_nudge() {
    let mut doc = fresh_doc();
    let layer = doc.add_vector_layer("V", rect_item((10.0, 10.0), (40.0, 40.0)));
    doc.set_active_layer(layer);
    assert!(doc.select_vector_item_at(25.0, 25.0));
    let before = item_bounds(&doc, layer);
    clear_history(&mut doc);
    assert!(doc.nudge_selected_vector_item(3.0, -2.0));
    let after = item_bounds(&doc, layer);
    assert!((after.0 - before.0).abs() > 0.5);
    assert!(doc.undo());
    let restored = item_bounds(&doc, layer);
    assert!((restored.0 - before.0).abs() < 0.01);
    assert!((restored.1 - before.1).abs() < 0.01);
}

#[test]
fn failed_paste_records_no_history() {
    let mut doc = fresh_doc();
    clear_history(&mut doc);
    let before = doc.layers.len();
    let rgba = vec![0u8; 16 * 16 * 4];
    assert_eq!(
        doc.paste_image_as_layer("Empty", &rgba, 16, 16),
        PasteOutcome::Failed
    );
    assert_eq!(doc.layers.len(), before);
    assert!(!doc.history.can_undo());
}

#[test]
fn undo_clip_restores_source_pixels_outside_the_base() {
    let mut doc = Document::new("p".into(), "t", 32, 32);
    doc.add_layer("Base");
    let base = doc.active_layer;
    doc.layers[base]
        .tiles_mut()
        .unwrap()
        .fill_uniform(DocRect::new(8, 8, 23, 23), [0, 0, 255, 255]);
    doc.add_layer("Top");
    let top = doc.active_layer;
    doc.layers[top]
        .tiles_mut()
        .unwrap()
        .fill_uniform(DocRect::new(0, 0, 31, 31), [255, 0, 0, 255]);
    let outside = pixel(&doc, top, 4, 4);
    clear_history(&mut doc);
    let before = doc.layers.len();
    assert!(doc.clip_layer_down(top));
    assert_eq!(doc.layers.len(), before - 1);
    assert!(doc.undo());
    assert_eq!(doc.layers.len(), before);
    let restored = doc.layers.iter().position(|l| l.name == "Top").unwrap();
    assert_eq!(pixel(&doc, restored, 4, 4), outside);
}

#[test]
fn undo_overflow_paste_removes_the_layer() {
    let mut doc = fresh_doc();
    clear_history(&mut doc);
    let before = doc.layers.len();
    let rgba = vec![255u8; 200 * 200 * 4];
    assert_eq!(
        doc.paste_image_as_layer("Big", &rgba, 200, 200),
        PasteOutcome::Overflowing
    );
    assert_eq!(doc.layers.len(), before + 1);
    assert!(doc.undo());
    assert_eq!(doc.layers.len(), before);
}

#[test]
fn redo_distribute_layers() {
    let mut doc = fresh_doc();
    doc.add_layer("Layer 2");
    doc.add_layer("Layer 3");
    doc.add_layer("Layer 4");
    paint_rect(&mut doc, 1, DocRect::new(10, 10, 30, 30), [255, 0, 0, 255]);
    paint_rect(&mut doc, 2, DocRect::new(35, 10, 45, 30), [0, 255, 0, 255]);
    paint_rect(&mut doc, 3, DocRect::new(55, 10, 75, 30), [0, 0, 255, 255]);
    paint_rect(
        &mut doc,
        4,
        DocRect::new(100, 10, 115, 30),
        [255, 255, 0, 255],
    );
    clear_history(&mut doc);
    let before = layer_aabb(&doc, 2).0;
    assert!(doc.distribute_layers(&[1, 2, 3, 4], DistributeAxis::Horizontal));
    let distributed = layer_aabb(&doc, 2).0;
    assert!((distributed - before).abs() > 0.5);
    assert!(doc.undo());
    assert!((layer_aabb(&doc, 2).0 - before).abs() < 0.01);
    assert!(doc.redo());
    assert!((layer_aabb(&doc, 2).0 - distributed).abs() < 0.01);
}

#[test]
fn undo_and_redo_walk_through_mixed_steps() {
    let mut doc = fresh_doc();
    clear_history(&mut doc);
    let paint_layer = doc.active_layer;
    doc.add_layer("Extra");
    doc.set_layer_opacity(paint_layer, 0.5);
    assert_eq!(doc.history.undo_depth(), 2);
    assert!(doc.undo());
    assert!((doc.layers[paint_layer].opacity - 1.0).abs() < 1e-6);
    assert!(doc.undo());
    assert_eq!(doc.layers.len(), 2);
    assert!(doc.redo());
    assert_eq!(doc.layers.len(), 3);
    assert!(doc.redo());
    assert!((doc.layers[paint_layer].opacity - 0.5).abs() < 1e-6);
    assert!(!doc.history.can_redo());
}
