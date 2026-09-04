use calumma_core::document::*;
use calumma_core::*;

/// A document with one paintable layer above Paper, split into a black half and a white half —
/// enough texture to tell where a clone actually copied from.
fn edged_board() -> Document {
    let mut doc = Document::new("p".into(), "t", 256, 256);
    doc.resize_viewport(256.0, 256.0, 1.0);
    doc.fit_to_view();
    let tiles = doc.layers[doc.active_layer].tiles_mut().unwrap();
    tiles.fill_uniform(DocRect::new(0, 0, 127, 255), [0, 0, 0, 255]);
    tiles.fill_uniform(DocRect::new(128, 0, 255, 255), [255, 255, 255, 255]);
    doc
}

fn pixel(doc: &Document, x: i32, y: i32) -> [u8; 4] {
    doc.layers[doc.active_layer]
        .tiles()
        .unwrap()
        .get_pixel(x, y)
}

fn drag(doc: &mut Document, from: (f32, f32), to: (f32, f32)) {
    let (sx, sy) = doc.camera.to_screen(from.0, from.1);
    let (ex, ey) = doc.camera.to_screen(to.0, to.1);
    doc.pointer_down(sx, sy);
    doc.pointer_move(ex, ey);
    doc.pointer_up(ex, ey);
}

fn hover(doc: &mut Document, x: f32, y: f32) {
    let (sx, sy) = doc.camera.to_screen(x, y);
    doc.set_pointer_hover(sx, sy);
}

#[test]
fn clone_copies_from_the_fixed_source_offset() {
    let mut doc = edged_board();
    doc.tool = Tool::Clone;
    doc.brush_size = 24.0;
    // Anchor on the black side; the stroke starts on the white side, so the offset between the
    // two — fixed the moment the stroke starts — carries black across for the whole drag.
    doc.set_clone_anchor(40.0, 128.0);

    assert_eq!(
        pixel(&doc, 200, 100),
        [255, 255, 255, 255],
        "white to start"
    );
    drag(&mut doc, (200.0, 60.0), (200.0, 180.0));

    assert_eq!(
        pixel(&doc, 200, 120),
        [0, 0, 0, 255],
        "the black source copied straight across"
    );
}

/// Painting with no source set is a no-op, not a crash — the crosshair simply never appears in
/// this case (see `Document::clone_source_cursor`), and neither should a pixel.
#[test]
fn clone_with_no_source_changes_nothing() {
    let mut doc = edged_board();
    doc.tool = Tool::Clone;
    doc.brush_size = 24.0;

    drag(&mut doc, (128.0, 60.0), (128.0, 180.0));

    assert_eq!(pixel(&doc, 128, 120), [255, 255, 255, 255]);
    assert!(!doc.history.can_undo(), "and left no undo step behind");
}

/// Clone commits as the pointer moves, exactly like blur, so it is exposed to the same
/// one-history-entry-per-event bug. A stroke is one undo, and undoing it puts every stamp back.
#[test]
fn a_whole_clone_stroke_is_one_undo() {
    let mut doc = edged_board();
    doc.tool = Tool::Clone;
    doc.brush_size = 24.0;
    doc.set_clone_anchor(40.0, 40.0);

    let (sx, sy) = doc.camera.to_screen(200.0, 40.0);
    doc.pointer_down(sx, sy);
    for step in 1..=6 {
        let (mx, my) = doc.camera.to_screen(200.0, 40.0 + step as f32 * 30.0);
        doc.pointer_move(mx, my);
    }
    let (ex, ey) = doc.camera.to_screen(200.0, 220.0);
    doc.pointer_up(ex, ey);

    assert_eq!(pixel(&doc, 200, 60), [0, 0, 0, 255], "the start cloned");
    assert_eq!(pixel(&doc, 200, 200), [0, 0, 0, 255], "so did the end");

    doc.undo();
    assert_eq!(pixel(&doc, 200, 60), [255, 255, 255, 255], "start restored");
    assert_eq!(
        pixel(&doc, 200, 200),
        [255, 255, 255, 255],
        "end restored too"
    );
    assert!(!doc.history.can_undo(), "one entry for the whole stroke");
}

