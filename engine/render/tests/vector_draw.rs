use calumma_core::tile::DocRect;
use calumma_core::vector::{VectorItem, VectorPath, VectorShape};
use calumma_core::{Document, Layer, LayerTransform, Shape, Tool};
use calumma_render::vector_draw::*;

fn shape_item(start: (f32, f32), end: (f32, f32)) -> VectorItem {
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

fn path_item(points: Vec<(f32, f32)>, closed: bool, fill: bool) -> VectorItem {
    VectorItem::Path(VectorPath {
        points,
        closed,
        fill,
        color: [0, 0, 255, 255],
        stroke: !fill,
        stroke_color: [0, 0, 255, 255],
        stroke_width: 4.0,
    })
}

fn layer_with(item: VectorItem, transform: Option<LayerTransform>) -> Layer {
    let mut layer = Layer::vector("V", item);
    layer.transform = transform;
    layer
}

#[test]
fn a_path_becomes_one_instance_per_segment() {
    let mut out = Vec::new();
    let VectorItem::Path(path) =
        path_item(vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)], false, false)
    else {
        unreachable!()
    };
    push_path_instances(&path, None, &mut out);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].segment, [0.0, 0.0, 10.0, 0.0]);
    assert_eq!(out[0].brush[0], 2.0, "radius is half the stroke width");
}

#[test]
fn a_closed_path_gets_the_segment_back_to_its_start() {
    let mut out = Vec::new();
    let VectorItem::Path(path) =
        path_item(vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)], true, false)
    else {
        unreachable!()
    };
    push_path_instances(&path, None, &mut out);
    assert_eq!(out.len(), 3);
    assert_eq!(out[2].segment, [10.0, 10.0, 0.0, 0.0]);
}

#[test]
fn a_single_point_path_still_draws_a_dot() {
    let mut out = Vec::new();
    let VectorItem::Path(path) = path_item(vec![(4.0, 6.0)], false, false) else {
        unreachable!()
    };
    push_path_instances(&path, None, &mut out);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].segment, [4.0, 6.0, 4.0, 6.0]);
}

#[test]
fn a_filled_closed_path_is_left_to_the_rasterizer() {
    let mut out = Vec::new();
    let VectorItem::Path(path) = path_item(vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)], true, true)
    else {
        unreachable!()
    };
    push_path_instances(&path, None, &mut out);
    assert!(out.is_empty());
}

#[test]
fn a_layer_offset_moves_both_the_points_and_the_shape_parameters() {
    let offset = Some(LayerTransform {
        offset_x: 100.0,
        offset_y: 0.0,
        ..LayerTransform::default()
    });
    let shape = shape_item((10.0, 10.0), (40.0, 40.0));
    let placement = vector_placement(&layer_with(shape.clone(), offset));
    assert!(placement.is_some(), "a moved layer needs a placement");
    let VectorItem::Shape(s) = &shape else {
        unreachable!()
    };
    assert_eq!(shape_instance(s, placement).p0, [110.0, 10.0]);

    let path = path_item(vec![(0.0, 0.0), (10.0, 0.0)], false, false);
    let mut out = Vec::new();
    let VectorItem::Path(p) = &path else {
        unreachable!()
    };
    push_path_instances(
        p,
        vector_placement(&layer_with(path.clone(), offset)),
        &mut out,
    );
    assert_eq!(out[0].segment, [100.0, 0.0, 110.0, 0.0]);
}

#[test]
fn an_untransformed_layer_needs_no_placement() {
    let layer = layer_with(shape_item((10.0, 10.0), (40.0, 40.0)), None);
    assert!(vector_placement(&layer).is_none());
}

#[test]
fn a_scaled_layer_scales_the_stroke_with_the_geometry() {
    let item = path_item(vec![(0.0, 0.0), (10.0, 0.0)], false, false);
    let layer = layer_with(
        item.clone(),
        Some(LayerTransform {
            scale_x: 2.0,
            scale_y: 2.0,
            ..LayerTransform::default()
        }),
    );
    let mut out = Vec::new();
    let VectorItem::Path(path) = &item else {
        unreachable!()
    };
    push_path_instances(path, vector_placement(&layer), &mut out);
    assert_eq!(out[0].brush[0], 4.0);
}

