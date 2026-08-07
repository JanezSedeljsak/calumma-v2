#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum Tool {
    #[default]
    Pen = 0,
    Line = 1,
    Rect = 2,
    Ellipse = 3,
    Arrow = 4,
    Eraser = 5,
    SelectRect = 6,
    SelectEllipse = 7,
    SelectLasso = 8,
    Fill = 9,
    Transform = 10,
}

impl Tool {
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            0 => Some(Self::Pen),
            1 => Some(Self::Line),
            2 => Some(Self::Rect),
            3 => Some(Self::Ellipse),
            4 => Some(Self::Arrow),
            5 => Some(Self::Eraser),
            6 => Some(Self::SelectRect),
            7 => Some(Self::SelectEllipse),
            8 => Some(Self::SelectLasso),
            9 => Some(Self::Fill),
            10 => Some(Self::Transform),
            _ => None,
        }
    }

    pub fn is_shape(self) -> bool {
        matches!(self, Tool::Line | Tool::Rect | Tool::Ellipse | Tool::Arrow)
    }

    pub fn is_selection(self) -> bool {
        matches!(
            self,
            Tool::SelectRect | Tool::SelectEllipse | Tool::SelectLasso
        )
    }

    pub fn takes_fill(self) -> bool {
        matches!(self, Tool::Rect | Tool::Ellipse)
    }
}

const BARB_ANGLE: f32 = 0.5;
const HEAD_RATIO: f32 = 6.0;
const MIN_HEAD: f32 = 10.0;
const MAX_HEAD: f32 = 80.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub tool: Tool,
    pub start: (f32, f32),
    pub end: (f32, f32),
    pub half_width: f32,
    pub fill: bool,
}

fn length(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

fn sd_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (pa_x, pa_y) = (p.0 - a.0, p.1 - a.1);
    let (ba_x, ba_y) = (b.0 - a.0, b.1 - a.1);
    let squared = ba_x * ba_x + ba_y * ba_y;
    let h = if squared > 0.0 {
        ((pa_x * ba_x + pa_y * ba_y) / squared).clamp(0.0, 1.0)
    } else {
        0.0
    };
    length(pa_x - ba_x * h, pa_y - ba_y * h)
}

fn sd_box(p: (f32, f32), center: (f32, f32), half: (f32, f32)) -> f32 {
    let dx = (p.0 - center.0).abs() - half.0;
    let dy = (p.1 - center.1).abs() - half.1;
    length(dx.max(0.0), dy.max(0.0)) + dx.max(dy).min(0.0)
}

fn sd_ellipse(p: (f32, f32), center: (f32, f32), radii: (f32, f32)) -> f32 {
    let rx = radii.0.max(f32::MIN_POSITIVE);
    let ry = radii.1.max(f32::MIN_POSITIVE);
    let (dx, dy) = (p.0 - center.0, p.1 - center.1);
    let outer = length(dx / rx, dy / ry);
    let gradient = length(dx / (rx * rx), dy / (ry * ry));
    if gradient <= f32::MIN_POSITIVE {
        return -rx.min(ry);
    }
    (outer - 1.0) * outer / gradient
}

impl Shape {
    pub fn head_len(&self) -> f32 {
        (self.half_width * HEAD_RATIO).clamp(MIN_HEAD, MAX_HEAD)
    }

    fn center(&self) -> (f32, f32) {
        (
            (self.start.0 + self.end.0) * 0.5,
            (self.start.1 + self.end.1) * 0.5,
        )
    }

    fn half_extent(&self) -> (f32, f32) {
        (
            (self.end.0 - self.start.0).abs() * 0.5,
            (self.end.1 - self.start.1).abs() * 0.5,
        )
    }

    fn arrow_distance(&self, p: (f32, f32)) -> f32 {
        let shaft = sd_segment(p, self.start, self.end);
        let (dx, dy) = (self.end.0 - self.start.0, self.end.1 - self.start.1);
        let span = length(dx, dy);
        if span <= f32::MIN_POSITIVE {
            return shaft;
        }
        let head = self.head_len().min(span);
        let (ux, uy) = (-dx / span * head, -dy / span * head);
        let (sin, cos) = (BARB_ANGLE.sin(), BARB_ANGLE.cos());
        let left = (
            self.end.0 + ux * cos - uy * sin,
            self.end.1 + ux * sin + uy * cos,
        );
        let right = (
            self.end.0 + ux * cos + uy * sin,
            self.end.1 - ux * sin + uy * cos,
        );
        shaft
            .min(sd_segment(p, self.end, left))
            .min(sd_segment(p, self.end, right))
    }

    pub fn distance(&self, x: f32, y: f32) -> f32 {
        let p = (x, y);
        match self.tool {
            Tool::Pen
            | Tool::Eraser
            | Tool::SelectRect
            | Tool::SelectEllipse
            | Tool::SelectLasso
            | Tool::Fill
            | Tool::Transform => f32::MAX,
            Tool::Line => sd_segment(p, self.start, self.end) - self.half_width,
            Tool::Arrow => self.arrow_distance(p) - self.half_width,
            Tool::Rect => {
                let d = sd_box(p, self.center(), self.half_extent());
                if self.fill {
                    d
                } else {
                    d.abs() - self.half_width
                }
            }
            Tool::Ellipse => {
                let d = sd_ellipse(p, self.center(), self.half_extent());
                if self.fill {
                    d
                } else {
                    d.abs() - self.half_width
                }
            }
        }
    }

    pub fn coverage(&self, x: f32, y: f32) -> f32 {
        (0.5 - self.distance(x, y)).clamp(0.0, 1.0)
    }

    pub fn padding(&self) -> f32 {
        self.half_width
            + if self.tool == Tool::Arrow {
                self.head_len()
            } else {
                0.0
            }
            + 1.0
    }

    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        let pad = self.padding();
        (
            self.start.0.min(self.end.0) - pad,
            self.start.1.min(self.end.1) - pad,
            self.start.0.max(self.end.0) + pad,
            self.start.1.max(self.end.1) + pad,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_coverage_on_path() {
        let s = Shape {
            tool: Tool::Line,
            start: (0.0, 0.0),
            end: (100.0, 0.0),
            half_width: 2.0,
            fill: false,
        };
        assert!(s.coverage(50.0, 0.0) > 0.9);
        assert!(s.coverage(50.0, 20.0) < 0.1);
    }

    #[test]
    fn rect_bounds_include_pad() {
        let s = Shape {
            tool: Tool::Rect,
            start: (10.0, 10.0),
            end: (40.0, 40.0),
            half_width: 2.0,
            fill: false,
        };
        let (x0, y0, x1, y1) = s.bounds();
        assert!(x0 < 10.0 && y0 < 10.0 && x1 > 40.0 && y1 > 40.0);
    }
}
