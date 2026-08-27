use calumma_core::{unpremultiply_rgba, AdjustmentKind, BlendMode, Tool, IMPORT_MAX_SIDE};
use calumma_ffi::*;
use std::ffi::{CStr, CString};
use std::os::raw::c_int;
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

    fn state(&self) -> CalmState {
        let mut out = CalmState {
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
            is_fit: 0,
            transform_active: 0,
        };
        let status = unsafe { calm_engine_state(self.ptr, &mut out) };
        assert_eq!(status, CalmStatus::Ok);
        out
    }
}

impl Drop for TestEngine {
    fn drop(&mut self) {
        unsafe { calm_engine_free(self.ptr) };
    }
}

fn take_buffer(ptr: *mut u8, len: usize) -> Vec<u8> {
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec();
    unsafe { calm_buffer_free(ptr, len) };
    slice
}

#[test]
fn engine_new_and_free_round_trips() {
    let engine = TestEngine::new();
    assert!(!engine.ptr.is_null());
}

#[test]
fn engine_new_with_null_path_uses_default() {
    let ptr = unsafe { calm_engine_new(ptr::null()) };
    assert!(!ptr.is_null());
    unsafe { calm_engine_free(ptr) };
}

#[test]
fn palette_and_import_limits_are_exposed() {
    assert!(calm_palette_count() > 0);
    let _ = calm_palette_color(0);
    assert!(calm_import_max_side() > 0);
}

#[test]
fn autosave_persists_off_the_render_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.sqlite");
    let path_c = CString::new(path.to_str().unwrap()).unwrap();

    let writer = unsafe { calm_engine_new(path_c.as_ptr()) };
    assert!(!writer.is_null());
    let id_ptr = unsafe { calm_project_create(writer, c"autosave".as_ptr(), 64, 64) };
    let id = unsafe { CStr::from_ptr(id_ptr) }
        .to_str()
        .unwrap()
        .to_string();
    unsafe { calm_string_free(id_ptr) };

    let status = unsafe { calm_engine_resize_document(writer, 96, 128) };
    assert_eq!(status, CalmStatus::Ok);

    std::thread::sleep(std::time::Duration::from_millis(
        calumma_core::limits::AUTOSAVE_INTERVAL_MS * 3,
    ));

    let reader = unsafe { calm_engine_new(path_c.as_ptr()) };
    assert!(!reader.is_null());
    let id_c = CString::new(id).unwrap();
    let status = unsafe { calm_project_open(reader, id_c.as_ptr()) };
    assert_eq!(status, CalmStatus::Ok);
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
        is_fit: 0,
        transform_active: 0,
    };
    let status = unsafe { calm_engine_state(reader, &mut state) };
    assert_eq!(status, CalmStatus::Ok);
    assert_eq!(state.width, 96);
    assert_eq!(state.height, 128);

    unsafe { calm_engine_free(reader) };
    unsafe { calm_engine_free(writer) };
}

#[test]
fn project_create_open_close_save_round_trip() {
    let engine = TestEngine::new();
    let id = engine.create_project("Demo", 64, 64);
    let state = engine.state();
    assert_eq!(state.width, 64);
    assert_eq!(state.height, 64);
    assert_eq!(unsafe { calm_project_save(engine.ptr) }, CalmStatus::Ok);
    assert_eq!(unsafe { calm_project_close(engine.ptr) }, CalmStatus::Ok);
    let id_c = CString::new(id.clone()).unwrap();
    assert_eq!(
        unsafe { calm_project_open(engine.ptr, id_c.as_ptr()) },
        CalmStatus::Ok
    );
    assert_eq!(engine.state().width, 64);
}

#[test]
fn project_rename_and_accent() {
    let engine = TestEngine::new();
    let id = engine.create_project("Old Name", 32, 32);
    let id_c = CString::new(id.clone()).unwrap();
    let new_name = CString::new("New Name").unwrap();
    assert_eq!(
        unsafe { calm_project_rename(engine.ptr, id_c.as_ptr(), new_name.as_ptr()) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_project_set_accent(engine.ptr, id_c.as_ptr(), 0x112233) },
        CalmStatus::Ok
    );
    let empty = CString::new("").unwrap();
    assert_eq!(
        unsafe { calm_project_rename(engine.ptr, id_c.as_ptr(), empty.as_ptr()) },
        CalmStatus::Error
    );
}

#[test]
fn project_list_and_delete() {
    let engine = TestEngine::new();
    let id = engine.create_project("Listed", 16, 16);
    let mut buf: Vec<CalmProjectInfo> = (0..8)
        .map(|_| CalmProjectInfo {
            id: ptr::null_mut(),
            name: ptr::null_mut(),
            width: 0,
            height: 0,
            opened_at: 0,
            accent: 0,
        })
        .collect();
    let count = unsafe { calm_project_list(engine.ptr, buf.as_mut_ptr(), buf.len()) };
    assert!(count >= 1);
    for item in buf.iter().take(count) {
        assert!(!item.id.is_null());
        unsafe { calm_string_free(item.id) };
        unsafe { calm_string_free(item.name) };
    }
    let id_c = CString::new(id).unwrap();
    assert_eq!(
        unsafe { calm_project_delete(engine.ptr, id_c.as_ptr()) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_project_delete(engine.ptr, id_c.as_ptr()) },
        CalmStatus::Error
    );
}

