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
            stroke: false,
        },
        color: [255, 0, 0, 255],
        stroke_color: [255, 0, 0, 255],
    })
}

fn path_item(points: Vec<(f32, f32)>) -> VectorItem {
    VectorItem::Path(VectorPath {
        points,
        closed: false,
        fill: false,
        stroke: true,
        color: [0, 0, 255, 255],
        stroke_color: [0, 0, 255, 255],
        stroke_width: 4.0,
    })
}

fn filled_rect_path(x0: f32, y0: f32, x1: f32, y1: f32) -> VectorItem {
    VectorItem::Path(VectorPath {
        points: vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)],
        closed: true,
        fill: true,
        stroke: false,
        color: [255, 0, 0, 255],
        stroke_color: [255, 0, 0, 255],
        stroke_width: 1.0,
    })
}

fn vector_layer(doc: &mut Document, item: VectorItem) -> usize {
    doc.add_vector_layer("V", item)
}

fn paint(doc: &mut Document, index: usize, rect: DocRect, rgba: [u8; 4]) {
    doc.layers[index]
        .tiles_mut()
        .unwrap()
        .paint_rect(rect, |_, _, _| Some(rgba));
}

fn drag(doc: &mut Document, from: (f32, f32), to: (f32, f32)) {
    let down = doc.camera.to_screen(from.0, from.1);
    let up = doc.camera.to_screen(to.0, to.1);
    doc.pointer_down(down.0, down.1);
    doc.pointer_move(up.0, up.1);
    doc.pointer_up(up.0, up.1);
}

fn item_bounds(doc: &Document, layer: usize) -> (f32, f32, f32, f32) {
    doc.layers[layer].content.item().unwrap().bounds().unwrap()
}

#[test]
fn drawing_twice_in_vector_mode_makes_two_layers() {
    let mut doc = doc_with_viewport();
    doc.set_vector_mode(true);
    doc.tool = Tool::Rect;
    let before = doc.layers.len();
    drag(&mut doc, (10.0, 10.0), (40.0, 40.0));
    drag(&mut doc, (60.0, 60.0), (90.0, 90.0));
    assert_eq!(doc.layers.len(), before + 2);
    assert_eq!(doc.vector_item_count(doc.active_layer), 1);
}

#[test]
fn vector_item_stores_ink_opacity() {
    let mut doc = doc_with_viewport();
    doc.set_vector_mode(true);
    doc.tool = Tool::Rect;
    doc.fill = true;
    doc.shape_fill_color = [10, 20, 30, 255];
    doc.set_ink_opacity(0.5);
    drag(&mut doc, (10.0, 10.0), (40.0, 40.0));
    let item = &doc.layers[doc.active_layer].content.item().unwrap();
    match item {
        VectorItem::Shape(shape) => assert_eq!(shape.color, [10, 20, 30, 128]),
        VectorItem::Path(_) => panic!("expected a shape item"),
    }
}

#[test]
fn picking_finds_the_item_under_the_point() {
    let mut doc = doc_with_viewport();
    let a = vector_layer(&mut doc, rect_item((10.0, 10.0), (40.0, 40.0)));
    let b = vector_layer(&mut doc, rect_item((60.0, 60.0), (90.0, 90.0)));
    assert_eq!(
        doc.vector_item_at(20.0, 20.0),
        Some(VectorPick { layer: a })
    );
    assert_eq!(
        doc.vector_item_at(75.0, 75.0),
        Some(VectorPick { layer: b })
    );
    assert_eq!(doc.vector_item_at(150.0, 150.0), None);
}

#[test]
fn picking_prefers_the_item_drawn_last() {
    let mut doc = doc_with_viewport();
    let _under = vector_layer(&mut doc, rect_item((10.0, 10.0), (60.0, 60.0)));
    let top = vector_layer(&mut doc, rect_item((20.0, 20.0), (50.0, 50.0)));
    assert_eq!(
        doc.vector_item_at(30.0, 30.0),
        Some(VectorPick { layer: top })
    );
}

