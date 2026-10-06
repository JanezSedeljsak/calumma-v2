use calumma_ffi::{BackgroundNotice, Engine};

fn engine() -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.sqlite");
    let engine = Engine::new(Some(path.as_path())).expect("engine");
    (dir, engine)
}

#[test]
fn paper_cannot_remove_its_background() {
    let (_dir, mut engine) = engine();
    engine.create_project("Bg", 32, 32).unwrap();
    let paper = engine
        .list_layers()
        .into_iter()
        .find(|layer| layer.is_paper)
        .expect("paper")
        .index;
    assert!(!engine.can_remove_background(paper));
    assert!(!engine.background_removal_running());
}

#[test]
fn a_paint_layer_follows_the_platform() {
    let (_dir, mut engine) = engine();
    engine.create_project("Bg", 32, 32).unwrap();
    let index = engine.active_layer_index().expect("paint layer");
    assert_eq!(
        engine.can_remove_background(index),
        engine.background_removal_available()
    );
    assert!(!engine.background_removal_running());
}

#[test]
fn an_empty_layer_reports_no_subject_when_removal_exists() {
    let (_dir, mut engine) = engine();
    if !engine.background_removal_available() {
        return;
    }
    engine.create_project("Bg", 32, 32).unwrap();
    let index = engine.active_layer_index().expect("paint layer");
    assert!(!engine.remove_background(index));
    assert!(matches!(
        engine.take_background_notice(),
        Some(BackgroundNotice::NoSubject)
    ));
    assert!(!engine.background_removal_running());
}

#[test]
fn closing_the_project_drops_a_pending_notice() {
    let (_dir, mut engine) = engine();
    engine.create_project("Bg", 32, 32).unwrap();
    let index = engine.active_layer_index().expect("paint layer");
    assert!(!engine.remove_background(index));
    engine.close_project();
    assert!(engine.take_background_notice().is_none());
    assert!(!engine.background_removal_running());
}

#[test]
fn remove_background_is_unavailable_off_macos() {
    let (_dir, mut engine) = engine();
    if engine.background_removal_available() {
        return;
    }
    engine.create_project("Bg", 32, 32).unwrap();
    let index = engine.active_layer_index().expect("paint layer");
    assert!(!engine.remove_background(index));
    assert!(matches!(
        engine.take_background_notice(),
        Some(BackgroundNotice::Unavailable)
    ));
    assert!(!engine.background_removal_running());
}
