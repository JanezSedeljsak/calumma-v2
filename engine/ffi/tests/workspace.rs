use calumma_ffi::*;
use std::ffi::{CStr, CString};
use std::ptr;

struct TestEngine {
    ptr: *mut CalmEngine,
    _dir: tempfile::TempDir,
}

impl TestEngine {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.sqlite");
        let path_c = CString::new(path.to_str().unwrap()).unwrap();
        let ptr = unsafe { calm_engine_new(path_c.as_ptr()) };
        assert!(!ptr.is_null());
        Self { ptr, _dir: dir }
    }

    fn create_project(&self, name: &str, w: u32, h: u32) -> String {
        let name_c = CString::new(name).unwrap();
        let id_ptr = unsafe { calm_project_create(self.ptr, name_c.as_ptr(), w, h) };
        assert!(!id_ptr.is_null());
        let id = unsafe { CStr::from_ptr(id_ptr) }
            .to_str()
            .unwrap()
            .to_string();
        unsafe { calm_string_free(id_ptr) };
        id
    }

    fn create_workspace(&self, name: &str) -> String {
        let name_c = CString::new(name).unwrap();
        let id_ptr = unsafe { calm_workspace_create(self.ptr, name_c.as_ptr()) };
        assert!(!id_ptr.is_null());
        let id = unsafe { CStr::from_ptr(id_ptr) }
            .to_str()
            .unwrap()
            .to_string();
        unsafe { calm_string_free(id_ptr) };
        id
    }
}

impl Drop for TestEngine {
    fn drop(&mut self) {
        unsafe { calm_engine_free(self.ptr) };
    }
}

fn empty_workspace_slot() -> CalmWorkspaceInfo {
    CalmWorkspaceInfo {
        id: ptr::null_mut(),
        name: ptr::null_mut(),
        accent: 0,
        active_project_id: ptr::null_mut(),
        opened_at: 0,
    }
}

fn free_workspace(info: &CalmWorkspaceInfo) {
    unsafe {
        if !info.id.is_null() {
            calm_string_free(info.id);
        }
        if !info.name.is_null() {
            calm_string_free(info.name);
        }
        if !info.active_project_id.is_null() {
            calm_string_free(info.active_project_id);
        }
    }
}

fn empty_project_slot() -> CalmProjectInfo {
    CalmProjectInfo {
        id: ptr::null_mut(),
        name: ptr::null_mut(),
        width: 0,
        height: 0,
        opened_at: 0,
        accent: 0,
    }
}

fn free_project(info: &CalmProjectInfo) {
    unsafe {
        if !info.id.is_null() {
            calm_string_free(info.id);
        }
        if !info.name.is_null() {
            calm_string_free(info.name);
        }
    }
}

#[test]
fn workspace_ffi_rename_accent_touch_get_and_delete() {
    let engine = TestEngine::new();
    let ws = engine.create_workspace("Desk");
    let ws_c = CString::new(ws.as_str()).unwrap();
    let renamed = CString::new("Studio").unwrap();
    unsafe {
        assert_eq!(
            calm_workspace_rename(engine.ptr, ws_c.as_ptr(), renamed.as_ptr()),
            CalmStatus::Ok
        );
        assert_eq!(
            calm_workspace_set_accent(engine.ptr, ws_c.as_ptr(), 0x11_22_33),
            CalmStatus::Ok
        );
        assert_eq!(
            calm_workspace_touch(engine.ptr, ws_c.as_ptr()),
            CalmStatus::Ok
        );
        let mut info = empty_workspace_slot();
        assert_eq!(
            calm_workspace_get(engine.ptr, ws_c.as_ptr(), &mut info),
            CalmStatus::Ok
        );
        assert_eq!(CStr::from_ptr(info.name).to_str().unwrap(), "Studio");
        assert_eq!(info.accent, 0x11_22_33);
        free_workspace(&info);
        assert_eq!(
            calm_workspace_delete(engine.ptr, ws_c.as_ptr()),
            CalmStatus::Ok
        );
        assert_eq!(
            calm_workspace_get(engine.ptr, ws_c.as_ptr(), &mut info),
            CalmStatus::Error
        );
    }
}