#[test]
fn a_hairline_path_is_pickable_next_to_it_not_only_on_it() {
    let mut doc = doc_with_viewport();
    vector_layer(&mut doc, path_item(vec![(20.0, 20.0), (80.0, 20.0)]));
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
            stroke: true,
        },
        color: [255, 0, 0, 255],
        stroke_color: [255, 0, 0, 255],
    });
    let layer = vector_layer(&mut doc, hollow);
    assert_eq!(
        doc.vector_item_at(35.0, 35.0),
        Some(VectorPick { layer }),
        "a click in the middle of an outlined rect grabs the rect"
    );
    assert_eq!(doc.vector_item_at(90.0, 35.0), None);
}

#[test]
fn a_line_has_no_inside_to_pick() {
    let mut doc = doc_with_viewport();
    vector_layer(
        &mut doc,
        VectorItem::Shape(VectorShape {
            shape: Shape {
                tool: Tool::Line,
                start: (10.0, 10.0),
                end: (90.0, 90.0),
                half_width: 1.0,
                fill: false,
                stroke: true,
            },
            color: [0, 0, 0, 255],
            stroke_color: [0, 0, 0, 255],
        }),
    );
    assert!(doc.vector_item_at(50.0, 50.0).is_some());
    assert!(doc.vector_item_at(80.0, 20.0).is_none());
}

#[test]
fn an_invisible_layer_is_not_pickable() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, rect_item((10.0, 10.0), (40.0, 40.0)));
    doc.layers[layer].visible = false;
    assert_eq!(doc.vector_item_at(20.0, 20.0), None);
}

#[test]
fn picking_skips_a_hidden_raster_layer_above() {
    let mut doc = doc_with_viewport();
    doc.add_layer("Blocker");
    let blocker = doc.active_layer;
    paint(
        &mut doc,
        blocker,
        DocRect::new(0, 0, 120, 120),
        [0, 0, 0, 255],
    );
    doc.set_layer_visible(blocker, false);
    let svg = vector_layer(&mut doc, filled_rect_path(10.0, 10.0, 50.0, 50.0));
    doc.set_tool(Tool::Move);
    assert_eq!(
        doc.vector_item_at(30.0, 30.0),
        Some(VectorPick { layer: svg })
    );
    assert!(doc.begin_move_at(30.0, 30.0));
}

#[test]
fn picking_respects_visible_raster_ink_above() {
    let mut doc = doc_with_viewport();
    let _svg = vector_layer(&mut doc, filled_rect_path(10.0, 10.0, 50.0, 50.0));
    doc.add_layer("Cover");
    let cover = doc.active_layer;
    paint(
        &mut doc,
        cover,
        DocRect::new(0, 0, 120, 120),
        [0, 0, 0, 255],
    );
    assert_eq!(doc.vector_item_at(30.0, 30.0), None);
}

#[test]
fn move_tool_can_grab_a_visible_vector_layer_as_a_whole() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, filled_rect_path(10.0, 10.0, 50.0, 50.0));
    doc.set_tool(Tool::Move);
    assert_eq!(doc.layer_at(30.0, 30.0), Some(layer));
    assert!(doc.begin_move_at(30.0, 30.0));
}

#[test]
fn dragging_an_item_moves_only_that_item() {
    let mut doc = doc_with_viewport();
    let a = vector_layer(&mut doc, rect_item((10.0, 10.0), (40.0, 40.0)));
    let b = vector_layer(&mut doc, rect_item((60.0, 60.0), (90.0, 90.0)));
    let untouched = item_bounds(&doc, b);
    let before = item_bounds(&doc, a);
    doc.set_active_layer(a);
    assert!(doc.enter_transform());
    drag(&mut doc, (20.0, 20.0), (35.0, 45.0));

    let moved = item_bounds(&doc, a);
    assert!((moved.0 - (before.0 + 15.0)).abs() < 0.01);
    assert!((moved.1 - (before.1 + 25.0)).abs() < 0.01);
    assert_eq!(item_bounds(&doc, b), untouched);
}

