use calumma_core::selection_mask::SelectionMask;

/// A wand click that reaches nothing must leave *no* selection, not an empty one. An empty
/// selection would still be a selection, and every paint tool that clips to it would silently
/// stop drawing.
#[test]
fn a_mask_with_nothing_set_finishes_to_nothing() {
    assert!(SelectionMask::new((0, 0), 32, 32).finish().is_none());
}

#[test]
fn finishing_crops_to_what_was_actually_reached() {
    let mut mask = SelectionMask::new((0, 0), 256, 256);
    mask.set(100, 40);
    mask.set(103, 42);
    let bounds = mask.finish().unwrap().bounds();
    assert_eq!((bounds.min_x, bounds.min_y), (100, 40));
    assert_eq!(
        (bounds.max_x, bounds.max_y),
        (103, 42),
        "the flood scope was 256 wide; the crop is tight to the two pixels set"
    );
}

/// Bits are packed eight to a byte, so a run that straddles a byte boundary is the case where
/// an off-by-one in the shift shows up.
#[test]
fn bits_survive_a_byte_boundary() {
    let mut mask = SelectionMask::new((0, 0), 32, 2);
    for x in 6..11 {
        mask.set(x, 1);
    }
    for x in 0..32 {
        assert_eq!(
            mask.get(x, 1),
            (6..11).contains(&x),
            "x={x} straddles the byte at bit 8"
        );
        assert!(!mask.get(x, 0), "the other row is untouched");
    }
}

#[test]
fn a_mask_can_sit_at_a_negative_origin() {
    let mut mask = SelectionMask::new((-20, -10), 16, 8);
    mask.set(-20, -10);
    mask.set(-5, -3);
    assert!(mask.get(-20, -10));
    assert!(mask.get(-5, -3));
    assert!(!mask.get(-21, -10), "one past the left edge is out");
    assert!(!mask.get(-4, -3), "one past the right edge is out");
}

/// `set` outside the scope is dropped rather than wrapping into a neighbouring row, and `get`
/// answers false for anything the bitmap never covered — including coordinates far enough out
/// to overflow the local offset.
#[test]
fn writes_and_reads_outside_the_scope_are_refused_not_wrapped() {
    let mut mask = SelectionMask::new((10, 10), 8, 8);
    mask.set(9, 10);
    mask.set(18, 10);
    mask.set(10, 9);
    mask.set(10, 18);
    assert!(
        mask.clone().finish().is_none(),
        "every write missed the scope, so nothing was recorded"
    );
    assert!(!mask.get(i32::MIN, i32::MIN));
    assert!(!mask.get(i32::MAX, i32::MAX));
}

/// One filled pixel has four boundary edges and no interior — the smallest outline there is.
#[test]
fn a_single_pixel_traces_four_edges() {
    let mut mask = SelectionMask::new((0, 0), 8, 8);
    mask.set(3, 4);
    let finished = mask.finish().unwrap();
    assert_eq!(finished.outline().len(), 4);
    let bounds = finished.bounds();
    assert_eq!((bounds.min_x, bounds.max_x), (3, 3));
}

/// The whole point of merging runs: a straight boundary is one instance however long it is,
/// so a wand over a wide flat region does not emit an edge per pixel.
#[test]
fn a_straight_boundary_merges_into_one_run_per_side() {
    let mut mask = SelectionMask::new((0, 0), 64, 8);
    for x in 0..40 {
        mask.set(x, 2);
    }
    let finished = mask.finish().unwrap();
    assert_eq!(
        finished.outline().len(),
        4,
        "a 40x1 bar is two horizontal runs plus one vertical edge at each end"
    );
    let widest = finished
        .outline()
        .iter()
        .map(|e| (e[2] - e[0]).abs())
        .fold(0.0f32, f32::max);
    assert_eq!(widest, 40.0, "the long side is one segment, not forty");
}

#[test]
fn a_finished_mask_reports_the_bytes_it_holds() {
    let mut mask = SelectionMask::new((0, 0), 64, 64);
    for x in 0..64 {
        mask.set(x, 0);
    }
    let finished = mask.finish().unwrap();
    assert!(
        finished.memory_bytes() >= 8,
        "64 bits of a one-row crop is eight bytes before the outline is counted"
    );
}