/// **Aligned** (the default) carries the offset from a stroke into the next one, so a second
/// stroke keeps reading from wherever the source has moved on to; turning it off snaps the
/// source back to the anchor every time.
#[test]
fn aligned_carries_the_offset_and_unaligned_resets_it() {
    // Anchor deep in black; the first (degenerate, one-tap) stroke fixes the source-minus-
    // destination offset at `anchor − (0, 20)`. A second stroke, tapped at (50, 20), reads
    // from wherever that offset points aligned, or straight from the anchor again unaligned —
    // the two land on opposite colors, which is what tells them apart.
    let anchor = (127.0, 20.0);
    let first_dest = (0.0, 20.0);
    let second_dest = (50.0, 20.0);

    let mut doc = edged_board();
    doc.tool = Tool::Clone;
    doc.brush_size = 16.0;
    assert!(doc.clone_aligned, "on by default");
    doc.set_clone_anchor(anchor.0, anchor.1);

    drag(&mut doc, first_dest, first_dest);
    drag(&mut doc, second_dest, second_dest);
    assert_eq!(
        pixel(&doc, second_dest.0 as i32, second_dest.1 as i32),
        [255, 255, 255, 255],
        "aligned: the offset from the first stroke carries the source past the black/white edge"
    );

    let mut doc = edged_board();
    doc.tool = Tool::Clone;
    doc.brush_size = 16.0;
    doc.set_clone_aligned(false);
    doc.set_clone_anchor(anchor.0, anchor.1);

    drag(&mut doc, first_dest, first_dest);
    drag(&mut doc, second_dest, second_dest);
    assert_eq!(
        pixel(&doc, second_dest.0 as i32, second_dest.1 as i32),
        [0, 0, 0, 255],
        "unaligned: the second stroke re-anchors, and the anchor itself is black"
    );
}

/// Every other paint tool refuses a text layer; the clone stamp is no different.
#[test]
fn clone_refuses_a_text_layer() {
    let mut doc = Document::new("p".into(), "t", 256, 256);
    doc.resize_viewport(256.0, 256.0, 1.0);
    doc.fit_to_view();
    doc.tool = Tool::Text;
    let (sx, sy) = doc.camera.to_screen(60.0, 60.0);
    doc.pointer_down(sx, sy);
    doc.text_insert("hi");
    doc.commit_text();
    let text_index = (0..doc.layers.len())
        .find(|&i| doc.layers[i].is_text())
        .expect("a text layer");
    doc.active_layer = text_index;

    doc.tool = Tool::Clone;
    doc.brush_size = 24.0;
    doc.set_clone_anchor(10.0, 10.0);
    let before = doc.layers[text_index].tiles().unwrap().clone();

    drag(&mut doc, (60.0, 60.0), (90.0, 60.0));

    assert_eq!(
        &before,
        doc.layers[text_index].tiles().unwrap(),
        "a text layer's cached glyph tiles were left alone"
    );
}

/// The bucket and blur both clip to the active selection; clone does the same.
#[test]
fn clone_clips_to_the_active_selection() {
    let mut doc = edged_board();
    doc.selection = Some(Selection {
        shape: SelectionShape::Rect {
            start: (0.0, 0.0),
            end: (256.0, 100.0),
        },
    });
    doc.tool = Tool::Clone;
    doc.brush_size = 40.0;
    doc.set_clone_anchor(20.0, 200.0);

    drag(&mut doc, (128.0, 90.0), (128.0, 90.0));
    drag(&mut doc, (128.0, 200.0), (128.0, 200.0));

    assert_eq!(
        pixel(&doc, 128, 90),
        [0, 0, 0, 255],
        "inside the selection, cloned"
    );
    assert_eq!(
        pixel(&doc, 128, 200),
        [255, 255, 255, 255],
        "outside it, untouched"
    );
}

/// `clone_source_cursor` is the board furniture's own answer to "where does this stroke read
/// from" — every branch is tested here directly rather than only inferred from committed
/// pixels, so a regression in the crosshair logic itself would actually fail something.
mod source_cursor {
    use super::*;