#[test]
fn a_drag_selects_the_item_it_grabbed() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, rect_item((10.0, 10.0), (40.0, 40.0)));
    doc.set_active_layer(layer);
    doc.enter_transform();
    drag(&mut doc, (20.0, 20.0), (25.0, 25.0));
    assert_eq!(doc.selected_vector_item(), Some(VectorPick { layer }));
    assert!(!doc.is_dragging_vector_item());
}

#[test]
fn clicking_empty_space_drops_the_selection() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, rect_item((10.0, 10.0), (40.0, 40.0)));
    doc.set_active_layer(layer);
    doc.enter_transform();
    assert!(doc.select_vector_item_at(20.0, 20.0));
    assert!(!doc.select_vector_item_at(150.0, 150.0));
    assert_eq!(doc.selected_vector_item(), None);
}

#[test]
fn a_drag_inside_a_moved_layer_follows_the_pointer() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, rect_item((10.0, 10.0), (40.0, 40.0)));
    doc.layers[layer].transform = Some(LayerTransform {
        offset_x: 50.0,
        offset_y: 0.0,
        ..LayerTransform::default()
    });
    doc.set_active_layer(layer);
    doc.enter_transform();
    let before = item_bounds(&doc, layer);
    drag(&mut doc, (75.0, 25.0), (85.0, 25.0));
    let moved = item_bounds(&doc, layer);
    assert!((moved.0 - (before.0 + 10.0)).abs() < 0.01);
}

#[test]
fn a_drag_inside_a_scaled_layer_moves_by_the_layer_scale() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, rect_item((10.0, 10.0), (40.0, 40.0)));
    doc.layers[layer].transform = Some(LayerTransform {
        scale_x: 2.0,
        scale_y: 2.0,
        ..LayerTransform::default()
    });
    doc.set_active_layer(layer);
    doc.enter_transform();
    let before = item_bounds(&doc, layer);
    doc.begin_vector_item_drag(25.0, 25.0);
    doc.update_vector_item_drag(45.0, 25.0);
    let after = item_bounds(&doc, layer);
    assert!((after.0 - (before.0 + 10.0)).abs() < 0.01);
}

#[test]
fn nudging_moves_the_selection_by_the_core_step() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, rect_item((10.0, 10.0), (40.0, 40.0)));
    doc.set_active_layer(layer);
    assert!(doc.select_vector_item_at(20.0, 20.0));
    let before = item_bounds(&doc, layer);
    assert!(doc.nudge_selected_vector_item(3.0, -2.0));
    let after = item_bounds(&doc, layer);
    assert!((after.0 - (before.0 + 3.0)).abs() < 0.01);
    assert!((after.1 - (before.1 - 2.0)).abs() < 0.01);
}

#[test]
fn deleting_removes_only_the_selected_item() {
    let mut doc = doc_with_viewport();
    let a = vector_layer(&mut doc, rect_item((10.0, 10.0), (40.0, 40.0)));
    let _b = vector_layer(&mut doc, rect_item((60.0, 60.0), (90.0, 90.0)));
    doc.set_active_layer(a);
    assert!(doc.select_vector_item_at(20.0, 20.0));
    assert!(doc.delete_selected_vector_item());
    assert_eq!(doc.selected_vector_item(), None);
    assert_eq!(doc.vector_item_at(20.0, 20.0), None);
    assert!(doc.vector_item_at(75.0, 75.0).is_some());
}

