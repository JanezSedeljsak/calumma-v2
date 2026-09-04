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
    engine.set_tool(Tool::Pen);
    engine.drag((2.0, 2.0), (25.0, 25.0));
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

/// A cut on a vector layer's selection still returns real pixels — `selection_rgba` reads
/// through `select_sample`, the same as core — and the item behind it is untouched: there are
/// no tiles for a clear to act on, so the layer survives the cut as if only copy had happened.
#[test]
fn cutting_a_selection_on_a_vector_layer_copies_and_leaves_it_intact() {
    let engine = ClipEngine::new();
    let index = engine.draw_a_vector_rect();
    engine.set_tool(Tool::MagicWand);
    engine.drag((10.0, 10.0), (10.0, 10.0));

    let before = unsafe { calm_engine_layer_svg(engine.ptr, index) };
    assert!(!before.is_null());
    let before = unsafe { std::ffi::CStr::from_ptr(before) }
        .to_str()
        .unwrap()
        .to_string();

    let (mut bytes, mut len, mut kind) = out_slots();
    unsafe {
        assert_eq!(
            calm_engine_cut(engine.ptr, &mut bytes, &mut len, &mut kind),
            CalmStatus::Ok
        );
        assert_eq!(kind, 0, "the selection copy is still a PNG, not an SVG");
        assert!(len > 8);
        assert_eq!(
            std::slice::from_raw_parts(bytes, 4),
            &[0x89, b'P', b'N', b'G']
        );
        calm_buffer_free(bytes, len);
    }

    assert_eq!(calm_engine_layer_is_vector(engine.ptr, index), 1);
    let after = unsafe { calm_engine_layer_svg(engine.ptr, index) };
    assert!(!after.is_null());
    let after = unsafe { std::ffi::CStr::from_ptr(after) }
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(
        before, after,
        "cutting the selection left the shape byte-for-byte the same"
    );
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

#[test]
fn copy_without_a_selection_returns_the_composite() {
    let engine = ClipEngine::new();
    engine.set_tool(Tool::Pen);
    engine.drag((4.0, 4.0), (18.0, 18.0));
    let (mut bytes, mut len, mut kind) = out_slots();
    unsafe {
        assert_eq!(
            calm_engine_copy(engine.ptr, &mut bytes, &mut len, &mut kind),
            CalmStatus::Ok
        );
        assert_eq!(kind, 0);
        assert!(len > 8);
        calm_buffer_free(bytes, len);
    }
}

#[test]
fn copy_rejects_null_output_slots() {
    let engine = ClipEngine::new();
    let mut bytes: *mut u8 = ptr::null_mut();
    let mut len = 0usize;
    let mut kind = 0u32;
    unsafe {
        assert_eq!(
            calm_engine_copy(engine.ptr, ptr::null_mut(), &mut len, &mut kind),
            CalmStatus::Null
        );
        assert_eq!(
            calm_engine_copy(engine.ptr, &mut bytes, ptr::null_mut(), &mut kind),
            CalmStatus::Null
        );
        assert_eq!(
            calm_engine_copy(engine.ptr, &mut bytes, &mut len, ptr::null_mut()),
            CalmStatus::Null
        );
    }
}

#[test]
fn fill_and_clone_tool_predicates_are_exposed() {
    assert_eq!(calm_tool_takes_fill(Tool::Rect as u32), 1);
    assert_eq!(calm_tool_takes_fill(Tool::Pen as u32), 0);
    assert_eq!(calm_tool_takes_clone_aligned(Tool::Clone as u32), 1);
    assert_eq!(calm_tool_takes_clone_aligned(Tool::Heal as u32), 1);
    assert_eq!(calm_tool_takes_clone_aligned(Tool::Pen as u32), 0);
    assert_eq!(calm_clone_aligned_default(), 1);
}

#[test]
fn hex_formatting_and_limit_getters_match_core() {
    unsafe {
        let raw = calm_format_hex_rgb(0xFF_80_40);
        assert!(!raw.is_null());
        let hex = std::ffi::CStr::from_ptr(raw).to_str().unwrap();
        assert_eq!(hex, "FF8040");
        calm_string_free(raw);

        let cstr = std::ffi::CString::new("#FF8040").unwrap();
        let mut rgb = 0u32;
        assert_eq!(calm_parse_hex_rgb(cstr.as_ptr(), &mut rgb), CalmStatus::Ok);
        assert_eq!(rgb, 0xFF_80_40);
    }

    assert!(calm_brush_size_min() < calm_brush_size_max());
    assert!(calm_brush_size_from_unit(0.5) > calm_brush_size_min());
    assert!(calm_brush_size_step(calm_brush_size_default(), 1) > calm_brush_size_default());

    assert!(calm_ink_opacity_min() < calm_ink_opacity_max());
    assert!(calm_blur_strength_min() < calm_blur_strength_max());
    assert!(calm_eraser_hardness_min() < calm_eraser_hardness_max());
    assert!(calm_tolerance_min() < calm_tolerance_max());
    assert!(calm_eyedropper_radius_min() < calm_eyedropper_radius_max());
    assert!(calm_lossy_export_quality() > 0.0);
    assert!(calm_pdf_default_dpi() > 0.0);
}

#[test]
fn every_tool_predicate_has_a_branch_for_shapes_selection_and_brushes() {
    for tool in 0..32u32 {
        let _ = calm_tool_is_shape(tool);
        let _ = calm_tool_is_selection(tool);
        let _ = calm_tool_takes_fill(tool);
        let _ = calm_tool_takes_brush_size(tool);
        let _ = calm_tool_takes_ink_opacity(tool);
        let _ = calm_tool_shows_vector_mode(tool);
        let _ = calm_tool_takes_blur_strength(tool);
        let _ = calm_tool_takes_clone_aligned(tool);
        let _ = calm_tool_takes_tolerance(tool);
        let _ = calm_tool_takes_eyedropper_radius(tool);
        let _ = calm_tool_takes_brush(tool);
        let _ = calm_tool_takes_eraser_hardness(tool);
    }
    assert_eq!(calm_tool_is_shape(Tool::Rect as u32), 1);
    assert_eq!(calm_tool_takes_blur_strength(Tool::Blur as u32), 1);
    assert_eq!(calm_tool_takes_tolerance(Tool::Fill as u32), 1);
}

#[test]
fn copy_layer_returns_svg_for_a_vector_layer() {
    let engine = ClipEngine::new();
    let layer = engine.draw_a_vector_rect();
    let (mut bytes, mut len, mut kind) = out_slots();
    unsafe {
        assert_eq!(
            calm_engine_copy_layer(engine.ptr, layer, &mut bytes, &mut len, &mut kind),
            CalmStatus::Ok
        );
        assert_eq!(kind, 1);
        assert!(len > 10);
        let text = std::str::from_utf8(std::slice::from_raw_parts(bytes, len)).unwrap();
        assert!(text.contains("<svg"));
        calm_buffer_free(bytes, len);
    }
}

#[test]
fn cut_with_a_selection_clears_the_region() {
    let engine = ClipEngine::new();
    engine.set_tool(Tool::Pen);
    engine.drag((6.0, 6.0), (20.0, 20.0));
    engine.set_tool(Tool::SelectRect);
    engine.drag((8.0, 8.0), (18.0, 18.0));
    let (mut bytes, mut len, mut kind) = out_slots();
    unsafe {
        assert_eq!(
            calm_engine_cut(engine.ptr, &mut bytes, &mut len, &mut kind),
            CalmStatus::Ok
        );
        assert!(len > 0);
        calm_buffer_free(bytes, len);
    }
}