#[test]
fn workspace_ffi_projects_active_and_remove() {
    let engine = TestEngine::new();
    let project = engine.create_project("Board", 32, 24);
    let ws = engine.create_workspace("Desk");
    let ws_c = CString::new(ws.as_str()).unwrap();
    let project_c = CString::new(project.as_str()).unwrap();
    unsafe {
        assert_eq!(
            calm_workspace_add_project(engine.ptr, ws_c.as_ptr(), project_c.as_ptr()),
            CalmStatus::Ok
        );
        assert_eq!(
            calm_workspace_set_active_project(engine.ptr, ws_c.as_ptr(), project_c.as_ptr()),
            CalmStatus::Ok
        );
        let mut projects: Vec<CalmProjectInfo> = (0..4).map(|_| empty_project_slot()).collect();
        let n = calm_workspace_projects(engine.ptr, ws_c.as_ptr(), projects.as_mut_ptr(), 4);
        assert_eq!(n, 1);
        assert_eq!(
            CStr::from_ptr(projects[0].id).to_str().unwrap(),
            project.as_str()
        );
        free_project(&projects[0]);

        let mut info = empty_workspace_slot();
        assert_eq!(
            calm_workspace_get(engine.ptr, ws_c.as_ptr(), &mut info),
            CalmStatus::Ok
        );
        assert!(!info.active_project_id.is_null());
        assert_eq!(
            CStr::from_ptr(info.active_project_id).to_str().unwrap(),
            project.as_str()
        );
        free_workspace(&info);

        assert_eq!(
            calm_workspace_remove_project(engine.ptr, ws_c.as_ptr(), project_c.as_ptr()),
            CalmStatus::Ok
        );
        let n = calm_workspace_projects(engine.ptr, ws_c.as_ptr(), projects.as_mut_ptr(), 4);
        assert_eq!(n, 0);
        assert_eq!(
            calm_workspace_set_active_project(engine.ptr, ws_c.as_ptr(), ptr::null()),
            CalmStatus::Ok
        );
    }
}

#[test]
fn workspace_ffi_create_for_project_and_lookup() {
    let engine = TestEngine::new();
    let project = engine.create_project("Imported", 40, 40);
    let project_c = CString::new(project.as_str()).unwrap();
    let name = CString::new("Wrap").unwrap();
    unsafe {
        let ws_id =
            calm_workspace_create_for_project(engine.ptr, project_c.as_ptr(), name.as_ptr());
        assert!(!ws_id.is_null());
        let found = calm_workspace_for_project(engine.ptr, project_c.as_ptr());
        assert!(!found.is_null());
        assert_eq!(
            CStr::from_ptr(ws_id).to_str().unwrap(),
            CStr::from_ptr(found).to_str().unwrap()
        );
        calm_string_free(ws_id);
        calm_string_free(found);

        let orphan = engine.create_project("Loose", 8, 8);
        let orphan_c = CString::new(orphan.as_str()).unwrap();
        let missing = calm_workspace_for_project(engine.ptr, orphan_c.as_ptr());
        assert!(missing.is_null());
    }
}

#[test]
fn workspace_ffi_open_tabs_round_trip() {
    let engine = TestEngine::new();
    let a = engine.create_workspace("A");
    let b = engine.create_workspace("B");
    let a_c = CString::new(a.as_str()).unwrap();
    let b_c = CString::new(b.as_str()).unwrap();
    let ids = [a_c.as_ptr(), b_c.as_ptr()];
    unsafe {
        assert_eq!(
            calm_set_open_workspace_tabs(engine.ptr, ids.as_ptr(), 2),
            CalmStatus::Ok
        );
        let mut out = [ptr::null_mut::<std::os::raw::c_char>(); 8];
        let n = calm_open_workspace_tabs(engine.ptr, out.as_mut_ptr(), 8);
        assert_eq!(n, 2);
        assert_eq!(CStr::from_ptr(out[0]).to_str().unwrap(), a.as_str());
        assert_eq!(CStr::from_ptr(out[1]).to_str().unwrap(), b.as_str());
        calm_string_free(out[0]);
        calm_string_free(out[1]);
        assert_eq!(
            calm_set_open_workspace_tabs(engine.ptr, ptr::null(), 0),
            CalmStatus::Ok
        );
        assert_eq!(calm_open_workspace_tabs(engine.ptr, out.as_mut_ptr(), 8), 0);
    }
}

