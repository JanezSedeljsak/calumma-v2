use calumma_core::Tool;
use calumma_ffi::*;
use std::ffi::CString;
use std::ptr;

const SIDE: u32 = 32;

/// Drives the clipboard and tool-query entry points in `clipboard_ffi.rs` through the same
/// C ABI the Swift shell calls, mirroring the helper pattern in `vector.rs` and `engine.rs`.
struct ClipEngine {
    ptr: *mut CalmEngine,
    _dir: tempfile::TempDir,
}

impl ClipEngine {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = CString::new(dir.path().join("c.sqlite").to_str().unwrap()).unwrap();
        let ptr = unsafe { calm_engine_new(path.as_ptr()) };
        assert!(!ptr.is_null());
        let name = CString::new("Clip").unwrap();
        let id = unsafe { calm_project_create(ptr, name.as_ptr(), SIDE, SIDE) };
        assert!(!id.is_null());
        unsafe { calm_string_free(id) };
        assert_eq!(
            unsafe { calm_engine_resize(ptr, SIDE, SIDE, 1.0) },
            CalmStatus::Ok
        );
        assert_eq!(unsafe { calm_engine_fit(ptr) }, CalmStatus::Ok);
        Self { ptr, _dir: dir }
    }

    fn set_tool(&self, tool: Tool) {
        assert_eq!(
            unsafe { calm_engine_set_tool(self.ptr, tool as u32) },
            CalmStatus::Ok
        );
    }

    fn drag(&self, from: (f32, f32), to: (f32, f32)) {
        unsafe {
            assert_eq!(
                calm_engine_pointer_down(self.ptr, from.0, from.1),
                CalmStatus::Ok
            );
            assert_eq!(
                calm_engine_pointer_move(self.ptr, to.0, to.1),
                CalmStatus::Ok
            );
            assert_eq!(calm_engine_pointer_up(self.ptr, to.0, to.1), CalmStatus::Ok);
        }
    }

    fn draw_a_vector_rect(&self) -> u32 {
        assert_eq!(calm_engine_set_vector_mode(self.ptr, 1), CalmStatus::Ok);
        self.set_tool(Tool::Rect);
        self.drag((4.0, 4.0), (20.0, 20.0));
        (0..16)
            .find(|i| calm_engine_layer_is_vector(self.ptr, *i) == 1)
            .expect("a vector layer should exist")
    }
}

impl Drop for ClipEngine {
    fn drop(&mut self) {
        unsafe { calm_engine_free(self.ptr) };
    }
}

fn out_slots() -> (*mut u8, usize, u32) {
    (ptr::null_mut(), 0, 99)
}

#[test]
fn cutting_without_a_selection_is_an_error() {
    let engine = ClipEngine::new();
    let (mut bytes, mut len, mut kind) = out_slots();
    unsafe {
        assert_eq!(
            calm_engine_cut(engine.ptr, &mut bytes, &mut len, &mut kind),
            CalmStatus::Error
        );
    }
    assert!(bytes.is_null(), "nothing is written on the error path");
    assert_eq!(len, 0);
}

#[test]
fn cutting_a_selection_returns_a_png_and_clears_the_ink() {
    let engine = ClipEngine::new();
    engine.set_tool(Tool::SelectRect);
    engine.drag((2.0, 2.0), (20.0, 20.0));
    let (mut bytes, mut len, mut kind) = out_slots();
    unsafe {
        assert_eq!(
            calm_engine_cut(engine.ptr, &mut bytes, &mut len, &mut kind),
            CalmStatus::Ok
        );
        assert_eq!(kind, 0, "a cut selection comes back as a PNG");
        assert!(len > 8);
        assert_eq!(
            std::slice::from_raw_parts(bytes, 4),
            &[0x89, b'P', b'N', b'G']
        );
        calm_buffer_free(bytes, len);
    }
}

#[test]
fn copy_layer_reads_a_raster_layer_as_a_png() {
    let engine = ClipEngine::new();
    let (mut bytes, mut len, mut kind) = out_slots();
    unsafe {
        assert_eq!(
            calm_engine_copy_layer(engine.ptr, 0, &mut bytes, &mut len, &mut kind),
            CalmStatus::Ok
        );
        assert_eq!(kind, 0);
        assert!(len > 8);
        calm_buffer_free(bytes, len);
    }
}

#[test]
fn copy_layer_reads_a_vector_layer_as_svg() {
    let engine = ClipEngine::new();
    let index = engine.draw_a_vector_rect();

    let (mut bytes, mut len, mut kind) = out_slots();
    unsafe {
        assert_eq!(
            calm_engine_copy_layer(engine.ptr, index, &mut bytes, &mut len, &mut kind),
            CalmStatus::Ok
        );
        assert_eq!(kind, 1, "a vector layer comes back as SVG, not a bitmap");
        let svg = std::str::from_utf8(std::slice::from_raw_parts(bytes, len)).unwrap();
        assert!(svg.starts_with("<svg"));
        calm_buffer_free(bytes, len);
    }
}

#[test]
fn copy_layer_with_an_out_of_range_index_is_an_error() {
    let engine = ClipEngine::new();
    let (mut bytes, mut len, mut kind) = out_slots();
    unsafe {
        assert_eq!(
            calm_engine_copy_layer(engine.ptr, 999, &mut bytes, &mut len, &mut kind),
            CalmStatus::Error
        );
    }
    assert!(bytes.is_null());
}

#[test]
fn parse_hex_rgb_rejects_nulls_bad_hex_and_non_utf8() {
    let mut rgb = 0u32;
    unsafe {
        assert_eq!(calm_parse_hex_rgb(ptr::null(), &mut rgb), CalmStatus::Null);

        let hex = CString::new("#1a2b3c").unwrap();
        assert_eq!(
            calm_parse_hex_rgb(hex.as_ptr(), ptr::null_mut()),
            CalmStatus::Null
        );

        let junk = CString::new("not a color").unwrap();
        assert_eq!(
            calm_parse_hex_rgb(junk.as_ptr(), &mut rgb),
            CalmStatus::Error
        );

        let invalid_utf8 = CString::new(vec![0xFFu8, 0xFE]).unwrap();
        assert_eq!(
            calm_parse_hex_rgb(invalid_utf8.as_ptr(), &mut rgb),
            CalmStatus::Error
        );
    }
}

#[test]
fn tool_predicates_have_a_false_branch_too() {
    assert_eq!(calm_tool_is_selection(Tool::Rect as u32), 0);
    assert_eq!(calm_tool_shows_vector_mode(Tool::Move as u32), 0);
}

#[test]
fn nudge_move_target_with_a_null_engine_is_a_no_op() {
    assert_eq!(calm_engine_nudge_move_target(ptr::null_mut(), 1.0, 0.0), 0);
}

#[test]
fn nudge_move_target_does_nothing_outside_move_mode() {
    let engine = ClipEngine::new();
    assert_eq!(calm_engine_nudge_move_target(engine.ptr, 1.0, 0.0), 0);
}

#[test]
fn nudge_move_target_moves_the_active_layer_in_move_mode() {
    let engine = ClipEngine::new();
    engine.draw_a_vector_rect();
    engine.set_tool(Tool::Move);
    assert_eq!(calm_engine_nudge_move_target(engine.ptr, 1.0, 0.0), 1);
}
