use calumma_core::{project_color, unpack_rgb, PROJECT_COLORS};
use calumma_ffi::Engine;

fn engine() -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.sqlite");
    let engine = Engine::new(Some(path.as_path())).expect("engine");
    (dir, engine)
}

#[test]
fn create_project_with_accent_stores_the_previewed_color() {
    let (_dir, mut engine) = engine();
    let accent = project_color(3);
    let id = engine
        .create_project_with_accent("Tinted", 32, 32, Some(accent))
        .unwrap();
    let summary = engine.project_summary(&id).unwrap();
    assert_eq!(unpack_rgb(summary.accent_rgb), accent);
}

#[test]
fn rename_and_recolor_update_the_open_document() {
    let (_dir, mut engine) = engine();
    let id = engine.create_project("Draft", 48, 48).unwrap();
    engine.rename_project(&id, "  Renamed  ").unwrap();
    engine.set_project_accent(&id, PROJECT_COLORS[5]).unwrap();
    assert_eq!(engine.project_name().as_deref(), Some("Renamed"));
    let summary = engine.project_summary(&id).unwrap();
    assert_eq!(summary.name, "Renamed");
    assert_eq!(unpack_rgb(summary.accent_rgb), PROJECT_COLORS[5]);

    engine.close_project();
    engine.open_project(&id).unwrap();
    assert_eq!(engine.project_name().as_deref(), Some("Renamed"));
    let reopened = engine.project_summary(&id).unwrap();
    assert_eq!(unpack_rgb(reopened.accent_rgb), PROJECT_COLORS[5]);
}

#[test]
fn rename_rejects_an_empty_name() {
    let (_dir, mut engine) = engine();
    let id = engine.create_project("Draft", 16, 16).unwrap();
    assert!(engine.rename_project(&id, "   ").is_err());
    assert_eq!(engine.project_name().as_deref(), Some("Draft"));
}
