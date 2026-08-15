use calumma_core::{Document, Tool};

#[test]
fn many_strokes_respect_memory_budget() {
    let mut doc = Document::new("id".into(), "n", 512, 512);
    doc.history = calumma_core::History::new(2 * 256 * 256 * 4);
    doc.resize_viewport(512.0, 512.0, 1.0);
    doc.fit_to_view();
    for i in 0..20 {
        let x = 20.0 + i as f32 * 10.0;
        let (sx, sy) = doc.camera.to_screen(x, 40.0);
        doc.pointer_down(sx, sy);
        doc.pointer_up(sx, sy);
    }
    assert!(doc.history.can_undo());
    let mut steps = 0;
    while doc.undo() {
        steps += 1;
        assert!(steps < 20);
    }
}

#[test]
fn layer_add_and_clear() {
    let mut doc = Document::new("id".into(), "n", 128, 128);
    doc.add_layer("Layer 2");
    assert_eq!(doc.layers.len(), 3);
    doc.set_active_layer(2);
    doc.resize_viewport(128.0, 128.0, 1.0);
    doc.fit_to_view();
    let (sx, sy) = doc.camera.to_screen(10.0, 10.0);
    doc.pointer_down(sx, sy);
    doc.pointer_up(sx, sy);
    doc.clear_active_layer();
    assert!(
        doc.layers[2].tiles().unwrap().is_empty()
            || doc.layers[2].tiles().unwrap().get_pixel(10, 10) == [0, 0, 0, 0]
    );
}

#[test]
fn tool_switch_shape_line() {
    let mut doc = Document::new("id".into(), "n", 200, 200);
    doc.tool = Tool::Line;
    doc.brush_size = 4.0;
    doc.resize_viewport(200.0, 200.0, 1.0);
    doc.fit_to_view();
    let (a, b) = doc.camera.to_screen(20.0, 20.0);
    let (c, d) = doc.camera.to_screen(80.0, 20.0);
    doc.pointer_down(a, b);
    doc.pointer_move(c, d);
    assert!(doc.preview_shape().is_some());
    doc.pointer_up(c, d);
    assert!(doc.preview_shape().is_none());
    assert!(!doc.layers[doc.active_layer].tiles().unwrap().is_empty());
}
