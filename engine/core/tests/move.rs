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
