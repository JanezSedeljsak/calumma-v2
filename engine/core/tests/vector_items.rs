use calumma_core::document::*;
use calumma_core::vector::{VectorItem, VectorPath, VectorShape};
use calumma_core::*;

const DOC: u32 = 200;

fn doc_with_viewport() -> Document {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.resize_viewport(DOC as f32, DOC as f32, 1.0);
    doc.fit_to_view();
    doc
}

fn rect_item(start: (f32, f32), end: (f32, f32)) -> VectorItem {
    VectorItem::Shape(VectorShape {
        shape: Shape {
            tool: Tool::Rect,
            start,
            end,
            half_width: 1.0,
            fill: true,
        },
        color: [255, 0, 0, 255],
    })
}

fn path_item(points: Vec<(f32, f32)>) -> VectorItem {
    VectorItem::Path(VectorPath {
        points,
        closed: false,
        fill: false,
        color: [0, 0, 255, 255],
        stroke_width: 4.0,
    })
}

fn vector_layer(doc: &mut Document, items: Vec<VectorItem>) -> usize {
    let index = doc.add_vector_layer("V");
    *doc.layers[index].content.items_mut().unwrap() = items;
    index
}

fn drag(doc: &mut Document, from: (f32, f32), to: (f32, f32)) {
    let down = doc.camera.to_screen(from.0, from.1);
    let up = doc.camera.to_screen(to.0, to.1);
    doc.pointer_down(down.0, down.1);
    doc.pointer_move(up.0, up.1);
    doc.pointer_up(up.0, up.1);
}

fn item_bounds(doc: &Document, layer: usize, item: usize) -> (f32, f32, f32, f32) {
    doc.layers[layer].content.items().unwrap()[item]
        .bounds()
        .unwrap()
}

#[test]
fn drawing_twice_in_vector_mode_fills_one_layer_with_two_items() {
    let mut doc = doc_with_viewport();
    doc.set_vector_mode(true);
    doc.tool = Tool::Rect;
    let before = doc.layers.len();
    drag(&mut doc, (10.0, 10.0), (40.0, 40.0));
    drag(&mut doc, (60.0, 60.0), (90.0, 90.0));
    assert_eq!(doc.layers.len(), before + 1);
    assert_eq!(doc.vector_item_count(doc.active_layer), 2);
}

#[test]
fn picking_finds_the_item_under_the_point() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(
        &mut doc,
        vec![
            rect_item((10.0, 10.0), (40.0, 40.0)),
            rect_item((60.0, 60.0), (90.0, 90.0)),
        ],
    );
    assert_eq!(
        doc.vector_item_at(20.0, 20.0),
        Some(VectorPick { layer, item: 0 })
    );
    assert_eq!(
        doc.vector_item_at(75.0, 75.0),
        Some(VectorPick { layer, item: 1 })
    );
    assert_eq!(doc.vector_item_at(150.0, 150.0), None);
}

#[test]
fn picking_prefers_the_item_drawn_last() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(
        &mut doc,
        vec![
            rect_item((10.0, 10.0), (60.0, 60.0)),
            rect_item((20.0, 20.0), (50.0, 50.0)),
        ],
    );
    assert_eq!(
        doc.vector_item_at(30.0, 30.0),
        Some(VectorPick { layer, item: 1 })
    );
}

#[test]
fn a_hairline_path_is_pickable_next_to_it_not_only_on_it() {
    let mut doc = doc_with_viewport();
    vector_layer(&mut doc, vec![path_item(vec![(20.0, 20.0), (80.0, 20.0)])]);
    assert!(doc.vector_item_at(50.0, 24.0).is_some());
    assert!(doc.vector_item_at(50.0, 60.0).is_none());
}

#[test]
fn an_outlined_shape_is_picked_from_the_inside_too() {
    let mut doc = doc_with_viewport();
    let hollow = VectorItem::Shape(VectorShape {
        shape: Shape {
            tool: Tool::Rect,
            start: (10.0, 10.0),
            end: (60.0, 60.0),
            half_width: 1.5,
            fill: false,
        },
        color: [255, 0, 0, 255],
    });
    let layer = vector_layer(&mut doc, vec![hollow]);
    assert_eq!(
        doc.vector_item_at(35.0, 35.0),
        Some(VectorPick { layer, item: 0 }),
        "a click in the middle of an outlined rect grabs the rect"
    );
    assert_eq!(doc.vector_item_at(90.0, 35.0), None);
}

