use calumma_core::document::*;
use calumma_core::vector::{VectorItem, VectorShape};
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

fn drag(doc: &mut Document, from: (f32, f32), to: (f32, f32)) {
    let down = doc.camera.to_screen(from.0, from.1);
    let up = doc.camera.to_screen(to.0, to.1);
    doc.pointer_down(down.0, down.1);
    doc.pointer_move(up.0, up.1);
    doc.pointer_up(up.0, up.1);
}

#[test]
fn set_tool_remembers_last_shape_and_select() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    assert_eq!(doc.last_shape_tool, Tool::Rect);
    assert_eq!(doc.last_select_tool, Tool::SelectRect);
    doc.set_tool(Tool::Triangle);
    assert_eq!(doc.tool, Tool::Triangle);
    assert_eq!(doc.last_shape_tool, Tool::Triangle);
    doc.set_tool(Tool::Pen);
    assert_eq!(doc.last_shape_tool, Tool::Triangle);
    doc.set_tool(Tool::SelectLasso);
    assert_eq!(doc.last_select_tool, Tool::SelectLasso);
    doc.set_tool(Tool::Move);
    assert_eq!(doc.tool, Tool::Move);
    assert_eq!(doc.last_select_tool, Tool::SelectLasso);
}

#[test]
fn move_tool_does_not_paint_empty_space() {
    let mut doc = doc_with_viewport();
    doc.set_tool(Tool::Move);
    drag(&mut doc, (80.0, 80.0), (90.0, 90.0));
    assert!(doc.shape_drag.is_none());
    assert!(doc.layers[1].transform.is_none() || doc.layers[1].transform.unwrap().is_identity());
}

#[test]
fn move_tool_drags_a_painted_layer() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(20, 20, 60, 60), [255, 0, 0, 255]);
    doc.set_tool(Tool::Move);
    drag(&mut doc, (30.0, 30.0), (50.0, 40.0));
    let t = doc.layers[1].transform.expect("offset");
    assert!((t.offset_x - 20.0).abs() < 0.6, "offset_x {}", t.offset_x);
    assert!((t.offset_y - 10.0).abs() < 0.6, "offset_y {}", t.offset_y);
    assert!(!doc.transform_active);
    assert!(doc.transform_handles().is_none());
}

#[test]
fn move_drag_highlights_the_layer_being_moved() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(40, 40, 80, 80), [255, 0, 0, 255]);
    doc.set_tool(Tool::Move);
    let down = doc.camera.to_screen(50.0, 50.0);
    doc.pointer_down(down.0, down.1);
    assert!(doc.layer_highlight().is_some());
    let up = doc.camera.to_screen(70.0, 60.0);
    doc.pointer_up(up.0, up.1);
    assert!(doc.layer_highlight().is_none());
}

#[test]
fn move_tool_skips_paper() {
    let mut doc = doc_with_viewport();
    doc.set_tool(Tool::Move);
    drag(&mut doc, (20.0, 20.0), (40.0, 40.0));
    assert!(doc.layers[0].is_paper());
    assert!(doc.layers[0].transform.is_none() || doc.layers[0].transform.unwrap().is_identity());
}

#[test]
fn move_tool_drags_a_vector_item() {
    let mut doc = doc_with_viewport();
    let index = doc.add_vector_layer("V");
    *doc.layers[index].content.items_mut().unwrap() = vec![VectorItem::Shape(VectorShape {
        shape: Shape {
            tool: Tool::Rect,
            start: (20.0, 20.0),
            end: (60.0, 60.0),
            half_width: 1.0,
            fill: true,
        },
        color: [255, 0, 0, 255],
    })];
    doc.set_tool(Tool::Move);
    drag(&mut doc, (40.0, 40.0), (55.0, 40.0));
    let item = &doc.layers[index].content.items().unwrap()[0];
    let VectorItem::Shape(shape) = item else {
        panic!("expected shape");
    };
    assert!((shape.shape.start.0 - 35.0).abs() < 1.0);
    assert!(doc.selected_vector_item().is_some());
}

