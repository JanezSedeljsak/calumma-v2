#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerTransform {
    pub offset_x: f32,
    pub offset_y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotation: f32,
}

impl Default for LayerTransform {
    fn default() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
        }
    }
}

const MIN_SCALE: f32 = 0.02;
const MAX_SCALE: f32 = 50.0;

impl LayerTransform {
    pub fn is_identity(&self) -> bool {
        self.offset_x == 0.0
            && self.offset_y == 0.0
            && self.scale_x == 1.0
            && self.scale_y == 1.0
            && self.rotation == 0.0
    }

    pub fn clamped(self) -> Self {
        Self {
            offset_x: self.offset_x,
            offset_y: self.offset_y,
            scale_x: self.scale_x.clamp(MIN_SCALE, MAX_SCALE),
            scale_y: self.scale_y.clamp(MIN_SCALE, MAX_SCALE),
            rotation: self.rotation,
        }
    }

    pub fn forward(&self, pivot: (f32, f32), p: (f32, f32)) -> (f32, f32) {
        let dx = (p.0 - pivot.0) * self.scale_x;
        let dy = (p.1 - pivot.1) * self.scale_y;
        let (sin, cos) = self.rotation.sin_cos();
        let rx = dx * cos - dy * sin;
        let ry = dx * sin + dy * cos;
        (pivot.0 + rx + self.offset_x, pivot.1 + ry + self.offset_y)
    }

    pub fn inverse(&self, pivot: (f32, f32), p: (f32, f32)) -> (f32, f32) {
        let qx = p.0 - self.offset_x - pivot.0;
        let qy = p.1 - self.offset_y - pivot.1;
        let (sin, cos) = (-self.rotation).sin_cos();
        let rx = qx * cos - qy * sin;
        let ry = qx * sin + qy * cos;
        let sx = if self.scale_x.abs() > 1e-6 {
            self.scale_x
        } else {
            1e-6
        };
        let sy = if self.scale_y.abs() > 1e-6 {
            self.scale_y
        } else {
            1e-6
        };
        (pivot.0 + rx / sx, pivot.1 + ry / sy)
    }

    pub fn to_local(&self, pivot: (f32, f32), p: (f32, f32)) -> (f32, f32) {
        let dx = p.0 - pivot.0;
        let dy = p.1 - pivot.1;
        let (sin, cos) = (-self.rotation).sin_cos();
        (dx * cos - dy * sin, dx * sin + dy * cos)
    }

    pub fn transformed_corners(
        &self,
        pivot: (f32, f32),
        bounds: (f32, f32, f32, f32),
    ) -> [(f32, f32); 4] {
        let (x0, y0, x1, y1) = bounds;
        [
            self.forward(pivot, (x0, y0)),
            self.forward(pivot, (x1, y0)),
            self.forward(pivot, (x1, y1)),
            self.forward(pivot, (x0, y1)),
        ]
    }
}

pub fn bounds_center(bounds: (f32, f32, f32, f32)) -> (f32, f32) {
    ((bounds.0 + bounds.2) * 0.5, (bounds.1 + bounds.3) * 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