/// The shell reads the selected item's index alone and takes its layer from
/// `CalmState.active_layer`; that only works because these two can never disagree.
#[test]
fn a_selected_item_always_lives_in_the_active_layer() {
    let mut doc = doc_with_viewport();
    let first = vector_layer(&mut doc, rect_item((10.0, 10.0), (40.0, 40.0)));
    let second = vector_layer(&mut doc, rect_item((60.0, 60.0), (90.0, 90.0)));
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
        path_item(vec![(10.0, 10.0), (20.0, 10.0), (20.0, 20.0)]),
    );
    doc.set_active_layer(layer);
    doc.enter_transform();
    let before = item_bounds(&doc, layer);
    assert!(doc.begin_vector_item_drag(15.0, 10.0));
    for step in 1..=200 {
        doc.update_vector_item_drag(15.0 + step as f32 * 0.1, 10.0);
    }
    doc.end_vector_item_drag();
    let after = item_bounds(&doc, layer);
    assert!(
        (after.0 - (before.0 + 20.0)).abs() < 1e-3,
        "200 frames of dragging land exactly where one 20px move would"
    );
}

#[test]
fn a_selection_does_not_survive_its_layer() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, rect_item((10.0, 10.0), (40.0, 40.0)));
    doc.set_active_layer(layer);
    assert!(doc.select_vector_item_at(20.0, 20.0));
    doc.remove_layer(layer);
    assert_eq!(doc.selected_vector_item(), None);
    assert!(!doc.nudge_selected_vector_item(1.0, 0.0));
}

#[test]
fn moving_an_item_bumps_the_revision_so_the_board_rebuilds() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, rect_item((10.0, 10.0), (40.0, 40.0)));
    doc.set_active_layer(layer);
    doc.select_vector_item_at(20.0, 20.0);
    let before = doc.vector_revision();
    doc.nudge_selected_vector_item(1.0, 0.0);
    assert_ne!(doc.vector_revision(), before);
}

#[test]
fn the_selection_box_follows_the_layer_transform() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, rect_item((10.0, 10.0), (40.0, 40.0)));
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
    let layer = vector_layer(&mut doc, rect_item((10.0, 10.0), (40.0, 40.0)));
    doc.set_active_layer(layer);
    assert!(doc.enter_transform());
    let (_, corners, _) = doc.transform_handles().unwrap();
    let corner = corners[2];
    drag(&mut doc, corner, (corner.0 + 20.0, corner.1 + 20.0));
    assert!(doc.layers[layer].transform.unwrap().scale_x > 1.0);
    assert_eq!(doc.selected_vector_item(), None);
}

fn item_geometry(doc: &Document, layer: usize) -> (f32, f32, f32, f32) {
    doc.layers[layer]
        .content
        .item()
        .unwrap()
        .geometry_bounds()
        .unwrap()
}

fn item_corner(doc: &Document, corner: usize) -> (f32, f32) {
    doc.selected_vector_item_corners().unwrap()[corner]
}

fn assert_bounds(actual: (f32, f32, f32, f32), expected: (f32, f32, f32, f32)) {
    for (a, b) in [
        (actual.0, expected.0),
        (actual.1, expected.1),
        (actual.2, expected.2),
        (actual.3, expected.3),
    ] {
        assert!(
            (a - b).abs() < 0.01,
            "expected {expected:?}, got {actual:?}"
        );
    }
}

/// A `rect_item` is filled with no outline, so `Shape::padding` is the antialiased pixel
/// alone — the pad every resize assertion below has to account for.
const RECT_PAD: f32 = 1.0;

/// Where a `rect_item((10, 10), (40, 40))` lands after its bottom-right corner is dragged
/// 20px out along both axes: the drag reaches the *ink* box, so the pad is on both ends of
/// the answer as well as on the corner that was grabbed.
fn dragged_rect_bounds() -> (f32, f32, f32, f32) {
    (
        -10.0 - RECT_PAD,
        -10.0 - RECT_PAD,
        60.0 + RECT_PAD,
        60.0 + RECT_PAD,
    )
}

