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
        },
        color: [255, 0, 0, 255],
    })
}

fn path_item(points: Vec<(f32, f32)>, closed: bool, fill: bool) -> VectorItem {
    VectorItem::Path(VectorPath {
        points,
        closed,
        fill,
        color: [0, 0, 255, 255],
        stroke_width: 4.0,
    })
}

fn layer_with(items: Vec<VectorItem>, transform: Option<LayerTransform>) -> Layer {
    let mut layer = Layer::vector("V", items);
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
    assert_eq!(out[0].radius, 2.0, "radius is half the stroke width");
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
    let items = vec![
        shape_item((10.0, 10.0), (40.0, 40.0)),
        path_item(vec![(0.0, 0.0), (10.0, 0.0)], false, false),
    ];
    let layer = layer_with(
        items.clone(),
        Some(LayerTransform {
            offset_x: 100.0,
            offset_y: 0.0,
            ..LayerTransform::default()
        }),
    );
    let placement = vector_placement(&layer);
    assert!(placement.is_some(), "a moved layer needs a placement");

    let VectorItem::Shape(shape) = &items[0] else {
        unreachable!()
    };
    assert_eq!(shape_instance(shape, placement).p0, [110.0, 10.0]);

    let mut out = Vec::new();
    let VectorItem::Path(path) = &items[1] else {
        unreachable!()
    };
    push_path_instances(path, placement, &mut out);
    assert_eq!(out[0].segment, [100.0, 0.0, 110.0, 0.0]);
}

#[test]
fn an_untransformed_layer_needs_no_placement() {
    let layer = layer_with(vec![shape_item((10.0, 10.0), (40.0, 40.0))], None);
    assert!(vector_placement(&layer).is_none());
}

#[test]
fn a_scaled_layer_scales_the_stroke_with_the_geometry() {
    let items = vec![path_item(vec![(0.0, 0.0), (10.0, 0.0)], false, false)];
    let layer = layer_with(
        items.clone(),
        Some(LayerTransform {
            scale_x: 2.0,
            scale_y: 2.0,
            ..LayerTransform::default()
        }),
    );
    let mut out = Vec::new();
    let VectorItem::Path(path) = &items[0] else {
        unreachable!()
    };
    push_path_instances(path, vector_placement(&layer), &mut out);
    assert_eq!(out[0].radius, 4.0);
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
        vec![item.clone()],
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
        vec![],
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
        },
        color: [1, 2, 3, 255],
    };
    assert_eq!(shape_instance(&shape, None).fill, 0.0);
}

#[test]
fn vector_selection_instances_draws_four_edges_and_four_corner_dots() {
    let mut doc = Document::new("p".into(), "t", 200, 200);
    doc.resize_viewport(200.0, 200.0, 1.0);
    doc.fit_to_view();
    let index = doc.add_vector_layer("V");
    *doc.layers[index].content.items_mut().unwrap() = vec![shape_item((10.0, 10.0), (40.0, 40.0))];
    doc.set_active_layer(index);
    assert!(doc.select_vector_item_at(20.0, 20.0));

    let out = vector_selection_instances(&doc);
    assert_eq!(out.len(), 8, "four edges plus four corner dots");
    for dot in &out[4..] {
        assert_eq!(
            (dot.segment[0], dot.segment[1]),
            (dot.segment[2], dot.segment[3]),
            "a corner dot is a degenerate segment"
        );
    }
}