#[test]
fn workspace_ffi_list_returns_created_workspaces() {
    let engine = TestEngine::new();
    let _ = engine.create_workspace("One");
    let _ = engine.create_workspace("Two");
    let mut buf: Vec<CalmWorkspaceInfo> = (0..8).map(|_| empty_workspace_slot()).collect();
    unsafe {
        let n = calm_workspace_list(engine.ptr, buf.as_mut_ptr(), 8);
        assert_eq!(n, 2);
        for item in buf.iter().take(n) {
            free_workspace(item);
        }
        assert_eq!(calm_workspace_list(engine.ptr, ptr::null_mut(), 8), 0);
        assert_eq!(calm_workspace_list(ptr::null_mut(), buf.as_mut_ptr(), 8), 0);
        assert_eq!(
            calm_workspace_create(ptr::null_mut(), ptr::null()),
            ptr::null_mut()
        );
        assert_eq!(
            calm_workspace_create_for_project(ptr::null_mut(), ptr::null(), ptr::null()),
            ptr::null_mut()
        );
    }
}

#[test]
fn workspace_ffi_null_and_error_guards() {
    let engine = TestEngine::new();
    let text = CString::new("x").unwrap();
    let mut info = empty_workspace_slot();
    let mut projects: Vec<CalmProjectInfo> = (0..1).map(|_| empty_project_slot()).collect();
    let mut png: *mut u8 = ptr::null_mut();
    let mut len = 0usize;
    unsafe {
        assert_eq!(
            calm_workspace_rename(ptr::null_mut(), text.as_ptr(), text.as_ptr()),
            CalmStatus::Null
        );
        assert_eq!(
            calm_workspace_set_accent(ptr::null_mut(), text.as_ptr(), 0),
            CalmStatus::Null
        );
        assert_eq!(
            calm_workspace_add_project(ptr::null_mut(), text.as_ptr(), text.as_ptr()),
            CalmStatus::Null
        );
        assert_eq!(
            calm_workspace_remove_project(ptr::null_mut(), text.as_ptr(), text.as_ptr()),
            CalmStatus::Null
        );
        assert_eq!(
            calm_workspace_set_active_project(ptr::null_mut(), text.as_ptr(), text.as_ptr()),
            CalmStatus::Null
        );
        assert_eq!(
            calm_workspace_get(ptr::null_mut(), text.as_ptr(), &mut info),
            CalmStatus::Null
        );
        assert_eq!(
            calm_workspace_get(engine.ptr, text.as_ptr(), ptr::null_mut()),
            CalmStatus::Null
        );
        assert_eq!(
            calm_set_open_workspace_tabs(ptr::null_mut(), ptr::null(), 1),
            CalmStatus::Null
        );
        assert_eq!(
            calm_workspace_projects(ptr::null_mut(), text.as_ptr(), projects.as_mut_ptr(), 1),
            0
        );
        assert_eq!(
            calm_open_workspace_tabs(ptr::null_mut(), ptr::null_mut(), 1),
            0
        );
        assert_eq!(
            calm_workspace_delete(engine.ptr, text.as_ptr()),
            CalmStatus::Error
        );
        assert_eq!(
            calm_project_thumbnail(engine.ptr, text.as_ptr(), &mut png, &mut len),
            CalmStatus::Error
        );
        assert!(calm_workspace_for_project(ptr::null_mut(), text.as_ptr()).is_null());
        assert_eq!(
            calm_workspace_rename(engine.ptr, ptr::null(), text.as_ptr()),
            CalmStatus::Null
        );
        assert_eq!(
            calm_workspace_add_project(engine.ptr, ptr::null(), text.as_ptr()),
            CalmStatus::Null
        );
        assert_eq!(
            calm_workspace_remove_project(engine.ptr, text.as_ptr(), ptr::null()),
            CalmStatus::Null
        );
        assert_eq!(
            calm_workspace_set_active_project(engine.ptr, ptr::null(), text.as_ptr()),
            CalmStatus::Null
        );
        assert_eq!(
            calm_workspace_set_accent(engine.ptr, ptr::null(), 0),
            CalmStatus::Null
        );
        assert_eq!(
            calm_workspace_touch(engine.ptr, ptr::null()),
            CalmStatus::Null
        );
        assert_eq!(
            calm_workspace_delete(engine.ptr, ptr::null()),
            CalmStatus::Null
        );
        assert_eq!(
            calm_project_thumbnail(engine.ptr, ptr::null(), &mut png, &mut len),
            CalmStatus::Null
        );
        assert_eq!(
            calm_set_open_workspace_tabs(engine.ptr, ptr::null(), 1),
            CalmStatus::Null
        );
        assert_eq!(
            calm_workspace_projects(engine.ptr, ptr::null(), projects.as_mut_ptr(), 1),
            0
        );
        assert_eq!(calm_open_workspace_tabs(engine.ptr, ptr::null_mut(), 1), 0);
        assert!(calm_workspace_create(engine.ptr, ptr::null()).is_null());
        assert!(
            calm_workspace_create_for_project(engine.ptr, ptr::null(), text.as_ptr()).is_null()
        );
        assert!(
            calm_workspace_create_for_project(engine.ptr, text.as_ptr(), ptr::null()).is_null()
        );
        assert!(calm_workspace_for_project(engine.ptr, ptr::null()).is_null());
    }
}

