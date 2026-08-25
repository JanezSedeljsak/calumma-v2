use calumma_core::Tool;
use calumma_ffi::*;
use std::ffi::{CStr, CString};
use std::ptr;

/// Drives the same C entry points the Swift shell calls, in the same order, so the bridge is
/// exercised end to end rather than only the Rust behind it.
struct TextEngine {
    ptr: *mut CalmEngine,
    _dir: tempfile::TempDir,
}

impl TextEngine {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = CString::new(dir.path().join("t.sqlite").to_str().unwrap()).unwrap();
        let ptr = unsafe { calm_engine_new(path.as_ptr()) };
        assert!(!ptr.is_null());
        let name = CString::new("Text").unwrap();
        let id = unsafe { calm_project_create(ptr, name.as_ptr(), 512, 512) };
        assert!(!id.is_null());
        unsafe { calm_string_free(id) };
        assert_eq!(
            unsafe { calm_engine_set_tool(ptr, Tool::Text as u32) },
            CalmStatus::Ok
        );
        Self { ptr, _dir: dir }
    }

    fn click(&self, x: f32, y: f32) {
        assert_eq!(
            unsafe { calm_engine_pointer_down(self.ptr, x, y) },
            CalmStatus::Ok
        );
    }

    fn type_text(&self, text: &str) {
        let c = CString::new(text).unwrap();
        assert_eq!(
            unsafe { calm_engine_text_insert(self.ptr, c.as_ptr()) },
            CalmStatus::Ok
        );
    }

    fn editing(&self) -> bool {
        unsafe { calm_engine_text_editing(self.ptr) == 1 }
    }

    fn layer_text(&self, index: u32) -> Option<String> {
        let raw = unsafe { calm_engine_layer_text(self.ptr, index) };
        if raw.is_null() {
            return None;
        }
        let text = unsafe { CStr::from_ptr(raw) }.to_str().unwrap().to_string();
        unsafe { calm_string_free(raw) };
        Some(text)
    }

    fn styles(&self) -> u32 {
        unsafe { calm_engine_text_styles(self.ptr) }
    }

    fn text_layer_index(&self) -> u32 {
        (0..8)
            .find(|i| unsafe { calm_engine_layer_is_text(self.ptr, *i) } == 1)
            .expect("a text layer should exist")
    }

    fn family(&self) -> String {
        let raw = unsafe { calm_engine_text_family(self.ptr) };
        assert!(!raw.is_null());
        let name = unsafe { CStr::from_ptr(raw) }.to_str().unwrap().to_string();
        unsafe { calm_string_free(raw) };
        name
    }
}

impl Drop for TextEngine {
    fn drop(&mut self) {
        unsafe { calm_engine_free(self.ptr) };
    }
}

#[test]
fn the_font_list_is_served_by_the_engine() {
    let count = calm_font_family_count();
    assert!(count > 5, "expected system fonts over FFI, got {count}");
    let first = calm_font_family_name(0);
    assert!(!first.is_null());
    let name = unsafe { CStr::from_ptr(first) }
        .to_str()
        .unwrap()
        .to_string();
    unsafe { calm_string_free(first) };
    assert!(!name.is_empty());
    assert!(
        calm_font_family_name(count).is_null(),
        "out of range is null"
    );
}

#[test]
fn text_size_bounds_come_from_core() {
    assert!(calm_text_size_min() > 0.0);
    assert!(calm_text_size_max() > calm_text_size_min());
    let default = calm_text_size_default();
    assert!(default >= calm_text_size_min() && default <= calm_text_size_max());
}

#[test]
fn click_type_and_commit_over_the_bridge() {
    let engine = TextEngine::new();
    assert!(!engine.editing());
    engine.click(200.0, 200.0);
    assert!(engine.editing());
    engine.type_text("Hello");
    assert_eq!(
        unsafe { calm_engine_text_commit(engine.ptr) },
        CalmStatus::Ok
    );
    assert!(!engine.editing());

    let index = (0..8).find(|i| unsafe { calm_engine_layer_is_text(engine.ptr, *i) } == 1);
    let index = index.expect("a text layer should exist");
    assert_eq!(engine.layer_text(index).as_deref(), Some("Hello"));
    assert!(engine.layer_text(0).is_none(), "paper carries no run");
}