#[test]
fn project_create_from_image_round_trips() {
    let engine = TestEngine::new();
    let w = 4u32;
    let h = 2u32;
    let rgba = [10u8, 20, 30, 255].repeat((w * h) as usize);
    let name = CString::new("Imported").unwrap();
    let id_ptr = unsafe {
        calm_project_create_from_image(engine.ptr, name.as_ptr(), w, h, rgba.as_ptr(), rgba.len())
    };
    assert!(!id_ptr.is_null());
    unsafe { calm_string_free(id_ptr) };
    assert_eq!(engine.state().width, w);
}

#[test]
fn project_create_from_image_rejects_oversized() {
    let engine = TestEngine::new();
    let name = CString::new("TooBig").unwrap();
    let over = IMPORT_MAX_SIDE + 1;
    let id_ptr = unsafe {
        calm_project_create_from_image(engine.ptr, name.as_ptr(), over, over, ptr::null(), 0)
    };
    assert!(id_ptr.is_null());
}

#[test]
fn painting_pointer_lifecycle_and_undo_redo() {
    let engine = TestEngine::new();
    engine.create_project("Paint", 128, 128);
    assert_eq!(
        unsafe { calm_engine_set_tool(engine.ptr, Tool::Pen as u32) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_set_color(engine.ptr, 200, 30, 30, 255) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_set_brush(engine.ptr, 6.0) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_set_ink_opacity(engine.ptr, 0.5) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_set_fill(engine.ptr, 1) },
        CalmStatus::Ok
    );
    assert_eq!(unsafe { calm_engine_fit(engine.ptr) }, CalmStatus::Ok);
    assert_eq!(
        unsafe { calm_engine_resize(engine.ptr, 128, 128, 1.0) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_pointer_down(engine.ptr, 20.0, 20.0) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_pointer_move(engine.ptr, 30.0, 30.0) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_pointer_up(engine.ptr, 30.0, 30.0) },
        CalmStatus::Ok
    );
    assert!(engine.state().can_undo != 0);
    assert_eq!(unsafe { calm_engine_undo(engine.ptr) }, CalmStatus::Ok);
    assert_eq!(unsafe { calm_engine_redo(engine.ptr) }, CalmStatus::Ok);
    assert_eq!(
        unsafe { calm_engine_clear_layer(engine.ptr) },
        CalmStatus::Ok
    );
    assert_eq!(unsafe { calm_engine_render(engine.ptr) }, CalmStatus::Ok);
}

#[test]
fn camera_pan_zoom_and_board_colors() {
    let engine = TestEngine::new();
    engine.create_project("Cam", 256, 256);
    assert_eq!(
        unsafe { calm_engine_pan(engine.ptr, 5.0, 5.0) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_zoom(engine.ptr, 10.0, 10.0, 1.1) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_set_zoom(engine.ptr, 1.0) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_step_zoom(engine.ptr, 1) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_step_zoom(engine.ptr, 0) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_set_zoom_unit(engine.ptr, 0.5) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_set_board_colors(engine.ptr, 0xFF0000FF, 0x00FF00FF, 0x0000FFFF) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_set_dark(engine.ptr, 0) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_set_shift(engine.ptr, 1) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_resize_document(engine.ptr, 300, 200) },
        CalmStatus::Ok
    );
    assert_eq!(engine.state().width, 300);
}