#[test]
fn a_corner_handle_resizes_the_item_not_the_layer() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, rect_item((10.0, 10.0), (40.0, 40.0)));
    let other = vector_layer(&mut doc, rect_item((60.0, 60.0), (90.0, 90.0)));
    let untouched = item_bounds(&doc, other);
    doc.set_active_layer(layer);
    assert!(doc.enter_transform());
    assert!(doc.select_vector_item_at(20.0, 20.0));

    let corner = item_corner(&doc, 2);
    drag(&mut doc, corner, (corner.0 + 20.0, corner.1 + 20.0));

    let resized = item_bounds(&doc, layer);
    assert_bounds(resized, dragged_rect_bounds());
    assert_eq!(
        item_bounds(&doc, other),
        untouched,
        "the other layer is untouched"
    );
    assert!(
        doc.layers[layer].transform.is_none(),
        "resizing an item leaves the layer transform alone"
    );
}

#[test]
fn the_item_frame_takes_the_layer_frame_over_while_one_is_selected() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, rect_item((10.0, 10.0), (40.0, 40.0)));
    doc.set_active_layer(layer);
    assert!(doc.enter_transform());
    assert!(doc.transform_handles().is_some());

    assert!(doc.select_vector_item_at(20.0, 20.0));
    assert!(
        doc.transform_handles().is_none(),
        "the layer's corners stand down so only one frame is ever on screen"
    );
    assert!(doc.selected_vector_item_corners().is_some());

    doc.clear_vector_selection();
    assert_eq!(doc.selected_vector_item(), None);
    assert!(
        doc.transform_handles().is_some(),
        "dropping the selection hands the frame back to the layer"
    );
}

#[test]
fn a_layer_corner_is_not_clickable_while_an_item_holds_the_frame() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, rect_item((60.0, 60.0), (140.0, 140.0)));
    doc.set_active_layer(layer);
    assert!(doc.enter_transform());
    let layer_corner = doc.transform_handles().unwrap().1[0];
    assert!(doc.select_vector_item_at(100.0, 100.0));

    drag(
        &mut doc,
        layer_corner,
        (layer_corner.0 - 30.0, layer_corner.1 - 30.0),
    );
    assert!(
        doc.layers[layer].transform.is_none(),
        "the hidden layer corner scales nothing"
    );
}

/// Same drag, same item, in two documents — the only difference is Shift. Aspect is read off
/// the *geometry* box rather than `bounds`, whose constant ink pad would blur the ratio.
#[test]
fn shift_frees_the_two_axes_and_the_default_keeps_the_aspect() {
    let stretch = |shift: bool| {
        let mut doc = doc_with_viewport();
        let layer = vector_layer(&mut doc, rect_item((10.0, 10.0), (40.0, 20.0)));
        doc.set_active_layer(layer);
        doc.enter_transform();
        assert!(doc.select_vector_item_at(25.0, 15.0));
        doc.set_shift_held(shift);
        let corner = item_corner(&doc, 2);
        drag(&mut doc, corner, (corner.0 + 30.0, corner.1 + 5.0));
        let g = item_geometry(&doc, layer);
        (g.2 - g.0) / (g.3 - g.1)
    };
    assert!(
        (stretch(false) - 3.0).abs() < 0.01,
        "the default corner drag holds the 30x10 aspect"
    );
    assert!(
        (stretch(true) - 4.5).abs() < 0.01,
        "Shift scales the axes independently"
    );
}

/// The dragged corner has to land under the pointer, which is only true because the stroke
/// pad comes off both sides of the ratio. An arrow is the case that proves it: its head pads
/// the box by 29px here, so ignoring the pad would leave the corner a long way behind.
#[test]
fn a_padded_item_resizes_to_exactly_where_the_pointer_is() {
    let mut doc = doc_with_viewport();
    let arrow = VectorItem::Shape(VectorShape {
        shape: Shape {
            tool: Tool::Arrow,
            start: (50.0, 50.0),
            end: (100.0, 80.0),
            half_width: 4.0,
            fill: false,
            stroke: true,
        },
        color: [0, 0, 0, 255],
        stroke_color: [0, 0, 0, 255],
    });
    let layer = vector_layer(&mut doc, arrow);
    doc.set_active_layer(layer);
    doc.enter_transform();
    assert!(doc.select_vector_item_at(75.0, 65.0));
    doc.set_shift_held(true);

    let corner = item_corner(&doc, 2);
    let target = (corner.0 + 30.0, corner.1 + 30.0);
    drag(&mut doc, corner, target);

    let landed = item_bounds(&doc, layer);
    assert!((landed.2 - target.0).abs() < 0.01, "{landed:?}");
    assert!((landed.3 - target.1).abs() < 0.01, "{landed:?}");
}

