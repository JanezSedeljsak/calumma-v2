use calumma_core::{unpremultiply_rgba, BlendMode, Tool, IMPORT_MAX_SIDE};
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
        unsafe { calm_engine_merge_layer_down(engine.ptr, duplicated) },
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
        unsafe { calm_engine_paste_image(engine.ptr, rgba.as_ptr(), rgba.len(), w, h) },
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