#[test]
fn layer_crud_opacity_blend_and_adjustments() {
    let engine = TestEngine::new();
    engine.create_project("Layers", 64, 64);
    assert_eq!(unsafe { calm_engine_add_layer(engine.ptr) }, CalmStatus::Ok);
    let active = engine.state().active_layer;
    assert_eq!(
        unsafe { calm_engine_set_active_layer(engine.ptr, active) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_set_layer_visible(engine.ptr, active, 0) },
        CalmStatus::Ok
    );
    assert_eq!(unsafe { calm_engine_layer_visible(engine.ptr, active) }, 0);
    assert_eq!(
        unsafe { calm_engine_set_layer_visible(engine.ptr, active, 1) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_set_hover_layer(engine.ptr, active as c_int) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_set_hover_layer(engine.ptr, -1) },
        CalmStatus::Ok
    );

    assert_eq!(
        unsafe { calm_engine_set_layer_opacity(engine.ptr, active, 0.5) },
        CalmStatus::Ok
    );
    assert!((unsafe { calm_engine_layer_opacity(engine.ptr, active) } - 0.5).abs() < 1e-4);

    assert_eq!(
        unsafe {
            calm_engine_set_layer_blend_mode(engine.ptr, active, BlendMode::Multiply.as_u32())
        },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_layer_blend_mode(engine.ptr, active) },
        BlendMode::Multiply.as_u32()
    );

    assert_eq!(
        unsafe { calm_engine_set_layer_adjustments(engine.ptr, active, 0.1, 0.2, 0.0, 0.0, 1.5) },
        CalmStatus::Ok
    );
    let mut adj = CalmAdjustments {
        brightness: 0.0,
        contrast: 0.0,
        vibrance: 0.0,
        saturation: 0.0,
        levels_gamma: 0.0,
    };
    assert_eq!(
        unsafe { calm_engine_layer_adjustments(engine.ptr, active, &mut adj) },
        CalmStatus::Ok
    );
    assert!((adj.brightness - 0.1).abs() < 1e-4);
    assert!((adj.levels_gamma - 1.5).abs() < 1e-4);

    assert_eq!(
        unsafe { calm_engine_reset_layer_transform(engine.ptr, active) },
        CalmStatus::Ok
    );

    let name_ptr = unsafe { calm_engine_layer_name(engine.ptr, active) };
    assert!(!name_ptr.is_null());
    unsafe { calm_string_free(name_ptr) };

    let mut thumb_ptr: *mut u8 = ptr::null_mut();
    let mut tw = 0u32;
    let mut th = 0u32;
    assert_eq!(
        unsafe {
            calm_engine_layer_thumbnail(engine.ptr, active, 32, &mut thumb_ptr, &mut tw, &mut th)
        },
        CalmStatus::Ok
    );
    assert!(!thumb_ptr.is_null());
    let _ = take_buffer(thumb_ptr, (tw * th * 4) as usize);

    assert_eq!(
        unsafe { calm_engine_duplicate_layer(engine.ptr, active) },
        CalmStatus::Ok
    );
    let duplicated = engine.state().active_layer;
    assert_eq!(
        calm_engine_merge_layer_down(engine.ptr, duplicated),
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_remove_layer(engine.ptr, 999) },
        CalmStatus::Error
    );
}

#[test]
fn selection_copy_cut_paste() {
    let engine = TestEngine::new();
    engine.create_project("Selection", 64, 64);
    assert_eq!(
        unsafe { calm_engine_set_tool(engine.ptr, Tool::SelectRect as u32) },
        CalmStatus::Ok
    );
    assert_eq!(unsafe { calm_engine_fit(engine.ptr) }, CalmStatus::Ok);
    assert_eq!(
        unsafe { calm_engine_resize(engine.ptr, 64, 64, 1.0) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_pointer_down(engine.ptr, 5.0, 5.0) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_pointer_move(engine.ptr, 20.0, 20.0) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_pointer_up(engine.ptr, 20.0, 20.0) },
        CalmStatus::Ok
    );
    assert_eq!(unsafe { calm_engine_has_selection(engine.ptr) }, 1);

    let mut sel_ptr: *mut u8 = ptr::null_mut();
    let mut sw = 0u32;
    let mut sh = 0u32;
    assert_eq!(
        unsafe { calm_engine_selection_rgba(engine.ptr, &mut sel_ptr, &mut sw, &mut sh) },
        CalmStatus::Ok
    );
    assert!(!sel_ptr.is_null());
    let _ = take_buffer(sel_ptr, (sw * sh * 4) as usize);

    assert_eq!(
        unsafe { calm_engine_selection_clear_pixels(engine.ptr) },
        CalmStatus::Ok
    );
    assert_eq!(unsafe { calm_engine_deselect(engine.ptr) }, CalmStatus::Ok);
    assert_eq!(unsafe { calm_engine_has_selection(engine.ptr) }, 0);

    let w = 2u32;
    let h = 2u32;
    let mut rgba = [9u8, 8, 7, 255].repeat((w * h) as usize);
    unpremultiply_rgba(&mut rgba);
    assert_eq!(
        unsafe {
            calm_engine_paste_image(engine.ptr, rgba.as_ptr(), rgba.len(), w, h, ptr::null_mut())
        },
        CalmStatus::Ok
    );
}

#[test]
fn export_composite_layer_and_psd() {
    let engine = TestEngine::new();
    engine.create_project("Export", 8, 8);

    let mut comp_ptr: *mut u8 = ptr::null_mut();
    let mut cw = 0u32;
    let mut ch = 0u32;
    assert_eq!(
        unsafe { calm_engine_composite_rgba(engine.ptr, &mut comp_ptr, &mut cw, &mut ch) },
        CalmStatus::Ok
    );
    assert_eq!((cw, ch), (8, 8));
    let _ = take_buffer(comp_ptr, (cw * ch * 4) as usize);

    let active = engine.state().active_layer;
    let mut layer_ptr: *mut u8 = ptr::null_mut();
    let mut lw = 0u32;
    let mut lh = 0u32;
    assert_eq!(
        unsafe { calm_engine_layer_rgba(engine.ptr, active, &mut layer_ptr, &mut lw, &mut lh) },
        CalmStatus::Ok
    );
    let _ = take_buffer(layer_ptr, (lw * lh * 4) as usize);

    let svg_ptr = unsafe { calm_engine_layer_svg(engine.ptr, active) };
    assert!(svg_ptr.is_null());

    let mut psd_ptr: *mut u8 = ptr::null_mut();
    let mut psd_len: usize = 0;
    assert_eq!(
        unsafe { calm_engine_export_psd(engine.ptr, &mut psd_ptr, &mut psd_len) },
        CalmStatus::Ok
    );
    assert!(psd_len > 0);
    let bytes = take_buffer(psd_ptr, psd_len);
    assert_eq!(&bytes[0..4], b"8BPS");
}