#[test]
fn editing_keys_reach_the_run() {
    let engine = TextEngine::new();
    engine.click(200.0, 200.0);
    engine.type_text("abcd");
    unsafe {
        assert_eq!(
            calm_engine_text_move_caret(engine.ptr, 0),
            CalmStatus::Ok,
            "left"
        );
        assert_eq!(calm_engine_text_backspace(engine.ptr), CalmStatus::Ok);
        assert_eq!(calm_engine_text_move_caret(engine.ptr, 6), CalmStatus::Ok);
        assert_eq!(calm_engine_text_delete_forward(engine.ptr), CalmStatus::Ok);
    }
    let index = (0..8)
        .find(|i| unsafe { calm_engine_layer_is_text(engine.ptr, *i) } == 1)
        .unwrap();
    assert_eq!(engine.layer_text(index).as_deref(), Some("bd"));
}

#[test]
fn a_composition_round_trips_through_the_bridge() {
    let engine = TextEngine::new();
    engine.click(200.0, 200.0);
    let marked = CString::new("˜").unwrap();
    assert_eq!(
        unsafe { calm_engine_text_set_marked(engine.ptr, marked.as_ptr()) },
        CalmStatus::Ok
    );
    engine.type_text("ñ");
    let index = (0..8)
        .find(|i| unsafe { calm_engine_layer_is_text(engine.ptr, *i) } == 1)
        .unwrap();
    assert_eq!(engine.layer_text(index).as_deref(), Some("ñ"));
}

#[test]
fn style_setters_and_getters_agree() {
    let engine = TextEngine::new();
    engine.click(200.0, 200.0);
    engine.type_text("style");

    assert_eq!(
        unsafe { calm_engine_set_text_size(engine.ptr, 72.0) },
        CalmStatus::Ok
    );
    assert_eq!(unsafe { calm_engine_text_size(engine.ptr) }, 72.0);

    assert_eq!(
        unsafe { calm_engine_set_text_align(engine.ptr, 2) },
        CalmStatus::Ok
    );
    assert_eq!(unsafe { calm_engine_text_align(engine.ptr) }, 2);
    assert_eq!(
        unsafe { calm_engine_set_text_align(engine.ptr, 99) },
        CalmStatus::Error,
        "an unknown alignment is rejected, not silently coerced"
    );

    let first = calm_font_family_name(0);
    let wanted = unsafe { CStr::from_ptr(first) }
        .to_str()
        .unwrap()
        .to_string();
    unsafe { calm_string_free(first) };
    let c = CString::new(wanted.clone()).unwrap();
    assert_eq!(
        unsafe { calm_engine_set_text_family(engine.ptr, c.as_ptr()) },
        CalmStatus::Ok
    );
    assert_eq!(engine.family(), wanted);
}

#[test]
fn the_caret_reports_a_screen_rect_while_editing() {
    let engine = TextEngine::new();
    let mut x = 0.0f32;
    let mut y = 0.0f32;
    let mut h = 0.0f32;
    assert_eq!(
        unsafe { calm_engine_text_caret_rect(engine.ptr, &mut x, &mut y, &mut h) },
        CalmStatus::Error,
        "no caret before a session starts"
    );
    engine.click(200.0, 200.0);
    engine.type_text("caret");
    assert_eq!(
        unsafe { calm_engine_text_caret_rect(engine.ptr, &mut x, &mut y, &mut h) },
        CalmStatus::Ok
    );
    assert!(h > 0.0);
    assert!(x.is_finite() && y.is_finite());
}

