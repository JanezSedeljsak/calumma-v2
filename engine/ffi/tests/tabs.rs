//! Titlebar open-tab persistence across the FFI.

use calumma_ffi::*;
use std::ffi::{CStr, CString};
use std::ptr;

fn engine() -> (tempfile::TempDir, *mut CalmEngine) {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("tabs.sqlite").to_str().unwrap()).unwrap();
    let ptr = unsafe { calm_engine_new(path.as_ptr()) };
    assert!(!ptr.is_null());
    (dir, ptr)
}

fn create(ptr: *mut CalmEngine, name: &str) -> String {
    let name_c = CString::new(name).unwrap();
    let id_ptr = unsafe { calm_project_create(ptr, name_c.as_ptr(), 64, 64) };
    assert!(!id_ptr.is_null());
    let id = unsafe { CStr::from_ptr(id_ptr) }
        .to_str()
        .unwrap()
        .to_string();
    unsafe { calm_string_free(id_ptr) };
    id
}

#[test]
fn open_tabs_round_trip_through_the_store() {
    let (_dir, e) = engine();
    let a = create(e, "Alpha");
    let b = create(e, "Beta");
    let a_c = CString::new(a.clone()).unwrap();
    let b_c = CString::new(b.clone()).unwrap();
    let ids = [a_c.as_ptr(), b_c.as_ptr()];
    assert_eq!(
        unsafe { calm_set_open_project_tabs(e, ids.as_ptr(), ids.len()) },
        CalmStatus::Ok
    );

    let mut out: [*mut i8; 4] = [ptr::null_mut(); 4];
    let n = unsafe { calm_open_project_tabs(e, out.as_mut_ptr(), out.len()) };
    assert_eq!(n, 2);
    let read: Vec<String> = out[..n]
        .iter()
        .map(|p| unsafe { CStr::from_ptr(*p) }.to_str().unwrap().to_string())
        .collect();
    for p in out[..n].iter() {
        unsafe { calm_string_free(*p) };
    }
    assert_eq!(read, vec![a, b]);
    unsafe { calm_engine_free(e) };
}

#[test]
fn open_tabs_survives_null_and_zero_capacity() {
    assert_eq!(
        unsafe { calm_open_project_tabs(ptr::null_mut(), ptr::null_mut(), 4) },
        0
    );
    let (_dir, e) = engine();
    assert_eq!(unsafe { calm_open_project_tabs(e, ptr::null_mut(), 4) }, 0);
    let mut slot = ptr::null_mut();
    assert_eq!(unsafe { calm_open_project_tabs(e, &mut slot, 0) }, 0);
    assert_eq!(
        unsafe { calm_set_open_project_tabs(ptr::null_mut(), ptr::null(), 1) },
        CalmStatus::Null
    );
    unsafe { calm_engine_free(e) };
}
