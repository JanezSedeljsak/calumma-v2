use calumma_io::{ProjectStore, StoreError};
use tempfile::tempdir;

fn store() -> (tempfile::TempDir, ProjectStore) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("t.sqlite");
    let store = ProjectStore::open(&path).unwrap();
    (dir, store)
}

#[test]
fn rename_recolour_and_touch_workspace() {
    let (_dir, store) = store();
    let ws = store.create_workspace("Desk", Some([1, 2, 3])).unwrap();
    assert_eq!(ws.accent, [1, 2, 3]);

    store.rename_workspace(&ws.id, "Studio").unwrap();
    store.set_workspace_accent(&ws.id, [9, 8, 7]).unwrap();
    let before = store.workspace(&ws.id).unwrap().opened_at;
    store.touch_workspace(&ws.id).unwrap();
    let after = store.workspace(&ws.id).unwrap();
    assert_eq!(after.name, "Studio");
    assert_eq!(after.accent, [9, 8, 7]);
    assert!(after.opened_at >= before);

    let listed = store.list_workspaces(8).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, ws.id);
}

#[test]
fn workspace_mutations_reject_unknown_ids() {
    let (_dir, store) = store();
    assert!(matches!(
        store.rename_workspace("nope", "x"),
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        store.set_workspace_accent("nope", [1, 2, 3]),
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        store.delete_workspace("nope"),
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        store.touch_workspace("nope"),
        Err(StoreError::NotFound)
    ));
    assert!(matches!(store.workspace("nope"), Err(StoreError::NotFound)));
    assert!(matches!(
        store.set_workspace_active_project("nope", None),
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        store.workspace_projects("nope"),
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        store.remove_project_from_workspace("nope", "also-nope"),
        Err(StoreError::NotFound)
    ));
}

#[test]
fn add_project_rejects_missing_members_and_is_idempotent() {
    let (_dir, store) = store();
    let doc = store.create("A", 16, 16).unwrap();
    let ws = store.create_workspace("Desk", None).unwrap();

    assert!(matches!(
        store.add_project_to_workspace("missing-ws", &doc.id),
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        store.add_project_to_workspace(&ws.id, "missing-project"),
        Err(StoreError::NotFound)
    ));

    store.add_project_to_workspace(&ws.id, &doc.id).unwrap();
    store.add_project_to_workspace(&ws.id, &doc.id).unwrap();
    assert_eq!(store.workspace_projects(&ws.id).unwrap().len(), 1);
    assert_eq!(
        store
            .workspace(&ws.id)
            .unwrap()
            .active_project_id
            .as_deref(),
        Some(doc.id.as_str())
    );
}

#[test]
fn removing_last_active_project_clears_active_id() {
    let (_dir, store) = store();
    let doc = store.create("Solo", 16, 16).unwrap();
    let ws = store.create_workspace("Desk", None).unwrap();
    store.add_project_to_workspace(&ws.id, &doc.id).unwrap();
    store
        .remove_project_from_workspace(&ws.id, &doc.id)
        .unwrap();
    assert!(store.workspace_projects(&ws.id).unwrap().is_empty());
    assert!(store.workspace(&ws.id).unwrap().active_project_id.is_none());
}

#[test]
fn create_workspace_for_project_sets_active_and_membership() {
    let (_dir, store) = store();
    let doc = store.create("Board", 24, 24).unwrap();
    let ws = store
        .create_workspace_for_project(&doc.id, "From Project", [4, 5, 6])
        .unwrap();
    assert_eq!(ws.name, "From Project");
    assert_eq!(ws.accent, [4, 5, 6]);
    assert_eq!(ws.active_project_id.as_deref(), Some(doc.id.as_str()));
    assert_eq!(store.workspace_projects(&ws.id).unwrap().len(), 1);
    assert_eq!(
        store
            .workspace_containing_project(&doc.id)
            .unwrap()
            .unwrap()
            .id,
        ws.id
    );
}

#[test]
fn workspace_containing_project_is_none_when_unassigned() {
    let (_dir, store) = store();
    let doc = store.create("Loose", 8, 8).unwrap();
    assert!(store
        .workspace_containing_project(&doc.id)
        .unwrap()
        .is_none());
}

#[test]
fn open_workspace_tabs_preserve_order_and_can_clear() {
    let (_dir, store) = store();
    let a = store.create_workspace("A", None).unwrap();
    let b = store.create_workspace("B", None).unwrap();
    store
        .set_open_workspace_tabs(&[a.id.clone(), b.id.clone()])
        .unwrap();
    assert_eq!(
        store.open_workspace_tabs().unwrap(),
        vec![a.id.clone(), b.id.clone()]
    );
    store.set_open_workspace_tabs(&[]).unwrap();
    assert!(store.open_workspace_tabs().unwrap().is_empty());
}

#[test]
fn deleting_workspace_cascades_open_tabs() {
    let (_dir, store) = store();
    let ws = store.create_workspace("Desk", None).unwrap();
    store
        .set_open_workspace_tabs(std::slice::from_ref(&ws.id))
        .unwrap();
    store.delete_workspace(&ws.id).unwrap();
    assert!(store.open_workspace_tabs().unwrap().is_empty());
}

#[test]
fn project_thumbnail_rejects_missing_project() {
    let (_dir, store) = store();
    assert!(matches!(
        store.project_thumbnail("missing"),
        Err(StoreError::NotFound)
    ));
}