#[test]
fn move_tool_nudges_the_active_layer() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(20, 20, 40, 40), [0, 255, 0, 255]);
    doc.set_tool(Tool::Move);
    assert!(doc.nudge_move_target(3.0, -2.0));
    let t = doc.layers[1].transform.expect("nudge");
    assert!((t.offset_x - 3.0).abs() < f32::EPSILON);
    assert!((t.offset_y + 2.0).abs() < f32::EPSILON);
}

#[test]
fn pen_does_not_nudge_the_layer() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(20, 20, 40, 40), [0, 255, 0, 255]);
    doc.set_tool(Tool::Pen);
    assert!(!doc.nudge_move_target(1.0, 0.0));
}

#[test]
fn move_tool_reaches_a_visible_layer_under_an_invisible_one() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(20, 20, 60, 60), [255, 0, 0, 255]);
    doc.add_layer("Top");
    let top = doc.layers.len() - 1;
    paint(
        &mut doc,
        top,
        DocRect::new(20, 20, 60, 60),
        [0, 255, 0, 255],
    );
    doc.set_layer_visible(top, false);
    doc.set_active_layer(1);
    doc.set_tool(Tool::Move);
    drag(&mut doc, (30.0, 30.0), (50.0, 40.0));
    assert!(doc.layers[1].transform.is_some(), "the visible layer moved");
    assert!(
        doc.layers[top].transform.is_none(),
        "the hidden layer underneath the click was left alone"
    );
}

/// Hiding a layer does not deselect it, so it can stay `active_layer` while invisible. Before
/// the fix, `active_layer_covers` read raw pixel alpha with no `visible` check, so the hidden
/// active layer's own paint under the cursor "won" precedence in `transform_pointer_down` and
/// ate the drag — the layer that actually moved couldn't be seen, so it looked like Move did
/// nothing at all.
#[test]
fn transform_retargets_past_an_invisible_active_layer_to_the_visible_one_below() {
    let mut doc = doc_with_viewport();
    paint(&mut doc, 1, DocRect::new(20, 20, 60, 60), [255, 0, 0, 255]);
    doc.add_layer("Top");
    let top = doc.layers.len() - 1;
    paint(
        &mut doc,
        top,
        DocRect::new(20, 20, 60, 60),
        [0, 255, 0, 255],
    );
    doc.set_layer_visible(top, false);
    doc.set_active_layer(top);
    assert!(doc.enter_transform());

    drag(&mut doc, (30.0, 30.0), (50.0, 40.0));

    assert_eq!(doc.active_layer, 1, "retargeted to the visible layer");
    assert!(doc.layers[1].transform.is_some(), "the visible layer moved");
    assert!(
        doc.layers[top].transform.is_none(),
        "the hidden layer was never touched"
    );
}

/// `⌘T` is a *mode*, not a tool that stays selected: asking for it toggles transform and
/// leaves the previous tool in place, so releasing the mode does not strand the user with no
/// tool at all.
#[test]
fn selecting_transform_toggles_the_mode_and_leaves_the_tool_alone() {
    let mut doc = doc_with_viewport();
    doc.add_layer("Ink");
    let layer = doc.active_layer;
    paint(
        &mut doc,
        layer,
        DocRect::new(20, 20, 60, 60),
        [0, 0, 0, 255],
    );
    doc.set_tool(Tool::Rect);

    assert!(doc.set_tool(Tool::Transform), "entered transform");
    assert!(doc.transform_handles().is_some());
    assert_eq!(doc.tool, Tool::Rect, "the shape tool is still selected");

    assert!(!doc.set_tool(Tool::Transform), "asking again leaves it");
    assert!(doc.transform_handles().is_none());
    assert_eq!(doc.tool, Tool::Rect);
}

#[test]
fn switching_to_any_other_tool_leaves_transform_mode() {
    let mut doc = doc_with_viewport();
    doc.add_layer("Ink");
    let layer = doc.active_layer;
    paint(
        &mut doc,
        layer,
        DocRect::new(20, 20, 60, 60),
        [0, 0, 0, 255],
    );
    assert!(doc.set_tool(Tool::Transform));
    assert!(doc.transform_handles().is_some());

    assert!(doc.set_tool(Tool::Pen));
    assert!(
        doc.transform_handles().is_none(),
        "picking a tool drops the handles"
    );
}