#[test]
fn a_line_has_no_inside_to_pick() {
    let mut doc = doc_with_viewport();
    vector_layer(
        &mut doc,
        vec![VectorItem::Shape(VectorShape {
            shape: Shape {
                tool: Tool::Line,
                start: (10.0, 10.0),
                end: (90.0, 90.0),
                half_width: 1.0,
                fill: false,
            },
            color: [0, 0, 0, 255],
        })],
    );
    assert!(doc.vector_item_at(50.0, 50.0).is_some());
    assert!(doc.vector_item_at(80.0, 20.0).is_none());
}

#[test]
fn an_invisible_layer_is_not_pickable() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, vec![rect_item((10.0, 10.0), (40.0, 40.0))]);
    doc.layers[layer].visible = false;
    assert_eq!(doc.vector_item_at(20.0, 20.0), None);
}

#[test]
fn dragging_an_item_moves_only_that_item() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(
        &mut doc,
        vec![
            rect_item((10.0, 10.0), (40.0, 40.0)),
            rect_item((60.0, 60.0), (90.0, 90.0)),
        ],
    );
    let untouched = item_bounds(&doc, layer, 1);
    let before = item_bounds(&doc, layer, 0);
    doc.set_active_layer(layer);
    assert!(doc.enter_transform());
    drag(&mut doc, (20.0, 20.0), (35.0, 45.0));

    let moved = item_bounds(&doc, layer, 0);
    assert!((moved.0 - (before.0 + 15.0)).abs() < 0.01);
    assert!((moved.1 - (before.1 + 25.0)).abs() < 0.01);
    assert_eq!(item_bounds(&doc, layer, 1), untouched);
}

#[test]
fn a_drag_selects_the_item_it_grabbed() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, vec![rect_item((10.0, 10.0), (40.0, 40.0))]);
    doc.set_active_layer(layer);
    doc.enter_transform();
    drag(&mut doc, (20.0, 20.0), (25.0, 25.0));
    assert_eq!(
        doc.selected_vector_item(),
        Some(VectorPick { layer, item: 0 })
    );
    assert!(!doc.is_dragging_vector_item());
}

#[test]
fn clicking_empty_space_drops_the_selection() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, vec![rect_item((10.0, 10.0), (40.0, 40.0))]);
    doc.set_active_layer(layer);
    doc.enter_transform();
    assert!(doc.select_vector_item_at(20.0, 20.0));
    assert!(!doc.select_vector_item_at(150.0, 150.0));
    assert_eq!(doc.selected_vector_item(), None);
}

#[test]
fn a_drag_inside_a_moved_layer_follows_the_pointer() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, vec![rect_item((10.0, 10.0), (40.0, 40.0))]);
    doc.layers[layer].transform = Some(LayerTransform {
        offset_x: 50.0,
        offset_y: 0.0,
        ..LayerTransform::default()
    });
    doc.set_active_layer(layer);
    doc.enter_transform();
    let before = item_bounds(&doc, layer, 0);
    drag(&mut doc, (75.0, 25.0), (85.0, 25.0));
    let moved = item_bounds(&doc, layer, 0);
    assert!((moved.0 - (before.0 + 10.0)).abs() < 0.01);
}

#[test]
fn a_drag_inside_a_scaled_layer_moves_by_the_layer_scale() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, vec![rect_item((10.0, 10.0), (40.0, 40.0))]);
    doc.layers[layer].transform = Some(LayerTransform {
        scale_x: 2.0,
        scale_y: 2.0,
        ..LayerTransform::default()
    });
    doc.set_active_layer(layer);
    doc.enter_transform();
    let before = item_bounds(&doc, layer, 0);
    doc.begin_vector_item_drag(25.0, 25.0);
    doc.update_vector_item_drag(45.0, 25.0);
    let after = item_bounds(&doc, layer, 0);
    assert!((after.0 - (before.0 + 10.0)).abs() < 0.01);
}

#[test]
fn nudging_moves_the_selection_by_the_core_step() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, vec![rect_item((10.0, 10.0), (40.0, 40.0))]);
    doc.set_active_layer(layer);
    assert!(doc.select_vector_item_at(20.0, 20.0));
    let before = item_bounds(&doc, layer, 0);
    assert!(doc.nudge_selected_vector_item(3.0, -2.0));
    let after = item_bounds(&doc, layer, 0);
    assert!((after.0 - (before.0 + 3.0)).abs() < 0.01);
    assert!((after.1 - (before.1 - 2.0)).abs() < 0.01);
}