#[test]
fn re_entering_a_layer_by_index_is_rejected_for_non_text() {
    let engine = TextEngine::new();
    engine.click(200.0, 200.0);
    engine.type_text("layer");
    assert_eq!(
        unsafe { calm_engine_text_commit(engine.ptr) },
        CalmStatus::Ok
    );
    let index = (0..8)
        .find(|i| unsafe { calm_engine_layer_is_text(engine.ptr, *i) } == 1)
        .unwrap();
    assert_eq!(
        unsafe { calm_engine_text_edit_layer(engine.ptr, index) },
        CalmStatus::Ok
    );
    assert!(engine.editing());
    assert_eq!(
        unsafe { calm_engine_text_edit_layer(engine.ptr, 0) },
        CalmStatus::Error
    );
}

#[test]
fn switching_tools_commits_the_session() {
    let engine = TextEngine::new();
    engine.click(200.0, 200.0);
    engine.type_text("kept");
    assert_eq!(
        unsafe { calm_engine_set_tool(engine.ptr, Tool::Pen as u32) },
        CalmStatus::Ok
    );
    assert!(!engine.editing());
    assert!((0..8).any(|i| unsafe { calm_engine_layer_is_text(engine.ptr, i) } == 1));
}

#[test]
fn the_ink_color_recolors_the_live_run() {
    let engine = TextEngine::new();
    engine.click(200.0, 200.0);
    engine.type_text("red");
    assert_eq!(
        unsafe { calm_engine_set_color(engine.ptr, 255, 0, 0, 255) },
        CalmStatus::Ok
    );
    let index = (0..8)
        .find(|i| unsafe { calm_engine_layer_is_text(engine.ptr, *i) } == 1)
        .unwrap();
    let mut rgba: *mut u8 = ptr::null_mut();
    let mut w = 0u32;
    let mut h = 0u32;
    assert_eq!(
        unsafe { calm_engine_layer_rgba(engine.ptr, index, &mut rgba, &mut w, &mut h) },
        CalmStatus::Ok
    );
    let len = (w as usize) * (h as usize) * 4;
    let pixels = unsafe { std::slice::from_raw_parts(rgba, len) }.to_vec();
    unsafe { calm_buffer_free(rgba, len) };
    let opaque = pixels.chunks_exact(4).find(|px| px[3] == 255);
    assert_eq!(opaque, Some([255u8, 0, 0, 255].as_slice()));
}

#[test]
fn text_calls_are_null_safe() {
    let text = CString::new("x").unwrap();
    unsafe {
        assert_eq!(
            calm_engine_text_insert(ptr::null_mut(), text.as_ptr()),
            CalmStatus::Null
        );
        assert_eq!(
            calm_engine_text_backspace(ptr::null_mut()),
            CalmStatus::Null
        );
        assert_eq!(calm_engine_text_commit(ptr::null_mut()), CalmStatus::Null);
        assert_eq!(calm_engine_text_editing(ptr::null_mut()), -1);
        assert_eq!(calm_engine_layer_is_text(ptr::null_mut(), 0), -1);
        assert!(calm_engine_text_family(ptr::null_mut()).is_null());
        assert!(calm_engine_layer_text(ptr::null_mut(), 0).is_null());
        assert_eq!(
            calm_engine_text_caret_rect(
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut()
            ),
            CalmStatus::Null
        );
    }
}

/// No project open is a normal state (the landing screen), not a crash.
#[test]
fn text_calls_without_a_project_are_errors_not_panics() {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("t.sqlite").to_str().unwrap()).unwrap();
    let engine = unsafe { calm_engine_new(path.as_ptr()) };
    let text = CString::new("x").unwrap();
    unsafe {
        assert_eq!(
            calm_engine_text_insert(engine, text.as_ptr()),
            CalmStatus::Error
        );
        assert_eq!(calm_engine_text_editing(engine), -1);
        assert_eq!(calm_engine_text_size(engine), calm_text_size_default());
        calm_engine_free(engine);
    }
}

