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

/// The reference pan advances by the *rounded* shift that was actually blitted, so a long pan
/// made of fractional per-frame deltas never drifts away from where the pixels really are.
/// Advancing it by the raw camera pan instead would bank the rounding remainder every frame.
#[test]
fn chaining_the_reference_by_the_rounded_shift_does_not_accumulate_error() {
    let dpr = 2.0f32;
    let per_frame = 0.37f32;
    let mut camera_pan = 0.0f32;
    let mut reference_pan = 0.0f32;
    for _ in 0..10_000 {
        camera_pan += per_frame;
        let shift = ((camera_pan - reference_pan) * dpr).round() as i32;
        reference_pan += shift as f32 / dpr;
        let residual = (camera_pan - reference_pan).abs();
        assert!(
            residual <= 0.5 / dpr + 1e-3,
            "residual {residual} grew past half a device pixel"
        );
    }
}

/// Why the reference is re-committed every blit frame rather than frozen at the last full
/// redraw: measured from the previous frame the exposed band stays one frame of travel wide,
/// measured from a frozen baseline it grows with the whole gesture until the overlap empties.
#[test]
fn re_committing_the_reference_keeps_exposed_strips_flat_across_a_gesture() {
    let scissor: PxRect = (0, 0, 800, 600);
    let step = 6i32;
    let frames = 60;

    let chained: u64 = (0..frames)
        .map(|_| {
            let (_, dst) = shift_plan(scissor, scissor, step, 0, 800, 600).expect("overlap");
            exposed_rects(scissor, dst)
                .iter()
                .flatten()
                .map(|r| area(*r))
                .sum::<u64>()
        })
        .sum();

    let frozen: u64 = (1..=frames)
        .map(
            |frame| match shift_plan(scissor, scissor, step * frame, 0, 800, 600) {
                Some((_, dst)) => exposed_rects(scissor, dst)
                    .iter()
                    .flatten()
                    .map(|r| area(*r))
                    .sum(),
                None => area(scissor),
            },
        )
        .sum();

    assert_eq!(chained, frames as u64 * step as u64 * 600);
    assert!(
        frozen > chained * 20,
        "frozen baseline should ramp badly: chained {chained}, frozen {frozen}"
    );
}