#[test]
fn a_resize_leaves_the_ink_width_alone() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(
        &mut doc,
        path_item(vec![(20.0, 20.0), (60.0, 20.0), (60.0, 60.0)]),
    );
    doc.set_active_layer(layer);
    doc.enter_transform();
    assert!(doc.select_vector_item_at(40.0, 20.0));

    let corner = item_corner(&doc, 2);
    drag(&mut doc, corner, (corner.0 + 40.0, corner.1 + 40.0));

    match &doc.layers[layer].content.item().unwrap() {
        VectorItem::Path(p) => assert_eq!(p.stroke_width, 4.0),
        VectorItem::Shape(_) => panic!("expected a path item"),
    }
}

#[test]
fn a_resize_stays_exact_over_many_frames() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, rect_item((10.0, 10.0), (40.0, 40.0)));
    doc.set_active_layer(layer);
    doc.enter_transform();
    assert!(doc.select_vector_item_at(20.0, 20.0));

    let corner = item_corner(&doc, 2);
    assert!(doc.begin_vector_item_drag(corner.0, corner.1));
    for step in 1..=200 {
        let at = corner.0 + step as f32 * 0.1;
        doc.update_vector_item_drag(at, at);
    }
    doc.end_vector_item_drag();

    assert_bounds(item_bounds(&doc, layer), dragged_rect_bounds());
}

#[test]
fn a_resize_inside_a_scaled_layer_reads_the_pointer_through_the_layer() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, rect_item((10.0, 10.0), (40.0, 40.0)));
    doc.layers[layer].transform = Some(LayerTransform {
        scale_x: 2.0,
        scale_y: 2.0,
        ..LayerTransform::default()
    });
    doc.set_active_layer(layer);
    doc.enter_transform();
    assert!(doc.select_vector_item_at(25.0, 25.0));
    doc.set_shift_held(true);

    let corner = item_corner(&doc, 2);
    let scaled_corner = 25.0 + (15.0 + RECT_PAD) * 2.0;
    assert_bounds(
        (corner.0, corner.1, corner.0, corner.1),
        (scaled_corner, scaled_corner, scaled_corner, scaled_corner),
    );
    drag(&mut doc, corner, (corner.0 + 20.0, corner.1 + 20.0));

    assert_bounds(
        item_bounds(&doc, layer),
        (-RECT_PAD, -RECT_PAD, 50.0 + RECT_PAD, 50.0 + RECT_PAD),
    );
}

#[test]
fn the_move_tool_resizes_from_the_same_corners() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, rect_item((10.0, 10.0), (40.0, 40.0)));
    doc.set_active_layer(layer);
    doc.set_tool(Tool::Move);
    assert!(doc.select_vector_item_at(20.0, 20.0));

    let corner = item_corner(&doc, 2);
    drag(&mut doc, corner, (corner.0 + 20.0, corner.1 + 20.0));
    assert_bounds(item_bounds(&doc, layer), dragged_rect_bounds());
}

#[test]
fn a_locked_layer_refuses_a_resize() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, rect_item((10.0, 10.0), (40.0, 40.0)));
    doc.set_active_layer(layer);
    doc.enter_transform();
    assert!(doc.select_vector_item_at(20.0, 20.0));
    let corner = item_corner(&doc, 2);
    let before = item_bounds(&doc, layer);

    doc.set_layer_locked(layer, true);
    assert!(!doc.begin_vector_item_drag(corner.0, corner.1));
    assert_eq!(item_bounds(&doc, layer), before);
}

