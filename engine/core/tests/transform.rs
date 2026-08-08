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