#[test]
fn deleting_removes_only_the_selected_item() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(
        &mut doc,
        vec![
            rect_item((10.0, 10.0), (40.0, 40.0)),
            rect_item((60.0, 60.0), (90.0, 90.0)),
        ],
    );
    doc.set_active_layer(layer);
    assert!(doc.select_vector_item_at(20.0, 20.0));
    assert!(doc.delete_selected_vector_item());
    assert_eq!(doc.vector_item_count(layer), 1);
    assert_eq!(doc.selected_vector_item(), None);
    assert!(doc.vector_item_at(75.0, 75.0).is_some());
}

/// The shell reads the selected item's index alone and takes its layer from
/// `CalmState.active_layer`; that only works because these two can never disagree.
#[test]
fn a_selected_item_always_lives_in_the_active_layer() {
    let mut doc = doc_with_viewport();
    let first = vector_layer(&mut doc, vec![rect_item((10.0, 10.0), (40.0, 40.0))]);
    let second = vector_layer(&mut doc, vec![rect_item((60.0, 60.0), (90.0, 90.0))]);
    doc.set_active_layer(second);

    assert!(doc.select_vector_item_at(20.0, 20.0));
    let pick = doc.selected_vector_item().unwrap();
    assert_eq!(pick.layer, first);
    assert_eq!(
        doc.active_layer, first,
        "picking retargets the active layer"
    );

    doc.set_active_layer(second);
    assert_eq!(
        doc.selected_vector_item(),
        None,
        "activating another layer drops the selection"
    );
}

#[test]
fn a_drag_stays_exact_over_many_frames() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(
        &mut doc,
        vec![path_item(vec![(10.0, 10.0), (20.0, 10.0), (20.0, 20.0)])],
    );
    doc.set_active_layer(layer);
    doc.enter_transform();
    let before = item_bounds(&doc, layer, 0);
    assert!(doc.begin_vector_item_drag(15.0, 10.0));
    for step in 1..=200 {
        doc.update_vector_item_drag(15.0 + step as f32 * 0.1, 10.0);
    }
    doc.end_vector_item_drag();
    let after = item_bounds(&doc, layer, 0);
    assert!(
        (after.0 - (before.0 + 20.0)).abs() < 1e-3,
        "200 frames of dragging land exactly where one 20px move would"
    );
}

#[test]
fn a_selection_does_not_survive_its_layer() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, vec![rect_item((10.0, 10.0), (40.0, 40.0))]);
    doc.set_active_layer(layer);
    assert!(doc.select_vector_item_at(20.0, 20.0));
    doc.remove_layer(layer);
    assert_eq!(doc.selected_vector_item(), None);
    assert!(!doc.nudge_selected_vector_item(1.0, 0.0));
}

#[test]
fn moving_an_item_bumps_the_revision_so_the_board_rebuilds() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, vec![rect_item((10.0, 10.0), (40.0, 40.0))]);
    doc.set_active_layer(layer);
    doc.select_vector_item_at(20.0, 20.0);
    let before = doc.vector_revision();
    doc.nudge_selected_vector_item(1.0, 0.0);
    assert_ne!(doc.vector_revision(), before);
}

#[test]
fn the_selection_box_follows_the_layer_transform() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, vec![rect_item((10.0, 10.0), (40.0, 40.0))]);
    doc.set_active_layer(layer);
    assert!(doc.select_vector_item_at(20.0, 20.0));
    let plain = doc.selected_vector_item_corners().unwrap();
    doc.layers[layer].transform = Some(LayerTransform {
        offset_x: 30.0,
        ..LayerTransform::default()
    });
    let moved = doc.selected_vector_item_corners().unwrap();
    for (a, b) in plain.iter().zip(moved.iter()) {
        assert!((b.0 - (a.0 + 30.0)).abs() < 0.01);
        assert!((b.1 - a.1).abs() < 0.01);
    }
}

#[test]
fn the_corner_handles_still_transform_the_whole_layer() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, vec![rect_item((10.0, 10.0), (40.0, 40.0))]);
    doc.set_active_layer(layer);
    assert!(doc.enter_transform());
    let (_, corners, _) = doc.transform_handles().unwrap();
    let corner = corners[2];
    drag(&mut doc, corner, (corner.0 + 20.0, corner.1 + 20.0));
    assert!(doc.layers[layer].transform.unwrap().scale_x > 1.0);
    assert_eq!(doc.selected_vector_item(), None);
}
