use calumma_core::shape::*;

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