/// Paper is the board's backing sheet and a locked layer is guarded on purpose, so neither
/// answers a Move grab — and an empty layer has no pixels to grab in the first place.
#[test]
fn a_move_grab_is_refused_on_paper_a_lock_and_an_empty_layer() {
    let mut doc = doc_with_viewport();
    doc.set_tool(Tool::Move);
    assert!(
        !doc.begin_move_at(100.0, 100.0),
        "Paper fills the board but never moves"
    );

    doc.add_layer("Empty");
    let empty = doc.active_layer;
    assert!(!doc.begin_move_at(100.0, 100.0), "nothing painted to grab");

    paint(
        &mut doc,
        empty,
        DocRect::new(20, 20, 60, 60),
        [0, 0, 0, 255],
    );
    assert!(doc.begin_move_at(40.0, 40.0));
    doc.end_move_drag();

    doc.set_layer_locked(empty, true);
    assert!(!doc.begin_move_at(40.0, 40.0), "a lock refuses the grab");
}

/// The same three refusals apply to the keyboard path, which does not go through the pointer
/// at all — a nudge that quietly moved a locked layer would be the one way around the lock.
#[test]
fn a_nudge_is_refused_on_paper_a_lock_and_an_empty_layer() {
    let mut doc = doc_with_viewport();
    doc.set_tool(Tool::Move);
    doc.set_active_layer(0);
    assert!(!doc.nudge_move_target(1.0, 0.0), "Paper does not nudge");

    doc.add_layer("Empty");
    let layer = doc.active_layer;
    assert!(
        !doc.nudge_move_target(1.0, 0.0),
        "an unpainted layer has no bounds to move"
    );

    paint(
        &mut doc,
        layer,
        DocRect::new(20, 20, 60, 60),
        [0, 0, 0, 255],
    );
    assert!(doc.nudge_move_target(1.0, 0.0));
    let after = doc.layers[layer].transform.unwrap().offset_x;

    doc.set_layer_locked(layer, true);
    assert!(!doc.nudge_move_target(5.0, 0.0), "a lock refuses the nudge");
    assert_eq!(doc.layers[layer].transform.unwrap().offset_x, after);
}

/// A nudge outside Move and outside `⌘T` does nothing at all — arrow keys belong to whatever
/// else has focus while a paint tool is up.
#[test]
fn a_nudge_outside_move_and_transform_does_nothing() {
    let mut doc = doc_with_viewport();
    doc.add_layer("Ink");
    let layer = doc.active_layer;
    paint(
        &mut doc,
        layer,
        DocRect::new(20, 20, 60, 60),
        [0, 0, 0, 255],
    );

    doc.set_tool(Tool::Pen);
    assert!(!doc.nudge_move_target(1.0, 0.0));
    assert!(doc.layers[layer].transform.is_none());

    assert!(doc.set_tool(Tool::Transform));
    assert!(
        doc.nudge_move_target(1.0, 0.0),
        "transform mode nudges even though the tool is still the pen"
    );
}

/// A selected vector item outranks the layer: the arrow keys move the item, and the layer's
/// own transform is left exactly where it was.
#[test]
fn a_nudge_prefers_the_selected_item_over_its_layer() {
    let mut doc = doc_with_viewport();
    let layer = doc.add_vector_layer("V");
    *doc.layers[layer].content.items_mut().unwrap() = vec![VectorItem::Shape(VectorShape {
        shape: Shape {
            tool: Tool::Rect,
            start: (10.0, 10.0),
            end: (40.0, 40.0),
            half_width: 1.0,
            fill: true,
        },
        color: [255, 0, 0, 255],
    })];
    doc.set_active_layer(layer);
    doc.set_tool(Tool::Move);
    assert!(doc.select_vector_item_at(20.0, 20.0));

    let before = doc.layers[layer].content.items().unwrap()[0]
        .bounds()
        .unwrap();
    assert!(doc.nudge_move_target(3.0, 0.0));
    let after = doc.layers[layer].content.items().unwrap()[0]
        .bounds()
        .unwrap();

    assert!((after.0 - (before.0 + 3.0)).abs() < 0.01);
    assert!(
        doc.layers[layer].transform.is_none(),
        "the layer itself did not move"
    );
}
