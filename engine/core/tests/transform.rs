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
