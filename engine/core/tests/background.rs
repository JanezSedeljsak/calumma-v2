use calumma_core::Document;

#[test]
fn a_running_removal_marks_that_layer_for_the_sweep() {
    let mut doc = Document::new("b".into(), "B", 32, 32);
    doc.layers[1]
        .tiles_mut()
        .unwrap()
        .set_pixel(4, 4, [255, 0, 0, 255]);
    let id = doc.layers[1].id.clone();
    doc.begin_background_removal(id);
    assert!(doc.background_removal_animating());
    assert!(doc.has_animated_overlay());
    let (index, _corners) = doc.background_removal_target().expect("painted layer");
    assert_eq!(index, 1);
    doc.clear_background_removal();
    assert!(!doc.background_removal_animating());
    assert!(!doc.has_animated_overlay());
    assert!(doc.background_removal_target().is_none());
}
