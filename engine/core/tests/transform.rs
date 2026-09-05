use calumma_core::limits::*;
use calumma_core::transform::*;
use std::f32::consts::PI;

fn approx(a: (f32, f32), b: (f32, f32)) {
    assert!((a.0 - b.0).abs() < 1e-3, "{a:?} != {b:?}");
    assert!((a.1 - b.1).abs() < 1e-3, "{a:?} != {b:?}");
}

#[test]
fn identity_is_a_no_op() {
    let t = LayerTransform::default();
    assert!(t.is_identity());
    approx(t.forward((5.0, 5.0), (10.0, 20.0)), (10.0, 20.0));
}

#[test]
fn forward_and_inverse_round_trip() {
    let t = LayerTransform {
        offset_x: 12.0,
        offset_y: -4.0,
        scale_x: 1.5,
        scale_y: 0.75,
        rotation: 0.6,
    };
    let pivot = (32.0, 32.0);
    let p = (50.0, 10.0);
    let out = t.forward(pivot, p);
    approx(t.inverse(pivot, out), p);
}

#[test]
fn scale_doubles_distance_from_pivot() {
    let t = LayerTransform {
        scale_x: 2.0,
        scale_y: 2.0,
        ..LayerTransform::default()
    };
    let pivot = (0.0, 0.0);
    approx(t.forward(pivot, (10.0, 0.0)), (20.0, 0.0));
}

#[test]
fn rotation_of_quarter_turn_maps_x_axis_to_y_axis() {
    let t = LayerTransform {
        rotation: PI / 2.0,
        ..LayerTransform::default()
    };
    let pivot = (0.0, 0.0);
    approx(t.forward(pivot, (10.0, 0.0)), (0.0, 10.0));
}

#[test]
fn clamped_keeps_scale_away_from_zero_and_absurdly_large() {
    let t = LayerTransform {
        scale_x: 0.0,
        scale_y: 999.0,
        ..LayerTransform::default()
    }
    .clamped();
    assert_eq!(t.scale_x, MIN_SCALE);
    assert_eq!(t.scale_y, MAX_SCALE);
}

#[test]
fn transformed_aabb_is_the_axis_aligned_hull_of_the_corners() {
    let t = LayerTransform {
        offset_x: 10.0,
        offset_y: 0.0,
        scale_x: 2.0,
        scale_y: 1.0,
        rotation: 0.0,
    };
    let (x0, y0, x1, y1) = t.transformed_aabb((0.0, 0.0, 10.0, 10.0));
    assert!((x0 - 5.0).abs() < 1e-3, "{x0}");
    assert!((x1 - 25.0).abs() < 1e-3, "{x1}");
    assert!((y0 - 0.0).abs() < 1e-3, "{y0}");
    assert!((y1 - 10.0).abs() < 1e-3, "{y1}");
}

#[test]
fn identity_transform_leaves_the_aabb_alone() {
    assert_eq!(
        transformed_aabb((1.0, 2.0, 3.0, 4.0), Some(LayerTransform::default())),
        (1.0, 2.0, 3.0, 4.0)
    );
    assert_eq!(
        transformed_aabb((1.0, 2.0, 3.0, 4.0), None),
        (1.0, 2.0, 3.0, 4.0)
    );
}

#[test]
fn clipped_pixel_span_pads_one_pixel_and_stays_inside_the_buffer() {
    let span = clipped_pixel_span((10.2, 20.0, 30.0, 40.8), 100, 80).unwrap();
    assert_eq!(span, (10, 20, 31, 42));
    assert!(clipped_pixel_span((-8.0, -8.0, -1.0, -1.0), 64, 64).is_none());
    let edge = clipped_pixel_span((60.0, 60.0, 90.0, 90.0), 64, 64).unwrap();
    assert_eq!(edge, (60, 60, 64, 64));
}

