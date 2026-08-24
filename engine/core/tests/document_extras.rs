use calumma_core::{
    names, Document, Layer, LayerContent, Shape, Tool, VectorItem, VectorPath, LAYER_ONE, PAPER,
};

#[test]
fn default_document_has_paper_and_raster() {
    let doc = Document::new("id".into(), "board", 100, 80);
    assert_eq!(doc.width, 100);
    assert_eq!(doc.height, 80);
    assert_eq!(doc.layers.len(), 2);
    assert_eq!(doc.layers[0].name, PAPER);
    assert!(doc.layers[0].content.is_raster());
    assert_eq!(
        doc.layers[0].tiles().unwrap().get_pixel(0, 0),
        [255, 255, 255, 255]
    );
    assert_eq!(doc.layers[1].name, LAYER_ONE);
    assert!(doc.layers[1].content.is_raster());
    assert_eq!(doc.active_layer, 1);
}

#[test]
fn remove_layer_can_delete_paper_and_last() {
    let mut doc = Document::new("id".into(), "board", 64, 64);
    assert!(doc.remove_layer(0));
    assert_eq!(doc.layers.len(), 1);
    assert!(!doc.layers[0].is_paper());
    assert!(doc.remove_layer(0));
    assert!(doc.layers.is_empty());
    assert!(!doc.remove_layer(0));
}

#[test]
fn camera_pan_clamped_when_zoomed_out() {
    let mut doc = Document::new("id".into(), "board", 200, 100);
    doc.resize_viewport(400.0, 300.0, 1.0);
    doc.fit_to_view();
    let zoom = doc.camera.zoom;
    doc.camera.pan_by(50.0, 50.0, 200.0, 100.0);
    assert!((doc.camera.zoom - zoom).abs() < f32::EPSILON);
}

#[test]
fn ellipse_and_arrow_commit_paint() {
    for tool in [Tool::Ellipse, Tool::Arrow] {
        let mut doc = Document::new("id".into(), "board", 256, 256);
        doc.tool = tool;
        doc.fill = tool.takes_fill();
        doc.resize_viewport(256.0, 256.0, 1.0);
        doc.fit_to_view();
        let (a, b) = doc.camera.to_screen(40.0, 40.0);
        let (c, d) = doc.camera.to_screen(120.0, 90.0);
        doc.pointer_down(a, b);
        doc.pointer_move(c, d);
        assert!(doc.preview_shape().is_some());
        doc.pointer_up(c, d);
        assert!(doc.preview_shape().is_none());
        assert!(!doc.layers[doc.active_layer].tiles().unwrap().is_empty());
    }
}

#[test]
fn shape_distance_rect_inside_negative_when_filled_path() {
    let shape = Shape {
        tool: Tool::Rect,
        start: (10.0, 10.0),
        end: (50.0, 40.0),
        half_width: 1.0,
        fill: true,
        stroke: false,
    };
    let d = shape.distance(30.0, 25.0);
    assert!(
        d < 0.0,
        "interior should be negative for filled rect SDF, got {d}"
    );
}

#[test]
fn vector_layer_content_and_bounds() {
    let layer = Layer::vector(
        names::numbered_vector_layer(1),
        vec![VectorItem::Path(VectorPath {
            points: vec![(0.0, 0.0), (8.0, 0.0), (8.0, 6.0)],
            closed: true,
            fill: true,
            stroke: false,
            color: [0, 0, 0, 255],
            stroke_color: [0, 0, 0, 255],
            stroke_width: 1.0,
        })],
    );
    assert!(matches!(layer.content, LayerContent::Vector(_)));
    assert_eq!(layer.content_bounds(), Some((0.0, 0.0, 8.0, 6.0)));
}

#[test]
fn redo_after_undo_restores_stroke() {
    let mut doc = Document::new("id".into(), "board", 128, 128);
    doc.resize_viewport(128.0, 128.0, 1.0);
    doc.fit_to_view();
    let (sx, sy) = doc.camera.to_screen(32.0, 32.0);
    doc.pointer_down(sx, sy);
    doc.pointer_up(sx, sy);
    let painted = doc.layers[doc.active_layer]
        .tiles()
        .unwrap()
        .get_pixel(32, 32);
    assert_ne!(painted, [0, 0, 0, 0]);
    assert!(doc.undo());
    assert_eq!(
        doc.layers[doc.active_layer]
            .tiles()
            .unwrap()
            .get_pixel(32, 32),
        [0, 0, 0, 0]
    );
    assert!(doc.redo());
    assert_eq!(
        doc.layers[doc.active_layer]
            .tiles()
            .unwrap()
            .get_pixel(32, 32),
        painted
    );
}

#[test]
fn brush_and_color_knobs_clamp() {
    let mut doc = Document::new("id".into(), "board", 64, 64);
    doc.brush_size = 0.0;
    assert!(doc.brush_size <= calumma_core::limits::BRUSH_SIZE_MAX);
    doc.color = [255, 0, 128, 200];
    assert_eq!(doc.color[1], 0);
}

#[test]
fn set_layer_visible_toggles_paper() {
    let mut doc = Document::new("id".into(), "board", 32, 32);
    doc.set_layer_visible(0, false);
    assert!(!doc.layers[0].visible);
    doc.set_layer_visible(0, true);
    assert!(doc.layers[0].visible);
}

#[test]
fn tool_helpers() {
    assert!(!Tool::Pen.is_shape());
    assert!(Tool::Line.is_shape());
    assert!(Tool::Rect.takes_fill());
    assert!(Tool::Triangle.takes_fill());
    assert!(Tool::Pentagon.takes_fill());
    assert!(!Tool::Line.takes_fill());
    assert_eq!(Tool::from_u32(2), Some(Tool::Rect));
    assert_eq!(Tool::from_u32(12), Some(Tool::Triangle));
    assert_eq!(Tool::from_u32(13), Some(Tool::Pentagon));
    assert_eq!(Tool::from_u32(15), Some(Tool::Move));
    assert_eq!(Tool::from_u32(99), None);
    assert!(Tool::Pen.takes_brush_size());
    assert!(!Tool::Move.takes_brush_size());
    assert!(Tool::Pen.shows_vector_mode());
    assert!(!Tool::Move.shows_vector_mode());
    assert!(!Tool::Text.takes_brush_size());
    assert!(Tool::Pen.takes_ink_opacity());
    assert!(Tool::Fill.takes_ink_opacity());
    assert!(Tool::Rect.takes_ink_opacity());
    assert!(!Tool::Eraser.takes_ink_opacity());
    assert!(!Tool::Text.takes_ink_opacity());
    assert!(!Tool::Move.takes_ink_opacity());
}
