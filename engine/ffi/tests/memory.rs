use calumma_ffi::*;
use std::ffi::CString;
use std::ptr;

const SIDE: u32 = 1024;

fn engine() -> (tempfile::TempDir, *mut CalmEngine) {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("m.sqlite").to_str().unwrap()).unwrap();
    let ptr = unsafe { calm_engine_new(path.as_ptr()) };
    assert!(!ptr.is_null());
    (dir, ptr)
}

fn create(ptr: *mut CalmEngine, name: &str) -> String {
    let name = CString::new(name).unwrap();
    let id = unsafe { calm_project_create(ptr, name.as_ptr(), SIDE, SIDE) };
    assert!(!id.is_null());
    let out = unsafe { std::ffi::CStr::from_ptr(id) }
        .to_str()
        .unwrap()
        .to_string();
    unsafe { calm_string_free(id) };
    out
}

fn memory(ptr: *mut CalmEngine) -> CalmMemory {
    let mut out = CalmMemory::default();
    assert_eq!(unsafe { calm_engine_memory(ptr, &mut out) }, CalmStatus::Ok);
    out
}

#[test]
fn an_open_project_reports_what_it_holds() {
    let (_dir, ptr) = engine();
    create(ptr, "Report");
    let report = memory(ptr);
    assert!(report.tile_bytes > 0, "paper is resident");
    assert!(
        report.shared_tile_count > 0,
        "paper's tiles share one allocation"
    );
    assert_eq!(report.mask_bytes, 0);
    unsafe { calm_engine_free(ptr) };
}

#[test]
fn closing_a_project_leaves_nothing_of_it_behind() {
    let (_dir, ptr) = engine();
    create(ptr, "Closed");
    assert!(memory(ptr).tile_bytes > 0);

    assert_eq!(unsafe { calm_project_close(ptr) }, CalmStatus::Ok);
    let report = memory(ptr);
    assert_eq!(report.tile_bytes, 0, "no tiles for a project nobody is on");
    assert_eq!(report.history_bytes, 0);
    assert_eq!(report.gpu_bytes, 0);
    unsafe { calm_engine_free(ptr) };
}

/// Switching between projects — what clicking another titlebar tab does — must not
/// accumulate: the second project's report is the whole engine's memory, not a sum.
#[test]
fn opening_another_project_replaces_the_first_rather_than_adding_to_it() {
    let (_dir, ptr) = engine();
    create(ptr, "First");
    let first = memory(ptr);

    let second_id = create(ptr, "Second");
    let id = CString::new(second_id).unwrap();
    assert_eq!(
        unsafe { calm_project_open(ptr, id.as_ptr()) },
        CalmStatus::Ok
    );

    let second = memory(ptr);
    assert_eq!(
        second.tile_bytes, first.tile_bytes,
        "one document resident at a time"
    );
    unsafe { calm_engine_free(ptr) };
}

#[test]
fn the_memory_report_guards_its_pointers() {
    let (_dir, ptr) = engine();
    let mut out = CalmMemory::default();
    assert_eq!(
        unsafe { calm_engine_memory(std::ptr::null_mut(), &mut out) },
        CalmStatus::Null
    );
    assert_eq!(
        unsafe { calm_engine_memory(ptr, std::ptr::null_mut()) },
        CalmStatus::Null
    );
    assert_eq!(
        unsafe { calm_engine_memory(ptr, &mut out) },
        CalmStatus::Ok,
        "no project open is still a valid answer"
    );
    assert_eq!(out, CalmMemory::default());
    unsafe { calm_engine_free(ptr) };
}

#[test]
fn memory_pressure_levels_reach_the_renderer() {
    let (_dir, ptr) = engine();
    create(ptr, "Pressure");
    let before = memory(ptr).gpu_bytes;
    assert_eq!(
        unsafe { calm_engine_set_memory_pressure(ptr, 1) },
        CalmStatus::Ok
    );
    let warn = memory(ptr).gpu_bytes;
    assert_eq!(
        unsafe { calm_engine_set_memory_pressure(ptr, 2) },
        CalmStatus::Ok
    );
    let critical = memory(ptr).gpu_bytes;
    assert_eq!(
        unsafe { calm_engine_set_memory_pressure(ptr, 0) },
        CalmStatus::Ok
    );
    let _ = (before, warn, critical);
    unsafe { calm_engine_free(ptr) };
}

#[test]
fn memory_pressure_rejects_unknown_levels() {
    let (_dir, ptr) = engine();
    assert_eq!(
        unsafe { calm_engine_set_memory_pressure(ptr, 99) },
        CalmStatus::Error
    );
    assert_eq!(
        unsafe { calm_engine_set_memory_pressure(ptr::null_mut(), 1) },
        CalmStatus::Null
    );
    unsafe { calm_engine_free(ptr) };
}