/// `inverse` and `inverse_delta` divide by the scale, so a transform whose scale has been
/// driven to zero has to be caught before the division rather than returning infinities that
/// would then propagate into a pick, a drag, or a flatten span.
#[test]
fn a_degenerate_scale_does_not_divide_by_zero() {
    let t = LayerTransform {
        scale_x: 0.0,
        scale_y: 0.0,
        ..LayerTransform::default()
    };
    let out = t.inverse((10.0, 10.0), (50.0, 70.0));
    assert!(out.0.is_finite() && out.1.is_finite(), "{out:?}");

    let delta = t.inverse_delta((3.0, -4.0));
    assert!(delta.0.is_finite() && delta.1.is_finite(), "{delta:?}");
}

/// A *delta* carries the rotation and scale of `inverse` but must not pick up its translation
/// — that is what keeps an item under the pointer when it is dragged inside a moved layer.
#[test]
fn inverse_delta_ignores_the_offset_that_inverse_applies() {
    let t = LayerTransform {
        offset_x: 100.0,
        offset_y: -60.0,
        scale_x: 2.0,
        scale_y: 2.0,
        rotation: 0.0,
    };
    approx(t.inverse_delta((10.0, 20.0)), (5.0, 10.0));

    let pivot = (0.0, 0.0);
    let moved = (
        t.inverse(pivot, (10.0, 20.0)).0 - t.inverse(pivot, (0.0, 0.0)).0,
        t.inverse(pivot, (10.0, 20.0)).1 - t.inverse(pivot, (0.0, 0.0)).1,
    );
    approx(moved, t.inverse_delta((10.0, 20.0)));
}

#[test]
fn a_rotation_alone_leaves_the_pivot_where_it_is() {
    let t = LayerTransform {
        rotation: 0.9,
        ..LayerTransform::default()
    };
    let pivot = (17.0, -3.0);
    approx(t.forward(pivot, pivot), pivot);
    approx(t.inverse(pivot, pivot), pivot);
}

/// A rotated box's axis-aligned hull is wider than the box itself — the reason flatten walks
/// `transformed_aabb` rather than the raw bounds.
#[test]
fn a_quarter_turn_swaps_the_aabb_sides() {
    let t = LayerTransform {
        rotation: PI / 2.0,
        ..LayerTransform::default()
    };
    let (x0, y0, x1, y1) = t.transformed_aabb((0.0, 0.0, 40.0, 10.0));
    assert!((x1 - x0 - 10.0).abs() < 1e-3, "width became the old height");
    assert!((y1 - y0 - 40.0).abs() < 1e-3, "height became the old width");
}

#[test]
fn corner_scale_reads_the_ratio_off_the_grabbed_corner() {
    let half = (10.0, 5.0);
    let free = corner_scale(half, (1.0, 1.0), (20.0, 5.0), false);
    approx(free, (2.0, 1.0));

    let locked = corner_scale(half, (1.0, 1.0), (20.0, 10.0), true);
    assert!(
        (locked.0 - locked.1).abs() < 1e-3,
        "proportional locks the two axes together: {locked:?}"
    );
}

/// Every corner asks the same question about its own quadrant, so dragging the top-left out
/// and to the left grows the box exactly as dragging the bottom-right out and to the right.
#[test]
fn every_corner_scales_by_the_same_rule() {
    let half = (10.0, 10.0);
    let signs = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    for s in signs {
        let reach = (20.0 * s.0, 20.0 * s.1);
        approx(corner_scale(half, s, reach, false), (2.0, 2.0));
    }
}

/// A corner dragged onto — or past — the pivot collapses the box to the floor rather than
/// inverting it or reaching zero, which would make the box unrecoverable by dragging back out.
#[test]
fn a_corner_dragged_to_the_pivot_clamps_to_the_floor() {
    let half = (10.0, 10.0);
    for proportional in [true, false] {
        let at_pivot = corner_scale(half, (1.0, 1.0), (0.0, 0.0), proportional);
        assert_eq!(at_pivot, (MIN_SCALE, MIN_SCALE));
    }
}

