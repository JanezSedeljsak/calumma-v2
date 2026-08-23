use calumma_core::selection::*;
use calumma_core::selection_mask::SelectionMask;

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

/// A lasso needs three points to enclose anything. Fewer is not an empty selection with a
/// zero-area interior — it is a shape `point_in_polygon` refuses outright, so a stray click
/// with the lasso tool cannot leave a selection that clips every later paint stroke to
/// nothing.
#[test]
fn a_lasso_of_fewer_than_three_points_contains_nothing() {
    for points in [vec![], vec![(10.0, 10.0)], vec![(10.0, 10.0), (20.0, 20.0)]] {
        let sel = Selection {
            shape: SelectionShape::Lasso {
                points: points.clone(),
            },
        };
        assert!(
            !sel.contains(10.0, 10.0),
            "{} points enclose nothing, not even their own vertices",
            points.len()
        );
        assert!(!sel.contains(15.0, 15.0));
    }
}

/// The notch of a C has to come out *outside*, which is the whole reason the lasso runs a
/// crossing count rather than testing its bounding box.
#[test]
fn a_concave_lasso_excludes_its_notch() {
    let sel = Selection {
        shape: SelectionShape::Lasso {
            points: vec![
                (0.0, 0.0),
                (30.0, 0.0),
                (30.0, 10.0),
                (10.0, 10.0),
                (10.0, 20.0),
                (30.0, 20.0),
                (30.0, 30.0),
                (0.0, 30.0),
            ],
        },
    };
    assert!(sel.contains(5.0, 15.0), "the spine of the C is inside");
    assert!(sel.contains(20.0, 5.0), "the top arm is inside");
    assert!(
        !sel.contains(20.0, 15.0),
        "the notch is inside the bounding box but outside the polygon"
    );
    assert!(!sel.contains(40.0, 15.0));
}

/// A scanline through a vertex must cross the outline once, not twice — the half-open
/// `(yi > y) != (yj > y)` rule is what stops a shared vertex being counted by both of its
/// edges and flipping the answer back to "outside".
#[test]
fn a_scanline_through_a_vertex_is_not_counted_twice() {
    let sel = Selection {
        shape: SelectionShape::Lasso {
            points: vec![(0.0, 0.0), (20.0, 10.0), (0.0, 20.0)],
        },
    };
    assert!(
        sel.contains(5.0, 10.0),
        "a point level with the far vertex is still inside"
    );
}

#[test]
fn a_rect_dragged_backwards_selects_the_same_region() {
    let forward = Selection {
        shape: SelectionShape::Rect {
            start: (10.0, 10.0),
            end: (30.0, 30.0),
        },
    };
    let backward = Selection {
        shape: SelectionShape::Rect {
            start: (30.0, 30.0),
            end: (10.0, 10.0),
        },
    };
    assert_eq!(forward.bounds(), backward.bounds());
    assert!(backward.contains(20.0, 20.0));
    assert!(!backward.contains(5.0, 20.0));
}

#[test]
fn a_zero_area_drag_selects_nothing() {
    for shape in [
        SelectionShape::Rect {
            start: (10.0, 10.0),
            end: (10.0, 10.0),
        },
        SelectionShape::Ellipse {
            start: (10.0, 10.0),
            end: (10.0, 10.0),
        },
    ] {
        let sel = Selection { shape };
        assert!(!sel.contains(10.0, 10.0));
        assert!(!sel.contains(10.5, 10.5));
    }
}

/// Only a mask and a lasso own anything off to the side; the analytic shapes are a handful of
/// floats and must not claim bytes the memory readout would then have to explain.
#[test]
fn only_the_shapes_that_store_something_report_bytes() {
    let rect = Selection {
        shape: SelectionShape::Rect {
            start: (0.0, 0.0),
            end: (100.0, 100.0),
        },
    };
    assert_eq!(rect.memory_bytes(), 0);

    let lasso = Selection {
        shape: SelectionShape::Lasso {
            points: vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)],
        },
    };
    assert!(lasso.memory_bytes() >= 3 * std::mem::size_of::<(f32, f32)>());

    let mut mask = SelectionMask::new((0, 0), 64, 64);
    mask.set(1, 1);
    let mask = Selection {
        shape: SelectionShape::Mask(mask.finish().unwrap()),
    };
    assert!(mask.memory_bytes() > 0);
}
