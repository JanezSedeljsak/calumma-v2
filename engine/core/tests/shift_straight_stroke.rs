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
    doc.layers[doc.active_layer]
        .tiles_mut()
        .unwrap()
        .set_pixel(30, 30, [1, 2, 3, 255]);
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

/// `stroke_generation` promises the renderer that, while it holds, `stroke_points` is an
/// append-only extension of what it was — that is what lets a live brush stroke union only the
/// new segments onto GPU coverage it already accumulated. A straight segment rewinds the tail
/// on every event, and `Max` blending cannot take a capsule back out of the coverage target, so
/// a rewind has to read as a different stroke.
#[test]
fn rewinding_a_straight_segment_bumps_the_stroke_generation() {
    let mut doc = doc_with_tool(Tool::Pen);
    doc.set_shift_held(true);
    down_at(&mut doc, (20.0, 20.0));
    let anchored = doc.stroke_generation();

    move_to(&mut doc, (40.0, 60.0));
    assert_eq!(
        doc.stroke_generation(),
        anchored,
        "the first straight point only extends the list, so nothing was thrown away"
    );

    move_to(&mut doc, (60.0, 30.0));
    let after = doc.stroke_generation();
    assert_ne!(
        after, anchored,
        "swinging the tip drops the point it replaces"
    );

    move_to(&mut doc, (100.0, 100.0));
    assert_ne!(
        doc.stroke_generation(),
        after,
        "and again on the next swing"
    );
}

/// The other half of the same contract: a freehand stroke only ever pushes, so its generation
/// has to stay put for the whole gesture or the renderer would restart every frame and the
/// append path would never run at all.
#[test]
fn a_freehand_stroke_keeps_one_generation_for_the_whole_gesture() {
    let mut doc = doc_with_tool(Tool::Pen);
    down_at(&mut doc, (20.0, 20.0));
    let generation = doc.stroke_generation();

    for step in 1..8 {
        move_to(
            &mut doc,
            (20.0 + step as f32 * 12.0, 20.0 + step as f32 * 7.0),
        );
    }

    assert_eq!(doc.stroke_points.len(), 8);
    assert_eq!(doc.stroke_generation(), generation);
}

/// Releasing Shift mid-stroke hands the rest of the line back to freehand, which is append-only
/// again — so the generation settles rather than bumping once per event forever.
#[test]
fn releasing_shift_returns_the_stroke_to_append_only() {
    let mut doc = doc_with_tool(Tool::Pen);
    doc.set_shift_held(true);
    down_at(&mut doc, (20.0, 20.0));
    move_to(&mut doc, (40.0, 60.0));
    move_to(&mut doc, (60.0, 30.0));

    doc.set_shift_held(false);
    move_to(&mut doc, (80.0, 40.0));
    let settled = doc.stroke_generation();
    move_to(&mut doc, (100.0, 50.0));

    assert_eq!(doc.stroke_generation(), settled);
}