#[test]
fn items_off_screen_are_culled_before_they_reach_the_gpu() {
    let near = shape_item((10.0, 10.0), (40.0, 40.0));
    let far = shape_item((5000.0, 5000.0), (5040.0, 5040.0));
    let visible = DocRect::new(0, 0, 200, 200);
    assert!(item_visible(&near, None, visible));
    assert!(!item_visible(&far, None, visible));
}

#[test]
fn culling_asks_where_the_layer_transform_put_the_item() {
    let item = shape_item((5000.0, 5000.0), (5040.0, 5040.0));
    let layer = layer_with(
        item.clone(),
        Some(LayerTransform {
            offset_x: -4900.0,
            offset_y: -4900.0,
            ..LayerTransform::default()
        }),
    );
    let visible = DocRect::new(0, 0, 200, 200);
    let placement = vector_placement(&layer);
    assert!(
        !item_visible(&item, None, visible),
        "untransformed it is off"
    );
    assert!(item_visible(&item, placement, visible), "moved it is on");
}

#[test]
fn the_selection_box_is_drawn_only_when_something_is_selected() {
    let doc = Document::new("p".into(), "t", 64, 64);
    assert!(vector_selection_instances(&doc).is_empty());
}

#[test]
fn vector_placement_is_none_for_a_non_vector_layer_even_with_a_transform() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    doc.add_layer("Paint");
    let layer = doc.layers.last_mut().unwrap();
    layer.transform = Some(LayerTransform {
        offset_x: 5.0,
        ..LayerTransform::default()
    });
    assert!(vector_placement(layer).is_none());
}

#[test]
fn vector_placement_is_none_for_an_empty_transformed_layer() {
    let layer = layer_with(
        VectorItem::Path(VectorPath {
            points: vec![],
            closed: false,
            fill: false,
            stroke: true,
            color: [0, 0, 0, 255],
            stroke_color: [0, 0, 0, 255],
            stroke_width: 1.0,
        }),
        Some(LayerTransform {
            offset_x: 5.0,
            ..LayerTransform::default()
        }),
    );
    assert!(vector_placement(&layer).is_none());
}

#[test]
fn a_boundless_item_is_never_visible() {
    let boundless = VectorItem::Path(VectorPath {
        points: vec![],
        closed: false,
        fill: false,
        color: [0, 0, 0, 255],
        stroke: true,
        stroke_color: [0, 0, 0, 255],
        stroke_width: 1.0,
    });
    let visible = DocRect::new(0, 0, 200, 200);
    assert!(!item_visible(&boundless, None, visible));
}

#[test]
fn an_empty_path_pushes_no_instances() {
    let mut out = Vec::new();
    let VectorItem::Path(path) = path_item(vec![], false, false) else {
        unreachable!()
    };
    push_path_instances(&path, None, &mut out);
    assert!(out.is_empty());
}

#[test]
fn shape_instance_marks_outline_shapes_as_unfilled() {
    let shape = VectorShape {
        shape: Shape {
            tool: Tool::Rect,
            start: (0.0, 0.0),
            end: (10.0, 10.0),
            half_width: 1.0,
            fill: false,
            stroke: true,
        },
        color: [1, 2, 3, 255],
        stroke_color: [1, 2, 3, 255],
    };
    assert_eq!(shape_instance(&shape, None).fill, 0.0);
}

#[test]
fn vector_selection_instances_draws_four_edges_and_four_corner_dots() {
    let mut doc = Document::new("p".into(), "t", 200, 200);
    doc.resize_viewport(200.0, 200.0, 1.0);
    doc.fit_to_view();
    let index = doc.add_vector_layer("V", shape_item((10.0, 10.0), (40.0, 40.0)));
    doc.set_active_layer(index);
    assert!(doc.select_vector_item_at(20.0, 20.0));

    let out = vector_selection_instances(&doc);
    // Four edges, then two discs per corner: the grey border and the white grip over it.
    assert_eq!(
        out.len(),
        4 + 4 * 2,
        "four edges plus four bordered corner dots"
    );
    for dot in &out[4..] {
        assert_eq!(
            (dot.segment[0], dot.segment[1]),
            (dot.segment[2], dot.segment[3]),
            "a corner dot is a degenerate segment"
        );
    }
}

fn selected_doc() -> Document {
    let mut doc = Document::new("p".into(), "t", 200, 200);
    doc.resize_viewport(200.0, 200.0, 1.0);
    doc.fit_to_view();
    let index = doc.add_vector_layer("V", shape_item((10.0, 10.0), (40.0, 40.0)));
    doc.set_active_layer(index);
    assert!(doc.select_vector_item_at(20.0, 20.0));
    doc
}