    #[test]
    fn no_crosshair_off_the_clone_and_heal_tools() {
        let mut doc = edged_board();
        doc.set_clone_anchor(40.0, 40.0);
        hover(&mut doc, 40.0, 40.0);

        for tool in [Tool::Pen, Tool::Eraser, Tool::Blur] {
            doc.tool = tool;
            assert_eq!(doc.clone_source_cursor(), None, "{tool:?}");
        }
        for tool in [Tool::Clone, Tool::Heal] {
            doc.tool = tool;
            assert!(doc.clone_source_cursor().is_some(), "{tool:?}");
        }
    }

    #[test]
    fn no_crosshair_until_a_source_is_set() {
        let mut doc = edged_board();
        doc.tool = Tool::Clone;
        hover(&mut doc, 40.0, 40.0);

        assert_eq!(doc.clone_source_cursor(), None);
    }

    /// Before a stroke has fixed an offset, the crosshair sits on the anchor itself — it has
    /// nothing else to go on yet, and it should not chase the pointer around in the meantime.
    #[test]
    fn the_crosshair_sits_on_the_anchor_before_a_stroke_starts() {
        let mut doc = edged_board();
        doc.tool = Tool::Clone;
        doc.set_clone_anchor(40.0, 60.0);
        hover(&mut doc, 200.0, 5.0);

        assert_eq!(doc.clone_source_cursor(), Some((40.0, 60.0)));
    }

    /// Once a stroke has fixed the offset, the crosshair tracks the pointer at that fixed
    /// remove — this is the whole point of the furniture, showing where the *next* stamp
    /// would read from rather than where the last one did.
    #[test]
    fn the_crosshair_tracks_the_pointer_once_a_stroke_fixes_the_offset() {
        let mut doc = edged_board();
        doc.tool = Tool::Clone;
        doc.brush_size = 16.0;
        doc.set_clone_anchor(20.0, 20.0);

        drag(&mut doc, (100.0, 20.0), (100.0, 20.0));
        hover(&mut doc, 150.0, 60.0);

        // offset fixed at (20 - 100, 20 - 20) = (-80, 0)
        assert_eq!(doc.clone_source_cursor(), Some((70.0, 60.0)));
    }

    /// The same three guards `brush_ring` runs before promising a stamp: a source crosshair
    /// the engine would then refuse to paint from is worse than no crosshair at all.
    #[test]
    fn a_layer_that_refuses_paint_hides_the_crosshair_too() {
        let mut doc = edged_board();
        doc.tool = Tool::Clone;
        doc.set_clone_anchor(40.0, 40.0);
        hover(&mut doc, 40.0, 40.0);
        assert!(doc.clone_source_cursor().is_some());

        doc.set_layer_locked(doc.active_layer, true);
        assert_eq!(doc.clone_source_cursor(), None, "locked");

        doc.set_layer_locked(doc.active_layer, false);
        assert!(
            doc.clone_source_cursor().is_some(),
            "unlocking gives it back"
        );
    }

    #[test]
    fn transform_owns_the_pointer_so_the_crosshair_stands_down() {
        let mut doc = edged_board();
        doc.tool = Tool::Clone;
        doc.set_clone_anchor(40.0, 40.0);
        hover(&mut doc, 40.0, 40.0);
        assert!(doc.enter_transform());

        assert_eq!(doc.clone_source_cursor(), None);

        doc.exit_transform();
        assert!(doc.clone_source_cursor().is_some());
    }

    /// Once the offset is tracking the pointer, the crosshair stops exactly where the brush
    /// ring does — off the end of the layer there is nothing for either promise to land on.
    #[test]
    fn no_crosshair_off_the_end_of_the_layer_once_tracking() {
        let mut doc = edged_board();
        doc.tool = Tool::Clone;
        doc.brush_size = 16.0;
        doc.set_clone_anchor(20.0, 20.0);
        drag(&mut doc, (100.0, 20.0), (100.0, 20.0));

        hover(&mut doc, 120.0, 20.0);
        assert!(doc.clone_source_cursor().is_some(), "on the paper");

        let (w, h) = (doc.width as f32, doc.height as f32);
        hover(&mut doc, w + 40.0, h / 2.0);
        assert_eq!(
            doc.clone_source_cursor(),
            None,
            "past the right edge there is not"
        );
    }
}
