use crate::selection_mask::SelectionMask;
use crate::shape::{Shape, Tool};
use crate::tile::DocRect;

#[derive(Clone, Debug, PartialEq)]
pub enum SelectionShape {
    Rect {
        start: (f32, f32),
        end: (f32, f32),
    },
    Ellipse {
        start: (f32, f32),
        end: (f32, f32),
    },
    Lasso {
        points: Vec<(f32, f32)>,
    },
    /// What the magic wand's flood fill reached. The first shape whose answer is stored rather
    /// than derived — see `selection_mask.rs`. Everything downstream (paint clipping, copy,
    /// cut, delete) goes through `bounds` and `contains`, so nothing else had to change to
    /// accept one.
    Mask(SelectionMask),
}

impl SelectionShape {
    pub fn bounds(&self) -> DocRect {
        match self {
            Self::Rect { start, end } | Self::Ellipse { start, end } => DocRect::from_floats(
                start.0.min(end.0),
                start.1.min(end.1),
                start.0.max(end.0),
                start.1.max(end.1),
            ),
            Self::Lasso { points } => {
                let mut min_x = f32::INFINITY;
                let mut min_y = f32::INFINITY;
                let mut max_x = f32::NEG_INFINITY;
                let mut max_y = f32::NEG_INFINITY;
                for &(x, y) in points {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
                DocRect::from_floats(min_x, min_y, max_x, max_y)
            }
            Self::Mask(mask) => mask.bounds(),
        }
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        match self {
            Self::Rect { start, end } => {
                let shape = Shape {
                    tool: Tool::Rect,
                    start: *start,
                    end: *end,
                    half_width: 0.0,
                    fill: true,
                    stroke: false,
                };
                shape.coverage(x, y) > 0.5
            }
            Self::Ellipse { start, end } => {
                let shape = Shape {
                    tool: Tool::Ellipse,
                    start: *start,
                    end: *end,
                    half_width: 0.0,
                    fill: true,
                    stroke: false,
                };
                shape.coverage(x, y) > 0.5
            }
            Self::Lasso { points } => point_in_polygon(x, y, points),
            // Callers hand pixel centres (`x + 0.5`), so flooring lands on the pixel that
            // centre belongs to rather than the one before it at exact integers.
            Self::Mask(mask) => mask.get(x.floor() as i32, y.floor() as i32),
        }
    }

    /// Bytes this shape owns beyond itself. Only a mask has any — the analytic shapes are a
    /// handful of floats, and a lasso's points are bounded by the stroke buffer.
    pub fn memory_bytes(&self) -> usize {
        match self {
            Self::Mask(mask) => mask.memory_bytes(),
            Self::Lasso { points } => points.capacity() * std::mem::size_of::<(f32, f32)>(),
            _ => 0,
        }
    }
}

fn point_in_polygon(x: f32, y: f32, points: &[(f32, f32)]) -> bool {
    if points.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = points.len() - 1;
    for i in 0..points.len() {
        let (xi, yi) = points[i];
        let (xj, yj) = points[j];
        if (yi > y) != (yj > y) {
            let x_intersect = xi + (y - yi) / (yj - yi) * (xj - xi);
            if x < x_intersect {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

#[derive(Clone, Debug, PartialEq)]
pub struct Selection {
    pub shape: SelectionShape,
}

impl Selection {
    pub fn bounds(&self) -> DocRect {
        self.shape.bounds()
    }

    pub fn memory_bytes(&self) -> usize {
        self.shape.memory_bytes()
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.shape.contains(x, y)
    }
}
