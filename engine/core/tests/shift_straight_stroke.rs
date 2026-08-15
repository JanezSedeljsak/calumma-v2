use calumma_core::document::*;
use calumma_core::*;

const DOC: u32 = 400;

fn doc_with_tool(tool: Tool) -> Document {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.resize_viewport(DOC as f32, DOC as f32, 1.0);
    doc.fit_to_view();
    doc.tool = tool;
    doc
}

fn move_to(doc: &mut Document, p: (f32, f32)) {
    let s = doc.camera.to_screen(p.0, p.1);
    doc.pointer_move(s.0, s.1);
}

fn down_at(doc: &mut Document, p: (f32, f32)) {
    let s = doc.camera.to_screen(p.0, p.1);
    doc.pointer_down(s.0, s.1);
}

fn close(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01
}

#[test]
fn shift_collapses_a_wobbly_pen_stroke_to_a_straight_line() {
    let mut doc = doc_with_tool(Tool::Pen);
    doc.set_shift_held(true);
    down_at(&mut doc, (20.0, 20.0));
    move_to(&mut doc, (40.0, 60.0));
    move_to(&mut doc, (60.0, 30.0));
    move_to(&mut doc, (100.0, 100.0));

    assert_eq!(
        doc.stroke_points.len(),
        2,
        "only the anchor and the tip remain"
    );
    assert!(close(
        (doc.stroke_points[0].x, doc.stroke_points[0].y),
        (20.0, 20.0)
    ));
    assert!(close(
        (doc.stroke_points[1].x, doc.stroke_points[1].y),
        (100.0, 100.0)
    ));
}

#[test]
fn without_shift_a_pen_stroke_stays_freehand() {
    let mut doc = doc_with_tool(Tool::Pen);
    down_at(&mut doc, (20.0, 20.0));
    move_to(&mut doc, (40.0, 60.0));
    move_to(&mut doc, (60.0, 30.0));

    assert_eq!(doc.stroke_points.len(), 3, "every wobble point is kept");
}

#[test]
fn holding_shift_only_straightens_the_segment_drawn_while_held() {
    let mut doc = doc_with_tool(Tool::Pen);
    down_at(&mut doc, (20.0, 20.0));
    move_to(&mut doc, (40.0, 60.0));
    move_to(&mut doc, (60.0, 30.0));
    assert_eq!(doc.stroke_points.len(), 3, "freehand so far");

    doc.set_shift_held(true);
    move_to(&mut doc, (200.0, 40.0));
    move_to(&mut doc, (250.0, 44.0));

    assert_eq!(
        doc.stroke_points.len(),
        4,
        "the earlier freehand points survive, only the tip is re-drawn"
    );
    assert!(close(
        (doc.stroke_points[2].x, doc.stroke_points[2].y),
        (60.0, 30.0)
    ));
    assert!(close(
        (doc.stroke_points[3].x, doc.stroke_points[3].y),
        (250.0, 44.0)
    ));
}

#[test]
fn releasing_shift_mid_drag_resumes_freehand_from_where_it_left_off() {
    let mut doc = doc_with_tool(Tool::Pen);
    doc.set_shift_held(true);
    down_at(&mut doc, (20.0, 20.0));
    move_to(&mut doc, (100.0, 20.0));
    assert_eq!(doc.stroke_points.len(), 2);

    doc.set_shift_held(false);
    move_to(&mut doc, (110.0, 60.0));
    move_to(&mut doc, (90.0, 80.0));

    assert_eq!(
        doc.stroke_points.len(),
        4,
        "the straight endpoint is kept and freehand continues from it"
    );
    assert!(close(
        (doc.stroke_points[1].x, doc.stroke_points[1].y),
        (100.0, 20.0)
    ));
}

#[test]
fn the_lasso_is_left_alone_by_shift() {
    let mut doc = doc_with_tool(Tool::SelectLasso);
    doc.set_shift_held(true);
    down_at(&mut doc, (20.0, 20.0));
    move_to(&mut doc, (40.0, 60.0));
    move_to(&mut doc, (60.0, 30.0));

    assert_eq!(
        doc.stroke_points.len(),
        3,
        "lasso stays freehand under Shift"
    );
}

#[test]
fn shift_straightens_the_eraser_too() {
    let mut doc = doc_with_tool(Tool::Eraser);
    doc.set_shift_held(true);
    down_at(&mut doc, (20.0, 20.0));
    move_to(&mut doc, (40.0, 60.0));
    move_to(&mut doc, (100.0, 100.0));

    assert_eq!(doc.stroke_points.len(), 2);
}
