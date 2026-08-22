use calumma_core::document::*;
use calumma_core::*;

/// Paper at 0, then three layers named so a reorder is readable in a failure message.
fn stacked() -> Document {
    let mut doc = Document::new("p".into(), "t", 128, 128);
    doc.resize_viewport(128.0, 128.0, 1.0);
    doc.fit_to_view();
    while doc.layers.len() > 1 {
        doc.remove_layer(doc.layers.len() - 1);
    }
    for name in ["A", "B", "C"] {
        doc.add_layer(name);
    }
    doc
}

fn names(doc: &Document) -> Vec<&str> {
    doc.layers.iter().map(|l| l.name.as_str()).collect()
}

#[test]
fn move_layer_puts_it_where_it_was_asked_to_go() {
    let mut doc = stacked();
    assert_eq!(names(&doc), ["Paper", "A", "B", "C"]);

    assert!(doc.move_layer(3, 1), "C down to just above Paper");
    assert_eq!(names(&doc), ["Paper", "C", "A", "B"]);

    assert!(doc.move_layer(1, 3), "and back to the top");
    assert_eq!(names(&doc), ["Paper", "A", "B", "C"]);
}

/// Paper is the board's backing sheet, not part of the composition. Dragging it, or dropping
/// anything under it, would leave paint hidden beneath white and looking like it had vanished.
#[test]
fn paper_stays_pinned_to_the_bottom() {
    let mut doc = stacked();
    assert!(!doc.move_layer(0, 2), "Paper cannot be dragged");
    assert!(!doc.move_layer(3, 0), "and nothing drops beneath it");
    assert_eq!(names(&doc), ["Paper", "A", "B", "C"]);
}

#[test]
fn a_move_that_goes_nowhere_is_refused() {
    let mut doc = stacked();
    assert!(!doc.move_layer(2, 2));
    assert!(!doc.move_layer(9, 1), "out of range");
    assert!(!doc.move_layer(1, 9));
    assert_eq!(names(&doc), ["Paper", "A", "B", "C"]);
}

/// The active layer is an index, so a reorder has to carry it — otherwise the drag silently
/// retargets every later edit at whatever slid into that slot.
#[test]
fn reordering_carries_the_active_layer_with_it() {
    let mut doc = stacked();
    doc.active_layer = 3;
    assert_eq!(doc.layers[doc.active_layer].name, "C");

    doc.move_layer(3, 1);
    assert_eq!(
        doc.layers[doc.active_layer].name, "C",
        "the same layer is still active after it moved"
    );

    doc.active_layer = 3;
    assert_eq!(doc.layers[doc.active_layer].name, "B");
    doc.move_layer(1, 3);
    assert_eq!(
        doc.layers[doc.active_layer].name, "B",
        "and a layer the move only shifted past keeps its identity too"
    );
}

/// The panel draws the stack top-first while the document stores it bottom-first, so the two
/// are mirror images. The engine owns that flip; the shell hands over rows.
#[test]
fn rows_are_the_stack_upside_down() {
    let mut doc = stacked();
    assert_eq!(names(&doc), ["Paper", "A", "B", "C"]);

    assert!(doc.move_layer_row(0, 2), "drag the top row down two");
    assert_eq!(names(&doc), ["Paper", "C", "A", "B"]);

    assert!(doc.move_layer_row(2, 0), "and drag it back to the top");
    assert_eq!(names(&doc), ["Paper", "A", "B", "C"]);
}

#[test]
fn the_bottom_row_is_paper_and_refuses_to_move() {
    let mut doc = stacked();
    assert!(!doc.move_layer_row(3, 0), "Paper is the last row");
    assert!(!doc.move_layer_row(0, 3), "and nothing drops onto its row");
    assert_eq!(names(&doc), ["Paper", "A", "B", "C"]);
}

#[test]
fn renaming_trims_and_refuses_nothing_at_all() {
    let mut doc = stacked();
    assert!(doc.set_layer_name(1, "  Sketch  "));
    assert_eq!(doc.layers[1].name, "Sketch", "trimmed");

    assert!(!doc.set_layer_name(1, "   "), "whitespace is not a name");
    assert!(!doc.set_layer_name(1, ""), "and neither is nothing");
    assert!(!doc.set_layer_name(1, "Sketch"), "nor is a no-op rename");
    assert_eq!(doc.layers[1].name, "Sketch");
}

/// `Layer::is_paper` is name-matched, so the name is load-bearing: merge-down, click-to-pick
/// and the Filters menu all key off it. Both directions have to be refused, or a rename
/// quietly turns an ordinary layer into Paper — or stops Paper being Paper.
#[test]
fn the_paper_name_is_load_bearing_in_both_directions() {
    let mut doc = stacked();
    assert!(
        !doc.set_layer_name(0, "Background"),
        "Paper cannot be renamed"
    );
    assert!(doc.layers[0].is_paper());

    assert!(
        !doc.set_layer_name(2, calumma_core::PAPER),
        "and nothing else can become Paper"
    );
    assert_eq!(doc.layers[2].name, "B");
    assert_eq!(
        doc.layers.iter().filter(|l| l.is_paper()).count(),
        1,
        "exactly one Paper, always"
    );
}