#[test]
fn create_workspace_for_project_uses_seed_accent_when_doc_closed() {
    let engine = TestEngine::new();
    let project = engine.create_project("Closed", 16, 16);
    assert_eq!(unsafe { calm_project_close(engine.ptr) }, CalmStatus::Ok);
    let project_c = CString::new(project.as_str()).unwrap();
    let name = CString::new("Wrapped").unwrap();
    unsafe {
        let ws = calm_workspace_create_for_project(engine.ptr, project_c.as_ptr(), name.as_ptr());
        assert!(!ws.is_null());
        calm_string_free(ws);
    }
}

#[test]
fn project_thumbnail_requires_saved_blob() {
    let engine = TestEngine::new();
    let project = engine.create_project("Thumb", 48, 32);
    let project_c = CString::new(project.as_str()).unwrap();
    unsafe {
        assert_eq!(calm_project_save(engine.ptr), CalmStatus::Ok);
        let mut png: *mut u8 = ptr::null_mut();
        let mut len = 0usize;
        assert_eq!(
            calm_project_thumbnail(engine.ptr, project_c.as_ptr(), &mut png, &mut len),
            CalmStatus::Ok
        );
        assert!(len > 8);
        assert_eq!(
            std::slice::from_raw_parts(png, 4),
            &[0x89, b'P', b'N', b'G']
        );
        calm_buffer_free(png, len);
        assert_eq!(
            calm_project_thumbnail(engine.ptr, project_c.as_ptr(), ptr::null_mut(), &mut len),
            CalmStatus::Null
        );
    }
}