#[test]
fn every_font_row_reports_the_cuts_it_ships() {
    let count = calm_font_family_count();
    let styled = (0..count)
        .filter(|i| calm_font_family_styles(*i) != 0)
        .count();
    assert!(styled > 0, "no family reported a bold or italic cut");
    assert_eq!(
        calm_font_family_styles(count),
        0,
        "out of range reports no cuts"
    );
}

#[test]
fn line_height_bounds_come_from_core() {
    assert!(calm_text_line_height_min() > 0.0);
    assert!(calm_text_line_height_max() > calm_text_line_height_min());
    let default = calm_text_line_height_default();
    assert!(default >= calm_text_line_height_min());
    assert!(default <= calm_text_line_height_max());
}

#[test]
fn bold_italic_and_line_height_round_trip_over_the_bridge() {
    let engine = TextEngine::new();
    engine.click(200.0, 200.0);
    engine.type_text("styled");
    assert_eq!(engine.styles(), 0);

    unsafe {
        assert_eq!(calm_engine_set_text_bold(engine.ptr, 1), CalmStatus::Ok);
        assert_eq!(calm_engine_set_text_italic(engine.ptr, 1), CalmStatus::Ok);
        assert_eq!(
            calm_engine_set_text_line_height(engine.ptr, 2.0),
            CalmStatus::Ok
        );
    }
    assert_eq!(engine.styles(), 3, "bold and italic are bits 1 and 2");
    assert_eq!(unsafe { calm_engine_text_line_height(engine.ptr) }, 2.0);

    unsafe {
        assert_eq!(calm_engine_set_text_bold(engine.ptr, 0), CalmStatus::Ok);
        assert_eq!(
            calm_engine_set_text_line_height(engine.ptr, 999.0),
            CalmStatus::Ok
        );
    }
    assert_eq!(engine.styles(), 2);
    assert_eq!(
        unsafe { calm_engine_text_line_height(engine.ptr) },
        calm_text_line_height_max(),
        "the engine clamps, the shell does not"
    );
}

#[test]
fn an_unknown_family_is_rejected_by_the_engine() {
    let engine = TextEngine::new();
    engine.click(200.0, 200.0);
    engine.type_text("font");
    let before = engine.family();

    let missing = CString::new("Definitely Not Installed").unwrap();
    assert_eq!(
        unsafe { calm_engine_set_text_family(engine.ptr, missing.as_ptr()) },
        CalmStatus::Error,
        "a family the engine cannot shape is refused, not stored"
    );
    assert_eq!(engine.family(), before);
}

#[test]
fn a_text_layer_is_rasterized_on_request() {
    let engine = TextEngine::new();
    engine.click(200.0, 200.0);
    engine.type_text("pixels");
    let index = engine.text_layer_index();
    assert_eq!(
        calm_engine_rasterize_layer(engine.ptr, index),
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_layer_is_text(engine.ptr, index) },
        0,
        "the layer is ordinary pixels now"
    );
    assert!(engine.layer_text(index).is_none());
    assert_eq!(
        calm_engine_rasterize_layer(engine.ptr, index),
        CalmStatus::Error,
        "there is nothing left to rasterize"
    );
}

#[test]
fn the_new_text_calls_are_null_safe() {
    unsafe {
        assert_eq!(
            calm_engine_set_text_bold(ptr::null_mut(), 1),
            CalmStatus::Null
        );
        assert_eq!(
            calm_engine_set_text_italic(ptr::null_mut(), 1),
            CalmStatus::Null
        );
        assert_eq!(
            calm_engine_set_text_line_height(ptr::null_mut(), 1.5),
            CalmStatus::Null
        );
        assert_eq!(
            calm_engine_rasterize_layer(ptr::null_mut(), 0),
            CalmStatus::Null
        );
        assert_eq!(calm_engine_text_styles(ptr::null_mut()), 0);
        assert_eq!(
            calm_engine_text_line_height(ptr::null_mut()),
            calm_text_line_height_default()
        );
    }
}