#[test]
fn locking_refuses_paint() {
    let mut doc = stacked();
    doc.active_layer = 1;
    assert!(doc.active_layer_accepts_paint());

    assert!(doc.set_layer_locked(1, true));
    assert!(!doc.active_layer_accepts_paint(), "no paint, fill or clear");

    doc.tool = Tool::Pen;
    doc.set_color([0, 0, 0, 255]);
    doc.brush_size = 20.0;
    let (sx, sy) = doc.camera.to_screen(64.0, 64.0);
    doc.pointer_down(sx, sy);
    doc.pointer_up(sx, sy);
    assert_eq!(
        doc.layers[1].tiles().unwrap().get_pixel(64, 64),
        [0, 0, 0, 0],
        "the stroke landed nowhere"
    );
    assert!(!doc.history.can_undo(), "and left no undo step");

    assert!(doc.set_layer_locked(1, false));
    assert!(doc.active_layer_accepts_paint(), "unlocking gives it back");
}

#[test]
fn locking_refuses_transform_and_move() {
    let mut doc = stacked();
    doc.active_layer = 1;
    {
        let tiles = doc.layers[1].tiles_mut().unwrap();
        tiles.fill_uniform(DocRect::new(20, 20, 80, 80), [10, 20, 30, 255]);
    }
    assert!(doc.enter_transform(), "transformable while unlocked");
    doc.exit_transform();

    doc.set_layer_locked(1, true);
    assert!(!doc.enter_transform(), "not once locked");

    doc.tool = Tool::Move;
    let (sx, sy) = doc.camera.to_screen(50.0, 50.0);
    doc.pointer_down(sx, sy);
    let (ex, ey) = doc.camera.to_screen(90.0, 90.0);
    doc.pointer_move(ex, ey);
    doc.pointer_up(ex, ey);
    assert!(doc.layers[1].transform.is_none(), "and it did not move");

    assert!(
        !doc.nudge_move_target(1.0, 0.0),
        "arrow keys are the same edit by another name"
    );
}

/// Locking a layer while it is mid-transform has to drop the handles, or the box stays on
/// screen inviting a drag the engine will refuse.
#[test]
fn locking_the_active_layer_leaves_transform_mode() {
    let mut doc = stacked();
    doc.active_layer = 1;
    {
        let tiles = doc.layers[1].tiles_mut().unwrap();
        tiles.fill_uniform(DocRect::new(20, 20, 80, 80), [10, 20, 30, 255]);
    }
    assert!(doc.enter_transform());
    assert!(doc.transform_active);

    doc.set_layer_locked(1, true);
    assert!(!doc.transform_active, "the handles went away with the lock");
}

/// The point of the lock is that a stray stroke cannot find the layer. Click-to-pick would be
/// a way around it: click a locked layer's pixels in transform mode and it would become the
/// target anyway.
#[test]
fn a_locked_layer_cannot_be_picked_off_the_board() {
    let mut doc = stacked();
    {
        let tiles = doc.layers[2].tiles_mut().unwrap();
        tiles.fill_uniform(DocRect::new(20, 20, 80, 80), [10, 20, 30, 255]);
    }
    assert_eq!(doc.layer_at(50.0, 50.0), Some(2), "picked while unlocked");

    doc.set_layer_locked(2, true);
    assert_eq!(doc.layer_at(50.0, 50.0), None, "and invisible to a pick");
}

/// A lock guards against the stray stroke, not against the delete button sitting next to it —
/// Photoshop and Figma both let a locked layer be deleted, and so does this.
#[test]
fn locking_still_allows_visibility_and_delete() {
    let mut doc = stacked();
    doc.set_layer_locked(1, true);

    doc.set_layer_visible(1, false);
    assert!(!doc.layers[1].visible, "the eye still works");

    let before = doc.layers.len();
    assert!(doc.remove_layer(1));
    assert_eq!(doc.layers.len(), before - 1, "and so does delete");
}

#[test]
fn setting_a_lock_to_what_it_already_is_reports_no_change() {
    let mut doc = stacked();
    assert!(!doc.layer_locked(1));
    assert!(!doc.set_layer_locked(1, false), "already unlocked");
    assert!(doc.set_layer_locked(1, true));
    assert!(!doc.set_layer_locked(1, true), "already locked");
    assert!(!doc.set_layer_locked(99, true), "out of range");
}

/// Paper is the layer most worth locking — it is the one everybody paints on by accident.
#[test]
fn paper_can_be_locked_even_though_it_cannot_be_renamed() {
    let mut doc = stacked();
    assert!(doc.set_layer_locked(0, true));
    assert!(doc.layer_locked(0));
    doc.active_layer = 0;
    assert!(!doc.active_layer_accepts_paint());
}
