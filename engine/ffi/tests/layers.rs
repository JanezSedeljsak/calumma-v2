use calumma_core::names::{LAYER_ONE, PAPER};
use calumma_ffi::Engine;

fn engine() -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.sqlite");
    let engine = Engine::new(Some(path.as_path())).expect("engine");
    (dir, engine)
}

fn names(engine: &Engine) -> Vec<String> {
    engine
        .list_layers()
        .into_iter()
        .map(|layer| layer.name)
        .collect()
}

#[test]
fn move_layer_row_drops_onto_the_row() {
    let (_dir, mut engine) = engine();
    engine.create_project("Stack", 32, 32).unwrap();
    engine.add_layer();
    assert_eq!(names(&engine), ["Layer 2", LAYER_ONE, PAPER]);

    assert!(engine.move_layer_row(0, 1));
    assert_eq!(names(&engine), [LAYER_ONE, "Layer 2", PAPER]);

    assert!(engine.move_layer_row(1, 0));
    assert_eq!(names(&engine), ["Layer 2", LAYER_ONE, PAPER]);
}

#[test]
fn list_layers_marks_the_clip_base_row() {
    let (_dir, mut engine) = engine();
    engine.create_project("Clip", 32, 32).unwrap();
    engine.add_layer();
    let layers = engine.list_layers();
    assert!(layers
        .iter()
        .all(|layer| !layer.clipped && !layer.clip_base));

    assert!(engine.create_clipping_mask(2));
    let layers = engine.list_layers();
    let top = layers.iter().find(|layer| layer.index == 2).unwrap();
    let base = layers.iter().find(|layer| layer.index == 1).unwrap();
    assert!(top.clipped);
    assert!(!top.clip_base);
    assert!(!base.clipped);
    assert!(base.clip_base);
}

#[test]
fn move_layer_row_refuses_paper() {
    let (_dir, mut engine) = engine();
    engine.create_project("Stack", 16, 16).unwrap();
    engine.add_layer();
    let last = names(&engine).len() - 1;
    assert!(!engine.move_layer_row(0, last));
    assert!(!engine.move_layer_row(last, 0));
    assert_eq!(names(&engine), ["Layer 2", LAYER_ONE, PAPER]);
}

fn opaque_rgb(rgba: &[u8]) -> Option<[u8; 3]> {
    rgba.chunks_exact(4)
        .find(|px| px[3] > 0)
        .map(|px| [px[0], px[1], px[2]])
}

#[test]
fn layer_thumbnail_bakes_adjustments() {
    let (_dir, mut engine) = engine();
    engine.create_project("Filters", 32, 32).unwrap();
    let paper = engine
        .list_layers()
        .into_iter()
        .find(|layer| layer.is_paper)
        .expect("paper")
        .index;
    let before = engine.layer_thumbnail_rgba(paper).expect("thumb");
    let before_rgb = opaque_rgb(&before.2).expect("painted");
    let before_rev = engine.layer_preview_revision(paper);

    let mut adjustments = engine.layer_adjustments(paper);
    adjustments.contrast = -1.0;
    engine.set_layer_adjustments(paper, adjustments);

    let after = engine.layer_thumbnail_rgba(paper).expect("thumb");
    let after_rgb = opaque_rgb(&after.2).expect("painted");
    assert_ne!(before_rgb, after_rgb);
    assert!(after_rgb.iter().all(|&c| (c as i16 - 128).abs() <= 1));
    assert_ne!(before_rev, engine.layer_preview_revision(paper));
}

fn opaque_alpha(rgba: &[u8]) -> Option<u8> {
    rgba.chunks_exact(4).find(|px| px[3] > 0).map(|px| px[3])
}

#[test]
fn layer_thumbnail_bakes_opacity() {
    let (_dir, mut engine) = engine();
    engine.create_project("Opacity", 32, 32).unwrap();
    let paper = engine
        .list_layers()
        .into_iter()
        .find(|layer| layer.is_paper)
        .expect("paper")
        .index;
    let before = engine.layer_thumbnail_rgba(paper).expect("thumb");
    let before_a = opaque_alpha(&before.2).expect("painted");
    let before_rev = engine.layer_preview_revision(paper);
    assert_eq!(before_a, 255);

    engine.set_layer_opacity(paper, 0.5);
    let after = engine.layer_thumbnail_rgba(paper).expect("thumb");
    let after_a = opaque_alpha(&after.2).expect("painted");
    assert_eq!(after_a, 128);
    assert_ne!(before_rev, engine.layer_preview_revision(paper));
}
