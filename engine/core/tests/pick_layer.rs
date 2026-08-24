use calumma_core::document::*;
use calumma_core::*;

const DOC: u32 = 200;

fn doc_with_viewport() -> Document {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.resize_viewport(DOC as f32, DOC as f32, 1.0);
    doc.fit_to_view();
    doc
}

fn paint(doc: &mut Document, index: usize, rect: DocRect, rgba: [u8; 4]) {
    doc.layers[index]
        .tiles_mut()
        .unwrap()
        .paint_rect(rect, |_, _, _| Some(rgba));
}

fn click(doc: &mut Document, x: f32, y: f32) {
    let (sx, sy) = doc.camera.to_screen(x, y);
    doc.pointer_down(sx, sy);
}

#[test]
fn layer_at_finds_the_layer_holding_the_pixel() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(10, 10, 40, 40), [255, 0, 0, 255]);
    assert_eq!(doc.layer_at(20.0, 20.0), Some(1));
    assert_eq!(doc.layer_at(80.0, 80.0), None);
}

#[test]
fn layer_at_prefers_the_topmost_of_two_overlapping_layers() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(10, 10, 60, 60), [255, 0, 0, 255]);
    doc.add_layer("above");
    let top = doc.active_layer;
    paint(
        &mut doc,
        top,
        DocRect::new(30, 30, 90, 90),
        [0, 0, 255, 255],
    );
    assert_eq!(doc.layer_at(45.0, 45.0), Some(top));
    assert_eq!(doc.layer_at(15.0, 15.0), Some(1));
    assert_eq!(doc.layer_at(80.0, 80.0), Some(top));
}

#[test]
fn layer_at_never_returns_paper() {
    let doc = doc_with_viewport();
    assert!(doc.layers[0].is_paper());
    assert_ne!(doc.layers[0].tiles().unwrap().get_pixel(20, 20)[3], 0);
    assert_eq!(doc.layer_at(20.0, 20.0), None);
}

#[test]
fn layer_at_finds_a_visible_vector_layer() {
    let mut doc = doc_with_viewport();
    let layer = doc.add_vector_layer("SVG");
    *doc.layers[layer].content.items_mut().unwrap() = vec![VectorItem::Shape(VectorShape {
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
    })];
    assert_eq!(doc.layer_at(30.0, 30.0), Some(layer));
}

#[test]
fn layer_at_skips_hidden_layers() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(10, 10, 40, 40), [255, 0, 0, 255]);
    doc.set_layer_visible(1, false);
    assert_eq!(doc.layer_at(20.0, 20.0), None);
}

#[test]
fn layer_at_is_pixel_accurate_not_tile_accurate() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(10, 10, 20, 20), [255, 0, 0, 255]);
    let bounds = doc.layers[1].content_bounds().expect("content bounds");
    assert!(bounds.2 - bounds.0 >= 200.0, "bounds are tile-granular");
    assert_eq!(doc.layer_at(15.0, 15.0), Some(1));
    assert_eq!(doc.layer_at(120.0, 120.0), None);
}

#[test]
fn layer_at_respects_a_mask() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(10, 10, 40, 40), [255, 0, 0, 255]);
    let mut mask = vec![255u8; (DOC as usize) * (DOC as usize)];
    mask[(20 * DOC + 20) as usize] = 0;
    doc.layers[1].set_mask(Some(mask));
    assert_eq!(doc.layer_at(20.5, 20.5), None);
    assert_eq!(doc.layer_at(25.5, 25.5), Some(1));
}

#[test]
fn layer_at_treats_zero_opacity_as_unpickable() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(10, 10, 40, 40), [255, 0, 0, 255]);
    doc.set_layer_opacity(1, 0.0);
    assert_eq!(doc.layer_at(20.0, 20.0), None);
    doc.set_layer_opacity(1, 1.0);
    assert_eq!(doc.layer_at(20.0, 20.0), Some(1));
}

#[test]
fn layer_at_follows_a_moved_layer_transform() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(10, 10, 40, 40), [255, 0, 0, 255]);
    doc.layers[1].transform = Some(LayerTransform {
        offset_x: 60.0,
        offset_y: 0.0,
        ..LayerTransform::default()
    });
    assert_eq!(doc.layer_at(20.0, 20.0), None);
    assert_eq!(doc.layer_at(80.0, 20.0), Some(1));
}

#[test]
fn layer_at_rejects_points_outside_the_document() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(10, 10, 40, 40), [255, 0, 0, 255]);
    assert_eq!(doc.layer_at(-1.0, 20.0), None);
    assert_eq!(doc.layer_at(20.0, DOC as f32), None);
}