extern "C" fn fake_op_available(_kind: CalmOpKind) -> bool {
    true
}

extern "C" fn fake_op_run(
    _kind: CalmOpKind,
    input: *const CalmOpInput,
    out: *mut CalmOpOutput,
) -> c_int {
    unsafe {
        let inp = &*input;
        let len = (inp.w * inp.h * 4) as usize;
        let mut buf = vec![0u8; len];
        std::ptr::copy_nonoverlapping(inp.rgba, buf.as_mut_ptr(), len);
        let boxed = buf.into_boxed_slice();
        let data = Box::into_raw(boxed) as *mut u8;
        *out = CalmOpOutput {
            kind: CalmOpOutputKind::Raster,
            data,
            len,
            w: inp.w,
            h: inp.h,
        };
    }
    0
}

extern "C" fn fake_op_free(out: *mut CalmOpOutput) {
    unsafe {
        if !(*out).data.is_null() {
            let slice = std::ptr::slice_from_raw_parts_mut((*out).data, (*out).len);
            drop(Box::from_raw(slice));
        }
    }
}

#[test]
fn platform_ops_install_available_and_run() {
    let engine = TestEngine::new();
    engine.create_project("Ops", 4, 4);
    assert!(!unsafe { calm_engine_op_available(engine.ptr, 0) });
    assert_eq!(
        unsafe { calm_engine_run_op(engine.ptr, 0, 0) },
        CalmStatus::Error
    );

    let ops = CalmPlatformOps {
        available: Some(fake_op_available),
        run: Some(fake_op_run),
        free_output: Some(fake_op_free),
    };
    assert_eq!(
        unsafe { calm_engine_install_platform_ops(engine.ptr, &ops) },
        CalmStatus::Ok
    );
    assert!(unsafe { calm_engine_op_available(engine.ptr, 0) });
    let active = engine.state().active_layer;
    assert_eq!(
        unsafe { calm_engine_run_op(engine.ptr, 0, active) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_install_platform_ops(engine.ptr, ptr::null()) },
        CalmStatus::Null
    );
}

#[test]
fn null_engine_pointer_is_handled_everywhere() {
    assert_eq!(
        unsafe { calm_engine_render(ptr::null_mut()) },
        CalmStatus::Null
    );
    assert_eq!(
        unsafe { calm_engine_pointer_down(ptr::null_mut(), 0.0, 0.0) },
        CalmStatus::Null
    );
    assert_eq!(
        unsafe { calm_engine_undo(ptr::null_mut()) },
        CalmStatus::Null
    );
    assert_eq!(unsafe { calm_engine_layer_visible(ptr::null_mut(), 0) }, -1);
    assert_eq!(unsafe { calm_engine_has_selection(ptr::null_mut()) }, 0);
    assert!(unsafe { calm_engine_layer_name(ptr::null_mut(), 0) }.is_null());
    let mut out = CalmState {
        width: 1,
        height: 1,
        zoom: 1.0,
        min_zoom: 1.0,
        max_zoom: 1.0,
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
        is_fit: 0,
        transform_active: 0,
    };
    // A null engine is rejected before the out-param is touched, same as everywhere else.
    assert_eq!(
        unsafe { calm_engine_state(ptr::null_mut(), &mut out) },
        CalmStatus::Null
    );
    assert_eq!(out.width, 1);
    assert_eq!(
        unsafe { calm_engine_state(ptr::null_mut(), ptr::null_mut()) },
        CalmStatus::Null
    );
    assert!(unsafe {
        calm_project_create(ptr::null_mut(), CString::new("x").unwrap().as_ptr(), 1, 1)
    }
    .is_null());
    assert!(!unsafe { calm_engine_op_available(ptr::null_mut(), 0) });
}

#[test]
fn scroll_pan_and_zoom_entry_points() {
    let engine = TestEngine::new();
    engine.create_project("Scroll", 128, 128);
    assert_eq!(unsafe { calm_engine_fit(engine.ptr) }, CalmStatus::Ok);
    assert_eq!(
        unsafe { calm_engine_pan_scroll(engine.ptr, 12.0, -8.0, 1) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_pan_scroll(engine.ptr, 12.0, -8.0, 0) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_zoom_scroll(engine.ptr, 40.0, 40.0, 1.0, 1) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_zoom_scroll(engine.ptr, 40.0, 40.0, -1.0, 0) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_pan_scroll(ptr::null_mut(), 1.0, 1.0, 1) },
        CalmStatus::Null
    );
    assert_eq!(
        unsafe { calm_engine_zoom_scroll(ptr::null_mut(), 0.0, 0.0, 1.0, 1) },
        CalmStatus::Null
    );
}

