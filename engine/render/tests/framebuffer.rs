use calumma_render::framebuffer::{exposed_rects, shift_plan, PxRect};

fn area(r: PxRect) -> u64 {
    r.2 as u64 * r.3 as u64
}

fn contains(outer: PxRect, inner: PxRect) -> bool {
    inner.0 >= outer.0
        && inner.1 >= outer.1
        && inner.0 + inner.2 <= outer.0 + outer.2
        && inner.1 + inner.3 <= outer.1 + outer.3
}

#[test]
fn a_pure_pan_shift_keeps_the_full_scissor_size() {
    let scissor: PxRect = (0, 0, 800, 600);
    let (src, dst) = shift_plan(scissor, scissor, 10, 0, 800, 600).expect("overlap");
    assert_eq!((src.2, src.3), (790, 600));
    assert_eq!(dst, (10, 0, 790, 600));
    assert!(contains(scissor, dst));
}

#[test]
fn exposed_rects_partition_the_scissor_around_the_shifted_dest() {
    let scissor: PxRect = (0, 0, 800, 600);
    let dst: PxRect = (10, 0, 790, 600);
    let strips = exposed_rects(scissor, dst);
    assert_eq!(strips, [None, None, Some((0, 0, 10, 600)), None]);
}

#[test]
fn exposed_rects_cover_exactly_outer_minus_inner_with_no_overlap() {
    let outer: PxRect = (0, 0, 400, 300);
    let inner: PxRect = (12, 7, 350, 260);
    let strips = exposed_rects(outer, inner);
    let covered: u64 = strips.iter().flatten().map(|r| area(*r)).sum();
    assert_eq!(covered + area(inner), area(outer));
    for strip in strips.into_iter().flatten() {
        assert!(contains(outer, strip));
        assert!(strip.2 > 0 && strip.3 > 0);
    }
}

#[test]
fn a_diagonal_shift_exposes_the_trailing_edges() {
    // Content sliding right (dx > 0) and up (dy < 0) exposes the edges it slid away from:
    // the left (nothing slid in behind it) and the bottom (nothing slid up into it).
    let outer: PxRect = (0, 0, 200, 100);
    let (src, dst) = shift_plan(outer, outer, 5, -3, 200, 100).expect("overlap");
    assert_eq!(area(src), area(dst));
    let strips = exposed_rects(outer, dst);
    assert!(strips[0].is_none(), "top not exposed when dy is negative");
    assert!(strips[1].is_some(), "bottom exposed when dy is negative");
    assert!(strips[2].is_some(), "left exposed when dx is positive");
    assert!(strips[3].is_none());
}

#[test]
fn a_shift_larger_than_the_scissor_has_no_overlap() {
    let outer: PxRect = (0, 0, 100, 100);
    assert!(shift_plan(outer, outer, 500, 0, 100, 100).is_none());
}

#[test]
fn a_zero_shift_covers_the_whole_scissor_with_no_exposed_strips() {
    let outer: PxRect = (0, 0, 640, 480);
    let (src, dst) = shift_plan(outer, outer, 0, 0, 640, 480).expect("overlap");
    assert_eq!(src, outer);
    assert_eq!(dst, outer);
    assert_eq!(exposed_rects(outer, dst), [None, None, None, None]);
}

#[test]
fn the_source_never_reads_outside_the_reference_rect() {
    let reference: PxRect = (0, 0, 200, 200);
    let current: PxRect = (0, 0, 400, 400);
    let (src, dst) = shift_plan(reference, current, 0, 0, 400, 400).expect("overlap");
    assert!(contains(reference, src));
    assert_eq!(area(src), area(dst));
}
