use calumma_core::document::*;
use calumma_core::shape::square_end;
use calumma_core::vector::VectorItem;
use calumma_core::*;

const DOC: u32 = 400;

fn doc_with_tool(tool: Tool) -> Document {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.resize_viewport(DOC as f32, DOC as f32, 1.0);
    doc.fit_to_view();
    doc.tool = tool;
    doc
}

fn drag(doc: &mut Document, from: (f32, f32), to: (f32, f32)) {
    let down = doc.camera.to_screen(from.0, from.1);
    let up = doc.camera.to_screen(to.0, to.1);
    doc.pointer_down(down.0, down.1);
    doc.pointer_move(up.0, up.1);
}

fn preview(doc: &Document) -> Shape {
    doc.preview_shape().expect("a drag is in progress")
}

fn close(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01
}

fn span(shape: Shape) -> (f32, f32) {
    (
        (shape.end.0 - shape.start.0).abs(),
        (shape.end.1 - shape.start.1).abs(),
    )
}

#[test]
fn a_constrained_drag_fills_it_rather_than_shrinking_to_the_short_side() {
    assert_eq!(square_end((0.0, 0.0), (30.0, 10.0)), (30.0, 30.0));
    assert_eq!(square_end((0.0, 0.0), (10.0, 40.0)), (40.0, 40.0));
}

#[test]
fn a_constrained_drag_keeps_the_direction_it_was_dragged() {
    assert_eq!(square_end((100.0, 100.0), (70.0, 90.0)), (70.0, 70.0));
    assert_eq!(square_end((100.0, 100.0), (130.0, 90.0)), (130.0, 70.0));
    assert_eq!(square_end((100.0, 100.0), (70.0, 130.0)), (70.0, 130.0));
}

#[test]
fn shift_turns_a_rectangle_into_a_square() {
    let mut doc = doc_with_tool(Tool::Rect);
    doc.set_shift_held(true);
    drag(&mut doc, (50.0, 50.0), (150.0, 90.0));
    let (w, h) = span(preview(&doc));
    assert!((w - h).abs() < 0.01, "square, got {w}x{h}");
    assert!((w - 100.0).abs() < 0.01, "the long side wins");
}

#[test]
fn shift_turns_an_ellipse_into_a_circle() {
    let mut doc = doc_with_tool(Tool::Ellipse);
    doc.set_shift_held(true);
    drag(&mut doc, (50.0, 50.0), (90.0, 200.0));
    let (w, h) = span(preview(&doc));
    assert!((w - h).abs() < 0.01, "circle, got {w}x{h}");
}

#[test]
fn without_shift_a_drag_is_free() {
    let mut doc = doc_with_tool(Tool::Rect);
    drag(&mut doc, (50.0, 50.0), (150.0, 90.0));
    assert!(close(span(preview(&doc)), (100.0, 40.0)));
}

#[test]
fn shift_snaps_a_line_and_an_arrow_to_forty_five_degrees() {
    for tool in [Tool::Line, Tool::Arrow] {
        let mut doc = doc_with_tool(tool);
        doc.set_shift_held(true);
        drag(&mut doc, (50.0, 50.0), (150.0, 60.0));
        let end = preview(&doc).end;
        assert!((end.1 - 50.0).abs() < 0.01, "{tool:?} snaps flat");
        assert!(
            (end.0 - 50.0 - 100f32.hypot(10.0)).abs() < 0.01,
            "{tool:?} keeps its length"
        );

        drag(&mut doc, (50.0, 50.0), (150.0, 140.0));
        let (dx, dy) = (preview(&doc).end.0 - 50.0, preview(&doc).end.1 - 50.0);
        assert!((dx - dy).abs() < 0.01, "{tool:?} snaps to the diagonal");
        assert!(
            (dx.hypot(dy) - 100f32.hypot(90.0)).abs() < 0.01,
            "{tool:?} keeps its length"
        );
    }
}