#[test]
fn attach_surface_rejects_null_layer_and_empty_size() {
    let engine = TestEngine::new();
    engine.create_project("Surface", 32, 32);
    assert_eq!(
        unsafe { calm_engine_attach_surface(engine.ptr, ptr::null_mut(), 64, 64, 1.0) },
        CalmStatus::Error
    );
    assert_eq!(
        unsafe { calm_engine_attach_surface(engine.ptr, 0x1 as *mut _, 0, 64, 1.0) },
        CalmStatus::Error
    );
    assert_eq!(
        unsafe { calm_engine_attach_surface(engine.ptr, 0x1 as *mut _, 64, 0, 1.0) },
        CalmStatus::Error
    );
    assert_eq!(
        unsafe { calm_engine_attach_surface(ptr::null_mut(), 0x1 as *mut _, 64, 64, 1.0) },
        CalmStatus::Null
    );
}

#[test]
fn opening_a_second_project_saves_and_replaces_the_first() {
    let engine = TestEngine::new();
    let first = engine.create_project("First", 16, 16);
    let second = {
        let name = CString::new("Second").unwrap();
        let id_ptr = unsafe { calm_project_create(engine.ptr, name.as_ptr(), 24, 24) };
        assert!(!id_ptr.is_null());
        let id = unsafe { CStr::from_ptr(id_ptr) }
            .to_str()
            .unwrap()
            .to_string();
        unsafe { calm_string_free(id_ptr) };
        id
    };
    assert_eq!(engine.state().width, 24);
    let first_c = CString::new(first.as_str()).unwrap();
    assert_eq!(
        unsafe { calm_project_open(engine.ptr, first_c.as_ptr()) },
        CalmStatus::Ok
    );
    assert_eq!(engine.state().width, 16);
    let second_c = CString::new(second.as_str()).unwrap();
    assert_eq!(
        unsafe { calm_project_open(engine.ptr, second_c.as_ptr()) },
        CalmStatus::Ok
    );
    assert_eq!(engine.state().width, 24);
}

extern "C" fn fake_op_unavailable(_kind: CalmOpKind) -> bool {
    false
}

extern "C" fn fake_op_run_fail(
    _kind: CalmOpKind,
    _input: *const CalmOpInput,
    _out: *mut CalmOpOutput,
) -> c_int {
    1
}

extern "C" fn fake_op_run_mask(
    _kind: CalmOpKind,
    input: *const CalmOpInput,
    out: *mut CalmOpOutput,
) -> c_int {
    unsafe {
        let inp = &*input;
        let len = (inp.w * inp.h) as usize;
        let buf = vec![255u8; len];
        let boxed = buf.into_boxed_slice();
        let data = Box::into_raw(boxed) as *mut u8;
        *out = CalmOpOutput {
            kind: CalmOpOutputKind::Mask,
            data,
            len,
            w: inp.w,
            h: inp.h,
        };
    }
    0
}

extern "C" fn fake_op_run_none(
    _kind: CalmOpKind,
    _input: *const CalmOpInput,
    out: *mut CalmOpOutput,
) -> c_int {
    unsafe {
        *out = CalmOpOutput {
            kind: CalmOpOutputKind::None,
            data: ptr::null_mut(),
            len: 0,
            w: 0,
            h: 0,
        };
    }
    0
}

#[test]
fn platform_ops_cover_unavailable_failed_and_mask_paths() {
    let engine = TestEngine::new();
    engine.create_project("Ops", 4, 4);
    let active = engine.state().active_layer;

    let unavailable = CalmPlatformOps {
        available: Some(fake_op_unavailable),
        run: Some(fake_op_run),
        free_output: Some(fake_op_free),
    };
    assert_eq!(
        unsafe { calm_engine_install_platform_ops(engine.ptr, &unavailable) },
        CalmStatus::Ok
    );
    assert!(!unsafe { calm_engine_op_available(engine.ptr, 0) });

    let missing_run = CalmPlatformOps {
        available: Some(fake_op_available),
        run: None,
        free_output: Some(fake_op_free),
    };
    assert_eq!(
        unsafe { calm_engine_install_platform_ops(engine.ptr, &missing_run) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_run_op(engine.ptr, 0, active) },
        CalmStatus::Error
    );

    let failing = CalmPlatformOps {
        available: Some(fake_op_available),
        run: Some(fake_op_run_fail),
        free_output: Some(fake_op_free),
    };
    assert_eq!(
        unsafe { calm_engine_install_platform_ops(engine.ptr, &failing) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_run_op(engine.ptr, 0, active) },
        CalmStatus::Error
    );

    let none_out = CalmPlatformOps {
        available: Some(fake_op_available),
        run: Some(fake_op_run_none),
        free_output: Some(fake_op_free),
    };
    assert_eq!(
        unsafe { calm_engine_install_platform_ops(engine.ptr, &none_out) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_run_op(engine.ptr, 0, active) },
        CalmStatus::Error
    );

    let mask = CalmPlatformOps {
        available: Some(fake_op_available),
        run: Some(fake_op_run_mask),
        free_output: Some(fake_op_free),
    };
    assert_eq!(
        unsafe { calm_engine_install_platform_ops(engine.ptr, &mask) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_run_op(engine.ptr, 0, active) },
        CalmStatus::Ok
    );
    assert!(!unsafe { calm_engine_op_available(engine.ptr, 999) });
}