#[test]
fn a_corner_handle_outranks_an_item_lying_under_it() {
    let mut doc = doc_with_viewport();
    let under = vector_layer(&mut doc, rect_item((10.0, 10.0), (40.0, 40.0)));
    let _over = vector_layer(&mut doc, rect_item((36.0, 36.0), (90.0, 90.0)));
    doc.set_active_layer(under);
    doc.enter_transform();
    assert!(doc.select_vector_item_at(15.0, 15.0));
    let pick = doc.selected_vector_item().unwrap();

    let corner = item_corner(&doc, 2);
    assert!(doc.begin_vector_item_drag(corner.0, corner.1));
    assert_eq!(
        doc.selected_vector_item(),
        Some(pick),
        "the handle keeps its own item rather than selecting the one beneath it"
    );
}

#[test]
fn resizing_an_item_bumps_the_revision_so_the_board_rebuilds() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(&mut doc, rect_item((10.0, 10.0), (40.0, 40.0)));
    doc.set_active_layer(layer);
    doc.enter_transform();
    assert!(doc.select_vector_item_at(20.0, 20.0));
    let corner = item_corner(&doc, 2);
    assert!(doc.begin_vector_item_drag(corner.0, corner.1));

    let before = doc.vector_revision();
    assert!(doc.update_vector_item_drag(corner.0 + 10.0, corner.1 + 10.0));
    assert_ne!(doc.vector_revision(), before);
}

/// `record_vector_history` used to price every vector undo entry at a flat 128 bytes no
/// matter how many points a `VectorPath` actually held, so a long freehand path's undo stack
/// could hold real megabytes while `History::memory_used` believed each step cost 128B. A
/// drag records the item's state *before* the drag (`commit_vector_drag_history`), so the
/// point count — and so the byte cost — is exactly the path's own.
#[test]
fn vector_undo_cost_scales_with_the_paths_point_count() {
    let drag_one = |points: Vec<(f32, f32)>| -> usize {
        let mut doc = doc_with_viewport();
        let start = points[0];
        let layer = vector_layer(&mut doc, path_item(points));
        doc.set_active_layer(layer);
        doc.enter_transform();
        assert!(doc.begin_vector_item_drag(start.0, start.1));
        doc.update_vector_item_drag(start.0 + 10.0, start.1);
        doc.end_vector_item_drag();
        doc.history.memory_used()
    };

    let short = drag_one(vec![(10.0, 10.0), (20.0, 10.0)]);
    let long = drag_one((0..2000).map(|i| (i as f32 * 0.01, 10.0)).collect());

    assert!(short > 0, "a real cost, not zero");
    assert!(
        long > short * 100,
        "a 2000-point path ({long}B) should cost far more than a 2-point one ({short}B), \
         not the same flat estimate"
    );
}

/// A vector layer's `content_bounds()` caches the O(n) point scan `VectorPath` bounds cost,
/// keyed on a cheap fingerprint of the points rather than re-deriving it — this only holds
/// because `set_translated`/`set_scaled` rebuild every point from an affine map applied
/// uniformly (`AGENTS.md`'s "Basic vector editing only"). The point of the test: the cache
/// must never answer with the *previous* bounds after a move.
#[test]
fn vector_content_bounds_cache_tracks_a_drag_not_just_the_first_call() {
    let mut doc = doc_with_viewport();
    let layer = vector_layer(
        &mut doc,
        path_item(vec![(10.0, 10.0), (20.0, 10.0), (20.0, 20.0)]),
    );
    doc.set_active_layer(layer);
    doc.enter_transform();

    let before = doc.layers[layer].content_bounds().unwrap();
    // Prime the cache with the pre-drag bounds before anything moves.
    assert_eq!(doc.layers[layer].content_bounds().unwrap(), before);

    assert!(doc.begin_vector_item_drag(15.0, 10.0));
    doc.update_vector_item_drag(115.0, 10.0);
    doc.end_vector_item_drag();

    let after = doc.layers[layer].content_bounds().unwrap();
    assert!(
        (after.0 - (before.0 + 100.0)).abs() < 1e-3,
        "moved 100px right: before={before:?} after={after:?}"
    );
}