#[test]
fn shift_makes_a_pentagon_regular_and_a_triangle_equilateral() {
    let mut doc = doc_with_tool(Tool::Pentagon);
    doc.set_shift_held(true);
    drag(&mut doc, (50.0, 50.0), (150.0, 90.0));
    assert!(close(span(preview(&doc)), (100.0, 100.0)));

    let mut doc = doc_with_tool(Tool::Triangle);
    doc.set_shift_held(true);
    drag(&mut doc, (50.0, 50.0), (150.0, 90.0));
    let verts = preview(&doc).triangle_vertices();
    let side = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).hypot(a.1 - b.1);
    let (ab, bc, ca) = (
        side(verts[0], verts[1]),
        side(verts[1], verts[2]),
        side(verts[2], verts[0]),
    );
    assert!(
        (ab - bc).abs() < 0.01 && (bc - ca).abs() < 0.01,
        "{ab} {bc} {ca}"
    );
    assert!((bc - 100.0).abs() < 0.01, "the base fills the drag");
}

#[test]
fn without_shift_the_polygons_and_lines_are_free() {
    for tool in [Tool::Line, Tool::Arrow, Tool::Triangle, Tool::Pentagon] {
        let mut doc = doc_with_tool(tool);
        drag(&mut doc, (50.0, 50.0), (150.0, 90.0));
        assert!(close(span(preview(&doc)), (100.0, 40.0)), "{tool:?}");
    }
}

/// The clamp is derived from the raw drag on every read, so the modifier can be pressed or
/// released without moving the mouse and the board still shows the right shape.
#[test]
fn pressing_and_releasing_shift_mid_drag_needs_no_pointer_event() {
    let mut doc = doc_with_tool(Tool::Rect);
    drag(&mut doc, (50.0, 50.0), (150.0, 90.0));
    assert!(close(span(preview(&doc)), (100.0, 40.0)));

    doc.set_shift_held(true);
    assert!(close(span(preview(&doc)), (100.0, 100.0)));

    doc.set_shift_held(false);
    assert!(close(span(preview(&doc)), (100.0, 40.0)), "and back again");
}

#[test]
fn a_shift_held_only_at_release_still_constrains_what_is_committed() {
    let mut doc = doc_with_tool(Tool::Rect);
    doc.set_vector_mode(true);
    drag(&mut doc, (50.0, 50.0), (150.0, 90.0));
    doc.set_shift_held(true);
    let up = doc.camera.to_screen(150.0, 90.0);
    doc.pointer_up(up.0, up.1);

    let item = doc.layers[doc.active_layer].content.item().unwrap();
    let VectorItem::Shape(committed) = item else {
        unreachable!("vector mode commits a shape")
    };
    assert!(close(committed.shape.start, (50.0, 50.0)));
    assert!(close(committed.shape.end, (150.0, 150.0)));
}

#[test]
fn the_selection_marquees_constrain_the_same_way() {
    let mut doc = doc_with_tool(Tool::SelectRect);
    doc.layers[doc.active_layer]
        .tiles_mut()
        .unwrap()
        .fill_uniform(DocRect::new(40, 40, 199, 199), [1, 2, 3, 255]);
    doc.set_shift_held(true);
    drag(&mut doc, (40.0, 40.0), (140.0, 80.0));
    let (w, h) = span(preview(&doc));
    assert!((w - h).abs() < 0.01, "the marquee squares off too");

    let up = doc.camera.to_screen(140.0, 80.0);
    doc.pointer_up(up.0, up.1);
    let sel = doc
        .selection
        .as_ref()
        .expect("a rect selection was committed");
    assert!(sel.contains(40.5, 40.5));
    assert!(sel.contains(139.5, 139.5));
    assert!(!sel.contains(141.5, 141.5));
    let bounds = sel.bounds();
    assert_eq!(bounds.max_x - bounds.min_x, bounds.max_y - bounds.min_y);
}

#[test]
fn the_transform_polarity_is_untouched_by_this() {
    let mut doc = doc_with_tool(Tool::Rect);
    doc.layers[1]
        .tiles_mut()
        .unwrap()
        .set_pixel(20, 20, [0, 0, 0, 255]);
    doc.set_active_layer(1);
    assert!(doc.enter_transform());
    doc.set_shift_held(true);
    let (_, corners, _) = doc.transform_handles().unwrap();
    let corner = corners[2];
    let down = doc.camera.to_screen(corner.0, corner.1);
    let to = doc.camera.to_screen(corner.0 + 40.0, corner.1 + 10.0);
    doc.pointer_down(down.0, down.1);
    doc.pointer_move(to.0, to.1);
    let t = doc.layers[1].transform.unwrap();
    assert!(
        (t.scale_x - t.scale_y).abs() > 0.01,
        "Shift is still free scale inside transform mode"
    );
}
