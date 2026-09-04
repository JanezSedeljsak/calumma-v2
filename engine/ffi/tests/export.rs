//! Export entry points the menu bar calls.

use calumma_core::Tool;
use calumma_ffi::*;
use calumma_io::PDF_DEFAULT_DPI;
use std::ffi::CString;
use std::ptr;

fn engine_with_paint() -> (tempfile::TempDir, *mut CalmEngine) {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("export.sqlite").to_str().unwrap()).unwrap();
    let ptr = unsafe { calm_engine_new(path.as_ptr()) };
    assert!(!ptr.is_null());
    let name = CString::new("Export").unwrap();
    let id = unsafe { calm_project_create(ptr, name.as_ptr(), 32, 32) };
    assert!(!id.is_null());
    unsafe { calm_string_free(id) };
    unsafe {
        assert_eq!(calm_engine_set_tool(ptr, Tool::Pen as u32), CalmStatus::Ok);
        assert_eq!(calm_engine_pointer_down(ptr, 4.0, 4.0), CalmStatus::Ok);
        assert_eq!(calm_engine_pointer_move(ptr, 20.0, 20.0), CalmStatus::Ok);
        assert_eq!(calm_engine_pointer_up(ptr, 20.0, 20.0), CalmStatus::Ok);
    }
    (dir, ptr)
}

#[test]
fn pdf_export_round_trips_through_the_ffi() {
    let (_dir, e) = engine_with_paint();
    let mut bytes: *mut u8 = ptr::null_mut();
    let mut len = 0usize;
    assert_eq!(
        unsafe { calm_engine_export_pdf(e, PDF_DEFAULT_DPI, &mut bytes, &mut len) },
        CalmStatus::Ok
    );
    assert!(len > 32);
    let head = unsafe { std::slice::from_raw_parts(bytes, 5) };
    assert_eq!(head, b"%PDF-");
    unsafe { calm_buffer_free(bytes, len) };
    unsafe { calm_engine_free(e) };
}

#[test]
fn pdf_export_guards_its_pointers() {
    let (_dir, e) = engine_with_paint();
    let mut bytes: *mut u8 = ptr::null_mut();
    let mut len = 0usize;
    assert_eq!(
        unsafe { calm_engine_export_pdf(ptr::null_mut(), 72.0, &mut bytes, &mut len) },
        CalmStatus::Null
    );
    assert_eq!(
        unsafe { calm_engine_export_pdf(e, 72.0, ptr::null_mut(), &mut len) },
        CalmStatus::Null
    );
    assert_eq!(
        unsafe { calm_engine_export_pdf(e, 72.0, &mut bytes, ptr::null_mut()) },
        CalmStatus::Null
    );
    unsafe { calm_engine_free(e) };
}

#[test]
fn pdf_default_dpi_is_exposed_for_the_shell() {
    assert!((calm_pdf_default_dpi() - PDF_DEFAULT_DPI).abs() < f32::EPSILON);
}