#[test]
fn create_from_image_rejects_bad_lengths_and_null_pixels() {
    let engine = TestEngine::new();
    let name = CString::new("Bad").unwrap();
    let rgba = [0u8; 16];
    assert!(unsafe {
        calm_project_create_from_image(engine.ptr, name.as_ptr(), 2, 2, ptr::null(), 16)
    }
    .is_null());
    assert!(unsafe {
        calm_project_create_from_image(engine.ptr, name.as_ptr(), 2, 2, rgba.as_ptr(), 8)
    }
    .is_null());
    assert!(unsafe {
        calm_project_create_from_image(engine.ptr, name.as_ptr(), 0, 2, rgba.as_ptr(), 0)
    }
    .is_null());
}

#[test]
fn nudge_layer_adjustment_steps_through_the_ffi() {
    let engine = TestEngine::new();
    engine.create_project("Nudge", 32, 32);
    let active = engine.state().active_layer;
    let mut adj = CalmAdjustments {
        brightness: 0.0,
        contrast: 0.0,
        vibrance: 0.0,
        saturation: 0.0,
        levels_gamma: 0.0,
    };

    assert_eq!(
        unsafe { calm_engine_nudge_layer_adjustment(engine.ptr, active, 0, 3.0) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_layer_adjustments(engine.ptr, active, &mut adj) },
        CalmStatus::Ok
    );
    let expected = AdjustmentKind::Brightness.step() * 3.0;
    assert!(
        (adj.brightness - expected).abs() < 1e-4,
        "{} != {expected}",
        adj.brightness
    );

    assert_eq!(
        unsafe { calm_engine_nudge_layer_adjustment(engine.ptr, active, 4, -2.0) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_layer_adjustments(engine.ptr, active, &mut adj) },
        CalmStatus::Ok
    );
    let expected = 1.0 - AdjustmentKind::LevelsGamma.step() * 2.0;
    assert!((adj.levels_gamma - expected).abs() < 1e-4);
}

#[test]
fn nudge_layer_adjustment_rejects_an_unknown_kind_and_ignores_a_bad_index() {
    let engine = TestEngine::new();
    engine.create_project("NudgeGuards", 32, 32);
    unsafe {
        assert_ne!(
            calm_engine_nudge_layer_adjustment(engine.ptr, 0, 99, 1.0),
            CalmStatus::Ok
        );
        assert_eq!(
            calm_engine_nudge_layer_adjustment(engine.ptr, 9999, 0, 1.0),
            CalmStatus::Ok
        );
        assert_eq!(
            calm_engine_nudge_layer_adjustment(ptr::null_mut(), 0, 0, 1.0),
            CalmStatus::Null
        );
    }
}

#[test]
fn last_shape_tool_survives_switching_to_pen() {
    let engine = TestEngine::new();
    engine.create_project("Tools", 32, 32);
    unsafe {
        assert_eq!(
            calm_engine_set_tool(engine.ptr, Tool::Triangle as u32),
            CalmStatus::Ok
        );
        assert_eq!(engine.state().last_shape_tool, Tool::Triangle as u32);
        assert_eq!(
            calm_engine_set_tool(engine.ptr, Tool::Pen as u32),
            CalmStatus::Ok
        );
        assert_eq!(engine.state().last_shape_tool, Tool::Triangle as u32);
        assert_eq!(engine.state().last_select_tool, Tool::SelectRect as u32);
    }
}

#[test]
fn copy_returns_a_png_of_the_board() {
    let engine = TestEngine::new();
    engine.create_project("Clip", 16, 16);
    let mut bytes: *mut u8 = ptr::null_mut();
    let mut len = 0usize;
    let mut kind = 99u32;
    unsafe {
        assert_eq!(
            calm_engine_copy(engine.ptr, &mut bytes, &mut len, &mut kind),
            CalmStatus::Ok
        );
        assert_eq!(kind, 0);
        assert!(len > 8);
        assert_eq!(
            std::slice::from_raw_parts(bytes, 4),
            &[0x89, b'P', b'N', b'G']
        );
        calm_buffer_free(bytes, len);
    }
}

