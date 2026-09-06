//! `offset_layers` and the selection helpers that feed it — each early-return guard
//! (missing index, paper/locked, no content) checked directly, since `move.rs`'s drag tests
//! only ever exercise the single-layer happy path through the pointer pipeline.

use calumma_core::*;

const DOC: u32 = 200;

fn doc() -> Document {
    Document::new("p".into(), "t", DOC, DOC)
}

fn paint(doc: &mut Document, index: usize, rect: DocRect, rgba: [u8; 4]) {
    doc.layers[index]
        .tiles_mut()
        .unwrap()
        .paint_rect(rect, |_, _, _| Some(rgba));
}

#[test]
fn offset_layers_moves_only_painted_unlocked_raster_layers() {
    let mut doc = doc();
    doc.add_layer("A");
    doc.add_layer("B");
    doc.add_layer("C");
    let a = 1;
    let b = 2;
    let c = 3;
    paint(&mut doc, a, DocRect::new(10, 10, 40, 40), [255, 0, 0, 255]);
    paint(&mut doc, b, DocRect::new(10, 10, 40, 40), [0, 255, 0, 255]);
    doc.set_layer_locked(b, true);
    // `c` is left empty on purpose: no content bounds.

    let moved = doc.offset_layers(&[a, b, c, 99], 5.0, 7.0);

    assert!(moved, "at least one layer actually moved");
    assert!(doc.layers[a].transform.is_some());
    assert_eq!(doc.layers[a].transform.unwrap().offset_x, 5.0);
    assert!(
        doc.layers[b].transform.is_none(),
        "locked layers do not move"
    );
    assert!(
        doc.layers[c].transform.is_none(),
        "an empty layer has nothing to offset"
    );
}

#[test]
fn offset_layers_reports_false_when_nothing_moves() {
    let mut doc = doc();
    // Layer 0 is the paper layer: never movable.
    assert!(!doc.offset_layers(&[0], 3.0, 3.0));
    assert!(!doc.offset_layers(&[42], 3.0, 3.0), "index out of range");
}