#[test]
fn pick_layer_sets_the_active_layer() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(10, 10, 40, 40), [255, 0, 0, 255]);
    doc.add_layer("above");
    let top = doc.active_layer;
    assert_eq!(doc.pick_layer(20.0, 20.0), Some(1));
    assert_eq!(doc.active_layer, 1);
    assert_eq!(doc.pick_layer(150.0, 150.0), None);
    assert_eq!(doc.active_layer, 1, "a miss leaves the active layer alone");
    assert_ne!(top, 1);
}

#[test]
fn transform_click_on_another_layer_retargets_instead_of_exiting() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(10, 10, 40, 40), [255, 0, 0, 255]);
    doc.add_layer("above");
    let top = doc.active_layer;
    paint(
        &mut doc,
        top,
        DocRect::new(120, 120, 180, 180),
        [0, 0, 255, 255],
    );
    assert!(doc.enter_transform());
    assert_eq!(doc.transform_handles().map(|h| h.0), Some(top));

    click(&mut doc, 20.0, 20.0);

    assert!(doc.transform_active, "picking keeps the mode alive");
    assert_eq!(doc.active_layer, 1);
    assert_eq!(doc.transform_handles().map(|h| h.0), Some(1));
}

#[test]
fn transform_retarget_starts_a_move_drag_in_the_same_gesture() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(10, 10, 40, 40), [255, 0, 0, 255]);
    doc.add_layer("above");
    let top = doc.active_layer;
    paint(
        &mut doc,
        top,
        DocRect::new(120, 120, 180, 180),
        [0, 0, 255, 255],
    );
    assert!(doc.enter_transform());

    click(&mut doc, 20.0, 20.0);
    let (sx, sy) = doc.camera.to_screen(35.0, 45.0);
    doc.pointer_move(sx, sy);

    let t = doc.layer_transform(1);
    assert!((t.offset_x - 15.0).abs() < 1.0, "offset_x = {}", t.offset_x);
    assert!((t.offset_y - 25.0).abs() < 1.0, "offset_y = {}", t.offset_y);
    assert_eq!(doc.layer_transform(top), LayerTransform::default());
}

#[test]
fn transform_click_outside_the_box_exits_even_with_other_layers_around() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(10, 10, 40, 40), [255, 0, 0, 255]);
    doc.add_layer("above");
    let top = doc.active_layer;
    paint(
        &mut doc,
        top,
        DocRect::new(120, 120, 180, 180),
        [0, 0, 255, 255],
    );
    assert!(doc.enter_transform());
    click(&mut doc, -40.0, -40.0);
    assert!(!doc.transform_active);
    assert_eq!(doc.active_layer, top);
}

/// Retargeting must not cost the pre-existing behaviour it shares a code path with:
/// an empty spot inside the (tile-granular) box with nothing else under it is still a
/// Move drag, not an exit.
#[test]
fn transform_click_on_empty_space_inside_the_box_still_moves() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(10, 10, 40, 40), [255, 0, 0, 255]);
    assert!(doc.enter_transform());
    click(&mut doc, 150.0, 150.0);
    assert!(doc.transform_active);
    assert_eq!(doc.active_layer, 1);
    let (sx, sy) = doc.camera.to_screen(160.0, 150.0);
    doc.pointer_move(sx, sy);
    assert!((doc.layer_transform(1).offset_x - 10.0).abs() < 1.0);
}

#[test]
fn transform_keeps_the_active_layer_when_clicking_its_own_pixels() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(10, 10, 90, 90), [255, 0, 0, 255]);
    doc.add_layer("above");
    let top = doc.active_layer;
    paint(
        &mut doc,
        top,
        DocRect::new(10, 10, 90, 90),
        [0, 0, 255, 255],
    );
    doc.set_active_layer(1);
    assert!(doc.enter_transform());
    click(&mut doc, 50.0, 50.0);
    assert_eq!(
        doc.active_layer, 1,
        "an overlapping layer above must not steal the transform target"
    );
    assert!(doc.transform_active);
}

#[test]
fn picking_only_happens_in_transform_mode() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(10, 10, 40, 40), [255, 0, 0, 255]);
    doc.add_layer("above");
    let top = doc.active_layer;
    doc.tool = Tool::Pen;
    click(&mut doc, 20.0, 20.0);
    doc.pointer_up(0.0, 0.0);
    assert_eq!(
        doc.active_layer, top,
        "a plain pen click paints, never picks"
    );
}