#[test]
fn hex_and_tool_queries_do_not_need_an_engine() {
    unsafe {
        assert_eq!(calm_tool_is_shape(Tool::Rect as u32), 1);
        assert_eq!(calm_tool_is_shape(Tool::Move as u32), 0);
        assert_eq!(calm_tool_is_selection(Tool::SelectLasso as u32), 1);
        assert_eq!(calm_tool_takes_brush_size(Tool::Pen as u32), 1);
        assert_eq!(calm_tool_takes_brush_size(Tool::Move as u32), 0);
        assert_eq!(calm_tool_takes_ink_opacity(Tool::Pen as u32), 1);
        assert_eq!(calm_tool_takes_ink_opacity(Tool::Fill as u32), 1);
        assert_eq!(calm_tool_takes_ink_opacity(Tool::Eraser as u32), 0);
        assert_eq!(calm_tool_shows_vector_mode(Tool::Pen as u32), 1);
        assert_eq!(calm_ink_opacity_min(), 0.0);
        assert_eq!(calm_ink_opacity_max(), 1.0);
        assert_eq!(calm_ink_opacity_default(), 1.0);
        let mut rgb = 0u32;
        let hex = CString::new("#1a2b3c").unwrap();
        assert_eq!(calm_parse_hex_rgb(hex.as_ptr(), &mut rgb), CalmStatus::Ok);
        assert_eq!(rgb, 0x1A2B3C);
        let formatted = calm_format_hex_rgb(rgb);
        assert!(!formatted.is_null());
        assert_eq!(CStr::from_ptr(formatted).to_str().unwrap(), "1A2B3C");
        calm_string_free(formatted);
        assert!((calm_lossy_export_quality() - 0.92).abs() < f32::EPSILON);
    }
}

/// Regression: hiding a text layer from the panel worked for the first one and was ignored for
/// every later one. This drives the same calls the shell does — create the layers by clicking
/// with the Text tool, type into each, then toggle visibility by index and read it back.
#[test]
fn every_text_layer_toggles_visibility_through_the_ffi() {
    let engine = TestEngine::new();
    engine.create_project("text-visibility", 512, 512);
    unsafe {
        assert_eq!(
            calm_engine_resize(engine.ptr, 512, 512, 1.0),
            CalmStatus::Ok
        );
        assert_eq!(calm_engine_fit(engine.ptr), CalmStatus::Ok);
        assert_eq!(
            calm_engine_set_tool(engine.ptr, Tool::Text as u32),
            CalmStatus::Ok
        );
    }

    for (n, y) in [(1u32, 80.0f32), (2, 240.0)] {
        let body = CString::new(format!("layer {n}")).unwrap();
        unsafe {
            assert_eq!(
                calm_engine_pointer_down(engine.ptr, 80.0, y),
                CalmStatus::Ok
            );
            assert_eq!(
                calm_engine_text_insert(engine.ptr, body.as_ptr()),
                CalmStatus::Ok
            );
        }
    }

    let count = engine.state().layer_count;
    let text_layers: Vec<u32> = (0..count)
        .filter(|&i| unsafe { calm_engine_layer_is_text(engine.ptr, i) } == 1)
        .collect();
    assert_eq!(text_layers.len(), 2, "two text layers reached the document");

    for &index in &text_layers {
        unsafe {
            assert_eq!(
                calm_engine_set_layer_visible(engine.ptr, index, 0),
                CalmStatus::Ok
            );
            assert_eq!(
                calm_engine_layer_visible(engine.ptr, index),
                0,
                "text layer {index} refused to hide"
            );
            assert_eq!(
                calm_engine_set_layer_visible(engine.ptr, index, 1),
                CalmStatus::Ok
            );
            assert_eq!(calm_engine_layer_visible(engine.ptr, index), 1);
        }
    }
}

/// The File → Export SVG path across the boundary. The markup itself is `engine/io`'s
/// (`svg_export.rs`); what this pins is that the string survives the trip and that the two
/// ways of having no document come back as a null pointer rather than a crash.
#[test]
fn export_svg_returns_markup_for_an_open_project_and_null_otherwise() {
    assert!(unsafe { calm_engine_export_svg(ptr::null_mut()) }.is_null());

    let e = TestEngine::new();
    assert!(
        unsafe { calm_engine_export_svg(e.ptr) }.is_null(),
        "nothing open, nothing to export"
    );

    e.create_project("Export", 64, 48);
    let raw = unsafe { calm_engine_export_svg(e.ptr) };
    assert!(!raw.is_null());
    let svg = unsafe { CStr::from_ptr(raw) }.to_str().unwrap().to_string();
    unsafe { calm_string_free(raw) };

    assert!(svg.starts_with("<svg"), "{svg:.80}");
    assert!(svg.contains("width=\"64\""), "{svg:.200}");
    assert!(svg.contains("height=\"48\""), "{svg:.200}");
    assert!(svg.trim_end().ends_with("</svg>"));
}

/// A vector layer has to reach the exported file as geometry rather than a bitmap — the whole
/// reason items are stored as parameters — and that has to still be true through the FFI.
#[test]
fn an_exported_vector_layer_is_still_geometry() {
    let e = TestEngine::new();
    e.create_project("Vectors", 64, 64);
    assert_eq!(calm_engine_set_vector_mode(e.ptr, 1), CalmStatus::Ok);
    assert_eq!(
        unsafe { calm_engine_set_tool(e.ptr, Tool::Rect as u32) },
        CalmStatus::Ok
    );
    assert_eq!(unsafe { calm_engine_set_fill(e.ptr, 1) }, CalmStatus::Ok);
    unsafe {
        calm_engine_pointer_down(e.ptr, 8.0, 8.0);
        calm_engine_pointer_move(e.ptr, 40.0, 32.0);
        calm_engine_pointer_up(e.ptr, 40.0, 32.0);
    }

    let raw = unsafe { calm_engine_export_svg(e.ptr) };
    assert!(!raw.is_null());
    let svg = unsafe { CStr::from_ptr(raw) }.to_str().unwrap().to_string();
    unsafe { calm_string_free(raw) };
    assert!(
        svg.contains("<rect"),
        "a drawn rect stays a <rect>: {svg:.400}"
    );
}

