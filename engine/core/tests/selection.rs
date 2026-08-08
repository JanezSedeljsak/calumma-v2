use calumma_core::selection::*;

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
