use crate::shape::{Shape, Tool};
use crate::tile::DocRect;

#[derive(Clone, Debug, PartialEq)]
pub enum SelectionShape {
    Rect { start: (f32, f32), end: (f32, f32) },
    Ellipse { start: (f32, f32), end: (f32, f32) },
    Lasso { points: Vec<(f32, f32)> },
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
                };
                shape.coverage(x, y) > 0.5
            }
            Self::Lasso { points } => point_in_polygon(x, y, points),
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

    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.shape.contains(x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_selection_contains_only_inside_points() {
        let sel = Selection {
            shape: SelectionShape::Rect {
                start: (10.0, 10.0),
                end: (30.0, 30.0),
            },
        };
        assert!(sel.contains(20.0, 20.0));
        assert!(!sel.contains(5.0, 5.0));
    }

    #[test]
    fn ellipse_selection_excludes_corners_of_its_bounds() {
        let sel = Selection {
            shape: SelectionShape::Ellipse {
                start: (0.0, 0.0),
                end: (20.0, 20.0),
            },
        };
        assert!(sel.contains(10.0, 10.0));
        assert!(!sel.contains(0.5, 0.5));
    }

    #[test]
    fn lasso_selection_uses_point_in_polygon() {
        let sel = Selection {
            shape: SelectionShape::Lasso {
                points: vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)],
            },
        };
        assert!(sel.contains(10.0, 10.0));
        assert!(!sel.contains(30.0, 30.0));
    }

    #[test]
    fn lasso_bounds_match_point_extents() {
        let sel = Selection {
            shape: SelectionShape::Lasso {
                points: vec![(5.0, 5.0), (25.0, 15.0), (5.0, 25.0)],
            },
        };
        let bounds = sel.bounds();
        assert_eq!(bounds.min_x, 5);
        assert_eq!(bounds.max_x, 25);
    }
}