/// A zero-extent box — a perfectly horizontal line has no height — still has to yield a
/// finite scale, because `half` is guarded before it becomes a divisor.
#[test]
fn a_flat_box_still_produces_a_finite_scale() {
    let scale = corner_scale((25.0, 0.0), (1.0, 1.0), (50.0, 0.0), false);
    assert!(scale.0.is_finite() && scale.1.is_finite(), "{scale:?}");
    assert!((scale.0 - 2.0).abs() < 1e-3, "the real axis still scales");
}

#[test]
fn bounds_center_is_the_midpoint_even_for_a_negative_box() {
    approx(bounds_center((-30.0, -10.0, -10.0, 10.0)), (-20.0, 0.0));
}

/// The whole point of `composed_with_rotation`: for a layer with an *identity* transform, the
/// composed transform's `forward` must draw exactly the same picture as rotating the point
/// straight through, by hand, about the canvas center — the definition Straighten is built on.
#[test]
fn composed_with_rotation_matches_a_direct_rotation_about_the_canvas_center_from_identity() {
    let canvas_center = (100.0, 60.0);
    let pivot = (40.0, 40.0);
    let theta = 0.4;
    let t = LayerTransform::default().composed_with_rotation(canvas_center, pivot, theta);

    for local in [(40.0, 40.0), (10.0, 90.0), (70.0, 5.0), (100.0, 60.0)] {
        let got = t.forward(pivot, local);
        let (sin, cos) = (-theta).sin_cos();
        let rel = (local.0 - canvas_center.0, local.1 - canvas_center.1);
        let want = (
            canvas_center.0 + rel.0 * cos - rel.1 * sin,
            canvas_center.1 + rel.0 * sin + rel.1 * cos,
        );
        approx(got, want);
    }
}

/// The general case: a layer that already has its own offset/rotation/scale (uniform, so the
/// composition is exact) must, after composing, land every point exactly where rotating the
/// *already-transformed* result about the canvas center by hand would put it — i.e.
/// `composed.forward(pivot, p) == rotate(t.forward(pivot, p))`, which is the actual algebraic
/// identity the closed form is derived from.
#[test]
fn composed_with_rotation_matches_rotating_the_pre_transformed_point_when_scale_is_uniform() {
    let canvas_center = (50.0, 50.0);
    let pivot = (20.0, 30.0);
    let t = LayerTransform {
        offset_x: 8.0,
        offset_y: -5.0,
        scale_x: 1.4,
        scale_y: 1.4,
        rotation: 0.9,
    };
    for theta in [0.0, 0.2, -0.5, PI / 2.0, 2.7] {
        let composed = t.composed_with_rotation(canvas_center, pivot, theta);
        for local in [(20.0, 30.0), (0.0, 0.0), (35.0, -12.0), (80.0, 64.0)] {
            let pre = t.forward(pivot, local);
            let (sin, cos) = (-theta).sin_cos();
            let rel = (pre.0 - canvas_center.0, pre.1 - canvas_center.1);
            let want = (
                canvas_center.0 + rel.0 * cos - rel.1 * sin,
                canvas_center.1 + rel.0 * sin + rel.1 * cos,
            );
            approx(composed.forward(pivot, local), want);
        }
    }
}

/// Straighten's whole reason for existing: composing the angle that levels a tilted line, then
/// forwarding a point on that line, must land it exactly horizontal (or vertical — either
/// reference the user could have dragged).
#[test]
fn composing_the_angle_that_levels_a_line_actually_levels_it() {
    let canvas_center = (0.0, 0.0);
    let pivot = (0.0, 0.0);
    let p0: (f32, f32) = (10.0, 10.0);
    let p1: (f32, f32) = (110.0, 34.0);
    let theta = (p1.1 - p0.1).atan2(p1.0 - p0.0);
    let t = LayerTransform::default().composed_with_rotation(canvas_center, pivot, theta);
    let (q0, q1) = (t.forward(pivot, p0), t.forward(pivot, p1));
    assert!((q1.1 - q0.1).abs() < 1e-3, "{q0:?} -> {q1:?} is not level");
}
