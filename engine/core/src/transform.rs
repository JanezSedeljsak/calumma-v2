use crate::limits::{MAX_SCALE, MIN_SCALE};

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