#[test]
fn workspace_switch_opens_the_named_project() {
    let engine = TestEngine::new();
    let a = engine.create_project("A", 32, 24);
    let b = engine.create_project("B", 64, 48);
    let ws = engine.create_workspace("Desk");
    let ws_c = CString::new(ws.as_str()).unwrap();
    let a_c = CString::new(a.as_str()).unwrap();
    let b_c = CString::new(b.as_str()).unwrap();
    unsafe {
        assert_eq!(
            calm_workspace_add_project(engine.ptr, ws_c.as_ptr(), a_c.as_ptr()),
            CalmStatus::Ok
        );
        assert_eq!(
            calm_workspace_add_project(engine.ptr, ws_c.as_ptr(), b_c.as_ptr()),
            CalmStatus::Ok
        );
        assert_eq!(
            calm_workspace_switch(engine.ptr, ws_c.as_ptr(), a_c.as_ptr()),
            CalmStatus::Ok
        );
        let mut state = CalmState {
            width: 0,
            height: 0,
            zoom: 0.0,
            min_zoom: 0.0,
            max_zoom: 0.0,
            pan_x: 0.0,
            pan_y: 0.0,
            active_layer: 0,
            layer_count: 0,
            can_undo: 0,
            can_redo: 0,
            stroke_active: 0,
            dark_theme: 0,
            accent: 0,
            zoom_unit: 0.0,
            last_shape_tool: 0,
            last_select_tool: 0,
        };
        assert_eq!(calm_engine_state(engine.ptr, &mut state), CalmStatus::Ok);
        assert_eq!(state.width, 32);
        assert_eq!(state.height, 24);
        assert_eq!(
            calm_workspace_switch(engine.ptr, ws_c.as_ptr(), b_c.as_ptr()),
            CalmStatus::Ok
        );
        assert_eq!(calm_engine_state(engine.ptr, &mut state), CalmStatus::Ok);
        assert_eq!(state.width, 64);
        assert_eq!(state.height, 48);
    }
}

#[test]
fn workspace_switch_restores_viewport() {
    let engine = TestEngine::new();
    let a = engine.create_project("A", 1280, 720);
    let b = engine.create_project("B", 640, 360);
    let ws = engine.create_workspace("Desk");
    let ws_c = CString::new(ws.as_str()).unwrap();
    let a_c = CString::new(a.as_str()).unwrap();
    let b_c = CString::new(b.as_str()).unwrap();
    unsafe {
        assert_eq!(
            calm_engine_resize(engine.ptr, 960, 540, 2.0),
            CalmStatus::Ok
        );
        assert_eq!(
            calm_workspace_add_project(engine.ptr, ws_c.as_ptr(), a_c.as_ptr()),
            CalmStatus::Ok
        );
        assert_eq!(
            calm_workspace_add_project(engine.ptr, ws_c.as_ptr(), b_c.as_ptr()),
            CalmStatus::Ok
        );
        assert_eq!(
            calm_workspace_switch(engine.ptr, ws_c.as_ptr(), a_c.as_ptr()),
            CalmStatus::Ok
        );
        let mut state = CalmState {
            width: 0,
            height: 0,
            zoom: 0.0,
            min_zoom: 0.0,
            max_zoom: 0.0,
            pan_x: 0.0,
            pan_y: 0.0,
            active_layer: 0,
            layer_count: 0,
            can_undo: 0,
            can_redo: 0,
            stroke_active: 0,
            dark_theme: 0,
            accent: 0,
            zoom_unit: 0.0,
            last_shape_tool: 0,
            last_select_tool: 0,
        };
        assert_eq!(calm_engine_state(engine.ptr, &mut state), CalmStatus::Ok);
        assert!(
            state.zoom < 0.9,
            "wide board should fit below 1x, got {}",
            state.zoom
        );
        assert_eq!(
            calm_workspace_switch(engine.ptr, ws_c.as_ptr(), b_c.as_ptr()),
            CalmStatus::Ok
        );
        assert_eq!(calm_engine_state(engine.ptr, &mut state), CalmStatus::Ok);
        assert!(
            state.zoom > 1.05,
            "smaller board should fit above 1x with a live viewport, got {}",
            state.zoom
        );
        assert_eq!(calm_engine_render(engine.ptr), CalmStatus::Ok);
    }
}