/// The item frame is the layer frame minus the rotate stalk — one fewer edge and one fewer
/// handle — because per-item rotation is not something the board can draw. Offering a handle
/// that did nothing would be worse than not offering one.
#[test]
fn the_item_frame_is_the_layer_frame_without_the_rotate_stalk() {
    let doc = selected_doc();
    let item = vector_selection_instances(&doc);
    let layer = calumma_render::compose::box_overlay_instances(
        doc.selected_vector_item_corners().unwrap(),
        Some((0.0, -30.0)),
    );
    assert_eq!(item.len(), 4 + 4 * 2);
    assert_eq!(
        layer.len(),
        4 + 1 + 5 * 2,
        "the stalk is one edge and one handle more"
    );
    assert_eq!(
        item[..4],
        layer[..4],
        "the four box edges are drawn identically at both levels"
    );
}

/// Both frames draw the same furniture, so a corner handle looks the same whether it belongs
/// to a layer or to one item inside it — the box is what changed, not the affordance.
#[test]
fn a_corner_handle_looks_the_same_at_both_levels() {
    let doc = selected_doc();
    let corners = doc.selected_vector_item_corners().unwrap();
    let item = vector_selection_instances(&doc);
    let layer = calumma_render::compose::box_overlay_instances(corners, Some((0.0, -30.0)));
    // Two discs per handle now, so the corner handles start one edge later on the layer frame
    // and run in pairs. Both halves have to match for the affordance to read the same.
    for i in 0..8 {
        assert_eq!(item[4 + i].color, layer[5 + i].color);
        assert_eq!(item[4 + i].brush, layer[5 + i].brush);
    }
}

/// The frame is drawn from the item's own box, so it has to sit exactly on it — a frame that
/// drifted from the geometry would put the resize handles somewhere the item is not.
#[test]
fn the_item_frame_sits_on_the_items_own_bounds() {
    let doc = selected_doc();
    let bounds = doc.layers[doc.active_layer]
        .content
        .item()
        .unwrap()
        .bounds()
        .unwrap();
    let out = vector_selection_instances(&doc);
    let xs: Vec<f32> = out[4..].iter().map(|d| d.segment[0]).collect();
    let ys: Vec<f32> = out[4..].iter().map(|d| d.segment[1]).collect();
    let min_x = xs.iter().cloned().fold(f32::INFINITY, f32::min);
    let max_x = xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let min_y = ys.iter().cloned().fold(f32::INFINITY, f32::min);
    let max_y = ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    assert!((min_x - bounds.0).abs() < 0.01, "{min_x} vs {}", bounds.0);
    assert!((max_x - bounds.2).abs() < 0.01, "{max_x} vs {}", bounds.2);
    assert!((min_y - bounds.1).abs() < 0.01);
    assert!((max_y - bounds.3).abs() < 0.01);
}

/// Selecting an item is what takes the frame off the layer, so the two can never both be on
/// screen — the renderer relies on that to draw them from independent branches.
#[test]
fn only_one_frame_is_ever_live_at_a_time() {
    let mut doc = selected_doc();
    assert!(doc.enter_transform());
    assert!(!vector_selection_instances(&doc).is_empty());
    assert!(
        doc.transform_handles().is_none(),
        "the item holds the frame"
    );

    doc.clear_vector_selection();
    assert!(vector_selection_instances(&doc).is_empty());
    assert!(
        doc.transform_handles().is_some(),
        "dropping the selection hands it back"
    );
}

/// A box with no rotate handle draws four edges and four corners and nothing else — the case
/// the item frame is built from.
#[test]
fn a_box_with_no_rotate_handle_draws_only_its_own_corners() {
    let corners = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
    let out = calumma_render::compose::box_overlay_instances(corners, None);
    assert_eq!(out.len(), 4 + 4 * 2);
    for (i, corner) in corners.iter().enumerate() {
        // Border and grip, both degenerate segments on the corner itself.
        for dot in [out[4 + i * 2].segment, out[5 + i * 2].segment] {
            assert_eq!((dot[0], dot[1]), (corner.0, corner.1));
            assert_eq!((dot[2], dot[3]), (corner.0, corner.1));
        }
    }
}