/// `end_camera_motion` tells the renderer a gesture finished so it can stop holding its
/// caches off. With no renderer attached there is nothing to tell, and that has to be a
/// success rather than an error — the shell calls it on every gesture end regardless.
#[test]
fn ending_camera_motion_is_safe_with_or_without_a_project() {
    assert_eq!(
        unsafe { calm_engine_end_camera_motion(ptr::null_mut()) },
        CalmStatus::Null
    );

    let e = TestEngine::new();
    assert_eq!(
        unsafe { calm_engine_end_camera_motion(e.ptr) },
        CalmStatus::Ok
    );

    e.create_project("Motion", 32, 32);
    assert_eq!(unsafe { calm_engine_pan(e.ptr, 5.0, 5.0) }, CalmStatus::Ok);
    assert_eq!(
        unsafe { calm_engine_end_camera_motion(e.ptr) },
        CalmStatus::Ok
    );
    assert_eq!(
        unsafe { calm_engine_end_camera_motion(e.ptr) },
        CalmStatus::Ok,
        "ending a motion that already ended is not an error"
    );
}

/// `enter_transform` is the idempotent half of the toggle: the shell calls it after a paste,
/// where the board may or may not already be transforming, and either way the layer that just
/// arrived is the one that has to end up with the handles.
#[test]
fn entering_transform_twice_stays_in_transform() {
    let engine = TestEngine::new();
    engine.create_project("Transform", 32, 32);
    let w = 4u32;
    let h = 4u32;
    let mut rgba = [9u8, 8, 7, 255].repeat((w * h) as usize);
    unpremultiply_rgba(&mut rgba);
    assert_eq!(
        unsafe {
            calm_engine_paste_image(engine.ptr, rgba.as_ptr(), rgba.len(), w, h, ptr::null_mut())
        },
        CalmStatus::Ok
    );
    assert_eq!(engine.state().transform_active, 0);

    assert_eq!(
        unsafe { calm_engine_enter_transform(engine.ptr) },
        CalmStatus::Ok
    );
    assert_eq!(engine.state().transform_active, 1);

    assert_eq!(
        unsafe { calm_engine_enter_transform(engine.ptr) },
        CalmStatus::Ok
    );
    assert_eq!(
        engine.state().transform_active,
        1,
        "a second enter is not a toggle"
    );

    assert_eq!(
        unsafe { calm_engine_toggle_transform(engine.ptr) },
        CalmStatus::Ok
    );
    assert_eq!(engine.state().transform_active, 0);
}

/// An empty layer has no bounds to draw handles around, so the engine refuses — and the state
/// flag has to say so rather than leaving the shell lighting a button for a mode nothing is
/// in.
#[test]
fn an_empty_layer_cannot_enter_transform() {
    let engine = TestEngine::new();
    engine.create_project("Transform", 32, 32);
    assert_eq!(unsafe { calm_engine_add_layer(engine.ptr) }, CalmStatus::Ok);

    assert_eq!(
        unsafe { calm_engine_enter_transform(engine.ptr) },
        CalmStatus::Ok,
        "refusing is not an error — there is simply nothing to transform"
    );
    assert_eq!(engine.state().transform_active, 0);
}

#[test]
fn fit_size_answers_without_an_engine_and_refuses_null_outputs() {
    let mut w = 0.0f32;
    let mut h = 0.0f32;
    let status = unsafe { calm_fit_size(800.0, 600.0, 1000.0, 1000.0, &mut w, &mut h) };
    assert_eq!(status, CalmStatus::Ok);
    assert!((w - h).abs() < 1e-4);
    assert!(w > 0.0 && w <= 600.0);

    let null = unsafe { calm_fit_size(800.0, 600.0, 1000.0, 1000.0, ptr::null_mut(), &mut h) };
    assert_eq!(null, CalmStatus::Null);
}

#[test]
fn fit_camera_answers_without_an_engine_and_refuses_null_outputs() {
    let mut zoom = 0.0f32;
    let mut pan_x = 0.0f32;
    let mut pan_y = 0.0f32;
    let status = unsafe {
        calm_fit_camera(
            800.0, 600.0, 1000.0, 500.0, &mut zoom, &mut pan_x, &mut pan_y,
        )
    };
    assert_eq!(status, CalmStatus::Ok);
    assert!(zoom > 0.0);
    assert!(pan_x > 0.0);
    assert!(pan_y > 0.0);

    let null = unsafe {
        calm_fit_camera(
            800.0,
            600.0,
            1000.0,
            500.0,
            ptr::null_mut(),
            &mut pan_x,
            &mut pan_y,
        )
    };
    assert_eq!(null, CalmStatus::Null);
}
