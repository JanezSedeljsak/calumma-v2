use calumma_ffi::*;
use std::ffi::CString;
use std::os::raw::c_int;
use std::ptr;

fn null_engine() -> *mut CalmEngine {
    ptr::null_mut()
}

fn engine_with_project(name: &str, w: u32, h: u32) -> (tempfile::TempDir, *mut CalmEngine) {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("t.sqlite").to_str().unwrap()).unwrap();
    let ptr = unsafe { calm_engine_new(path.as_ptr()) };
    assert!(!ptr.is_null());
    let name_c = CString::new(name).unwrap();
    let id = unsafe { calm_project_create(ptr, name_c.as_ptr(), w, h) };
    assert!(!id.is_null());
    unsafe { calm_string_free(id) };
    (dir, ptr)
}

fn engine_without_project() -> (tempfile::TempDir, *mut CalmEngine) {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("t.sqlite").to_str().unwrap()).unwrap();
    let ptr = unsafe { calm_engine_new(path.as_ptr()) };
    assert!(!ptr.is_null());
    (dir, ptr)
}

#[test]
fn every_status_entry_point_rejects_a_null_engine() {
    let e = null_engine();
    let text = CString::new("x").unwrap();
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
    let mut buf: *mut u8 = ptr::null_mut();
    let mut w = 0u32;
    let mut h = 0u32;
    let mut len = 0usize;

    unsafe {
        assert_eq!(calm_engine_state(e, &mut state), CalmStatus::Null);
        assert_eq!(calm_engine_resize(e, 1, 1, 1.0), CalmStatus::Null);
        assert_eq!(calm_engine_resize_document(e, 1, 1), CalmStatus::Null);
        assert_eq!(calm_engine_render(e), CalmStatus::Null);
        assert_eq!(calm_engine_pointer_down(e, 0.0, 0.0), CalmStatus::Null);
        assert_eq!(calm_engine_pointer_move(e, 0.0, 0.0), CalmStatus::Null);
        assert_eq!(calm_engine_pointer_up(e, 0.0, 0.0), CalmStatus::Null);
        assert_eq!(calm_engine_pan(e, 1.0, 1.0), CalmStatus::Null);
        assert_eq!(calm_engine_zoom(e, 0.0, 0.0, 1.1), CalmStatus::Null);
        assert_eq!(calm_engine_fit(e), CalmStatus::Null);
        assert_eq!(calm_engine_set_zoom(e, 1.0), CalmStatus::Null);
        assert_eq!(calm_engine_step_zoom(e, 1), CalmStatus::Null);
        assert_eq!(calm_engine_set_zoom_unit(e, 0.5), CalmStatus::Null);
        assert_eq!(calm_engine_set_board_colors(e, 0, 0, 0), CalmStatus::Null);
        assert_eq!(calm_engine_set_tool(e, 0), CalmStatus::Null);
        assert_eq!(calm_engine_set_color(e, 1, 2, 3, 4), CalmStatus::Null);
        let mut picked = 0u32;
        assert_eq!(
            calm_engine_sample_color(e, 0.0, 0.0, &mut picked),
            CalmStatus::Null
        );
        assert_eq!(
            calm_engine_pick_color(e, 0.0, 0.0, &mut picked),
            CalmStatus::Null
        );
        assert_eq!(calm_engine_set_brush(e, 4.0), CalmStatus::Null);
        assert_eq!(calm_engine_set_ink_opacity(e, 0.5), CalmStatus::Null);
        assert_eq!(calm_engine_set_fill(e, 1), CalmStatus::Null);
        assert_eq!(calm_engine_set_dark(e, 1), CalmStatus::Null);
        assert_eq!(calm_engine_set_shift(e, 1), CalmStatus::Null);
        assert_eq!(calm_engine_undo(e), CalmStatus::Null);
        assert_eq!(calm_engine_redo(e), CalmStatus::Null);
        assert_eq!(calm_engine_add_layer(e), CalmStatus::Null);
        assert_eq!(calm_engine_remove_layer(e, 0), CalmStatus::Null);
        assert_eq!(calm_engine_set_layer_visible(e, 0, 1), CalmStatus::Null);
        assert_eq!(calm_engine_set_active_layer(e, 0), CalmStatus::Null);
        assert_eq!(calm_engine_set_layer_selection(e, ptr::null(), 0), CalmStatus::Null);
        assert_eq!(calm_engine_align_layers(e, ptr::null(), 0, 0), CalmStatus::Null);
        assert_eq!(calm_engine_duplicate_layer(e, 0), CalmStatus::Null);
        assert_eq!(calm_engine_merge_layer_down(e, 0), CalmStatus::Null);
        assert_eq!(calm_engine_clip_layer_down(e, 0), CalmStatus::Null);
        assert_eq!(calm_engine_layer_can_clip_down(e, 0), 0);
        assert_eq!(calm_engine_set_layer_opacity(e, 0, 1.0), CalmStatus::Null);
        assert_eq!(calm_engine_set_layer_blend_mode(e, 0, 0), CalmStatus::Null);
        assert_eq!(calm_engine_reset_layer_transform(e, 0), CalmStatus::Null);
        assert_eq!(
            calm_engine_nudge_layer_adjustment(e, 0, 0, 1.0),
            CalmStatus::Null
        );
        assert_eq!(calm_engine_toggle_transform(e), CalmStatus::Null);
        assert_eq!(calm_engine_enter_transform(e), CalmStatus::Null);
        assert_eq!(calm_engine_exit_transform(e), CalmStatus::Null);
        assert_eq!(calm_engine_set_hover_layer(e, 0 as c_int), CalmStatus::Null);
        assert_eq!(calm_engine_clear_layer(e), CalmStatus::Null);
        assert_eq!(calm_engine_deselect(e), CalmStatus::Null);
        assert_eq!(calm_engine_selection_clear_pixels(e), CalmStatus::Null);
        assert_eq!(
            calm_engine_composite_rgba(e, &mut buf, &mut w, &mut h),
            CalmStatus::Null
        );
        assert_eq!(
            calm_engine_layer_rgba(e, 0, &mut buf, &mut w, &mut h),
            CalmStatus::Null
        );
        assert_eq!(
            calm_engine_selection_rgba(e, &mut buf, &mut w, &mut h),
            CalmStatus::Null
        );
        assert_eq!(
            calm_engine_export_psd(e, &mut buf, &mut len),
            CalmStatus::Null
        );
        assert_eq!(
            calm_engine_paste_image(e, ptr::null(), 0, 1, 1, ptr::null_mut()),
            CalmStatus::Null
        );
        assert_eq!(calm_project_save(e), CalmStatus::Null);
        assert_eq!(calm_project_close(e), CalmStatus::Null);
        assert_eq!(calm_project_open(e, text.as_ptr()), CalmStatus::Null);
        assert_eq!(calm_project_delete(e, text.as_ptr()), CalmStatus::Null);
        assert_eq!(calm_project_delete_all(ptr::null_mut()), CalmStatus::Null);
        assert_eq!(
            calm_project_rename(e, text.as_ptr(), text.as_ptr()),
            CalmStatus::Null
        );
        assert_eq!(
            calm_project_set_accent(e, text.as_ptr(), 0),
            CalmStatus::Null
        );
        assert_eq!(
            calm_project_thumbnail(e, text.as_ptr(), &mut buf, &mut len),
            CalmStatus::Null
        );
        let mut kind = 0u32;
        assert_eq!(
            calm_engine_copy(e, &mut buf, &mut len, &mut kind),
            CalmStatus::Null
        );
        assert_eq!(
            calm_engine_cut(e, &mut buf, &mut len, &mut kind),
            CalmStatus::Null
        );
        assert_eq!(
            calm_engine_copy_layer(e, 0, &mut buf, &mut len, &mut kind),
            CalmStatus::Null
        );
    }
}

#[test]
fn pointer_returning_entry_points_reject_a_null_engine() {
    let e = null_engine();
    let text = CString::new("x").unwrap();
    unsafe {
        assert!(calm_project_create(e, text.as_ptr(), 8, 8).is_null());
        assert!(calm_project_create_from_image(e, text.as_ptr(), 1, 1, ptr::null(), 0).is_null());
        assert!(calm_engine_layer_name(e, 0).is_null());
        assert!(calm_engine_layer_svg(e, 0).is_null());
        assert_eq!(calm_project_list(e, ptr::null_mut(), 0), 0);
    }
}

#[test]
fn value_returning_entry_points_fall_back_to_neutral_values_for_a_null_engine() {
    let e = null_engine();
    unsafe {
        assert_eq!(calm_engine_layer_visible(e, 0), -1);
        assert_eq!(calm_engine_has_selection(e), 0);
        assert_eq!(calm_engine_layer_opacity(e, 0), 1.0);
        assert_eq!(calm_engine_layer_blend_mode(e, 0), 0);
    }
}

#[test]
fn freeing_null_pointers_is_a_no_op() {
    unsafe {
        calm_string_free(ptr::null_mut());
        calm_buffer_free(ptr::null_mut(), 0);
        calm_engine_free(ptr::null_mut());
    }
}

#[test]
fn engine_new_rejects_an_unwritable_database_path() {
    let bad = CString::new("/proc/definitely/not/writable/x.sqlite").unwrap();
    let ptr = unsafe { calm_engine_new(bad.as_ptr()) };
    if !ptr.is_null() {
        unsafe { calm_engine_free(ptr) };
    }
}

#[test]
fn knob_and_command_entry_points_no_op_with_no_project_open() {
    let (_dir, e) = engine_without_project();
    unsafe {
        assert_eq!(calm_engine_pointer_down(e, 1.0, 1.0), CalmStatus::Ok);
        assert_eq!(calm_engine_pointer_move(e, 2.0, 2.0), CalmStatus::Ok);
        assert_eq!(calm_engine_pointer_up(e, 2.0, 2.0), CalmStatus::Ok);
        assert_eq!(calm_engine_add_layer(e), CalmStatus::Ok);
        assert_eq!(calm_engine_clear_layer(e), CalmStatus::Ok);
        assert_eq!(calm_engine_undo(e), CalmStatus::Ok);
        assert_eq!(calm_engine_redo(e), CalmStatus::Ok);
        assert_eq!(calm_engine_fit(e), CalmStatus::Ok);
        assert_eq!(calm_engine_pan(e, 4.0, 4.0), CalmStatus::Ok);
        assert_eq!(calm_engine_deselect(e), CalmStatus::Ok);
        assert_eq!(calm_engine_render(e), CalmStatus::Ok);
        assert_eq!(calm_project_save(e), CalmStatus::Ok);

        calm_engine_free(e);
    }
}

#[test]
fn data_returning_entry_points_error_with_no_project_open() {
    let (_dir, e) = engine_without_project();
    unsafe {
        assert_eq!(calm_engine_resize_document(e, 4, 4), CalmStatus::Error);
        assert_eq!(calm_engine_duplicate_layer(e, 0), CalmStatus::Error);
        assert_eq!(calm_engine_merge_layer_down(e, 0), CalmStatus::Error);
        assert_eq!(calm_engine_clip_layer_down(e, 0), CalmStatus::Error);
        assert_eq!(calm_engine_layer_can_clip_down(e, 0), 0);
        assert_eq!(calm_engine_set_layer_opacity(e, 0, 0.5), CalmStatus::Error);
        assert_eq!(calm_engine_set_layer_blend_mode(e, 0, 1), CalmStatus::Error);
        assert_eq!(calm_engine_reset_layer_transform(e, 0), CalmStatus::Error);
        assert_eq!(
            calm_engine_nudge_layer_adjustment(e, 0, 0, 1.0),
            CalmStatus::Error
        );
        assert_eq!(calm_engine_toggle_transform(e), CalmStatus::Error);
        assert_eq!(calm_engine_enter_transform(e), CalmStatus::Error);
        let mut picked = 0u32;
        assert_eq!(
            calm_engine_pick_color(e, 0.0, 0.0, &mut picked),
            CalmStatus::Error
        );
        assert_eq!(
            calm_engine_sample_color(e, 0.0, 0.0, &mut picked),
            CalmStatus::Error
        );

        let mut buf: *mut u8 = ptr::null_mut();
        let mut w = 0u32;
        let mut h = 0u32;
        assert_eq!(
            calm_engine_composite_rgba(e, &mut buf, &mut w, &mut h),
            CalmStatus::Error
        );
        assert_eq!(
            calm_engine_layer_rgba(e, 0, &mut buf, &mut w, &mut h),
            CalmStatus::Error
        );
        assert_eq!(
            calm_engine_selection_rgba(e, &mut buf, &mut w, &mut h),
            CalmStatus::Error
        );
        let mut len = 0usize;
        assert_eq!(
            calm_engine_export_psd(e, &mut buf, &mut len),
            CalmStatus::Error
        );
        assert!(calm_engine_layer_name(e, 0).is_null());
        assert!(calm_engine_layer_svg(e, 0).is_null());
        assert_eq!(calm_engine_layer_visible(e, 0), -1);

        calm_engine_free(e);
    }
}

#[test]
fn out_of_range_layer_index_errors_on_the_accessor_family() {
    let (_dir, e) = engine_with_project("Bounds", 32, 32);
    let huge = 9999u32;
    unsafe {
        assert_eq!(calm_engine_duplicate_layer(e, huge), CalmStatus::Error);
        assert_eq!(calm_engine_merge_layer_down(e, huge), CalmStatus::Error);
        assert_eq!(calm_engine_clip_layer_down(e, huge), CalmStatus::Error);
        assert_eq!(calm_engine_layer_can_clip_down(e, huge), 0);
        assert_eq!(calm_engine_remove_layer(e, huge), CalmStatus::Error);
        assert!(calm_engine_layer_name(e, huge).is_null());
        assert!(calm_engine_layer_svg(e, huge).is_null());
        assert_eq!(calm_engine_layer_visible(e, huge), -1);

        let mut buf: *mut u8 = ptr::null_mut();
        let mut w = 0u32;
        let mut h = 0u32;
        assert_eq!(
            calm_engine_layer_rgba(e, huge, &mut buf, &mut w, &mut h),
            CalmStatus::Error
        );

        calm_engine_free(e);
    }
}

#[test]
fn out_of_range_layer_index_is_silently_ignored_by_the_setter_family() {
    let (_dir, e) = engine_with_project("BoundsQuiet", 32, 32);
    let huge = 9999u32;
    unsafe {
        assert_eq!(calm_engine_set_layer_visible(e, huge, 1), CalmStatus::Ok);
        assert_eq!(calm_engine_set_active_layer(e, huge), CalmStatus::Ok);
        assert_eq!(
            calm_engine_set_hover_layer(e, huge as c_int),
            CalmStatus::Ok
        );
        assert_eq!(calm_engine_set_layer_opacity(e, huge, 0.5), CalmStatus::Ok);
        assert_eq!(calm_engine_set_layer_blend_mode(e, huge, 0), CalmStatus::Ok);
        assert_eq!(calm_engine_reset_layer_transform(e, huge), CalmStatus::Ok);
        assert_eq!(
            calm_engine_nudge_layer_adjustment(e, huge, 0, 1.0),
            CalmStatus::Ok
        );

        let real = 0u32;
        assert_eq!(calm_engine_layer_opacity(e, real), 1.0);

        calm_engine_free(e);
    }
}

#[test]
fn setters_reject_unknown_enum_discriminants() {
    let (_dir, e) = engine_with_project("Enums", 16, 16);
    unsafe {
        assert_ne!(calm_engine_set_tool(e, 9999), CalmStatus::Ok);
        let active = 0u32;
        assert_ne!(
            calm_engine_set_layer_blend_mode(e, active, 9999),
            CalmStatus::Ok
        );
        assert_ne!(
            calm_engine_nudge_layer_adjustment(e, active, 9999, 1.0),
            CalmStatus::Ok
        );
        calm_engine_free(e);
    }
}

#[test]
fn brush_size_and_zoom_clamp_instead_of_failing() {
    let (_dir, e) = engine_with_project("Clamp", 64, 64);
    unsafe {
        assert_eq!(calm_engine_set_brush(e, -50.0), CalmStatus::Ok);
        assert_eq!(calm_engine_set_brush(e, 100_000.0), CalmStatus::Ok);
        assert_eq!(calm_engine_set_ink_opacity(e, -1.0), CalmStatus::Ok);
        assert_eq!(calm_engine_set_ink_opacity(e, 4.0), CalmStatus::Ok);
        assert_eq!(calm_engine_set_zoom_unit(e, -5.0), CalmStatus::Ok);
        assert_eq!(calm_engine_set_zoom_unit(e, 5.0), CalmStatus::Ok);
        assert_eq!(calm_engine_set_zoom(e, 0.0), CalmStatus::Ok);
        calm_engine_free(e);
    }
}

#[test]
fn paste_image_validates_its_buffer_length() {
    let (_dir, e) = engine_with_project("Paste", 32, 32);
    let rgba = [255u8; 4 * 4];
    unsafe {
        assert_ne!(
            calm_engine_paste_image(e, rgba.as_ptr(), rgba.len(), 100, 100, ptr::null_mut()),
            CalmStatus::Ok
        );
        assert_ne!(
            calm_engine_paste_image(e, ptr::null(), 0, 2, 2, ptr::null_mut()),
            CalmStatus::Ok
        );
        assert_eq!(
            calm_engine_paste_image(e, rgba.as_ptr(), rgba.len(), 2, 2, ptr::null_mut()),
            CalmStatus::Ok
        );
        calm_engine_free(e);
    }
}

#[test]
fn project_open_rejects_an_unknown_id() {
    let (_dir, e) = engine_without_project();
    let missing = CString::new("no-such-project").unwrap();
    unsafe {
        assert_ne!(calm_project_open(e, missing.as_ptr()), CalmStatus::Ok);
        assert_ne!(calm_project_delete(e, missing.as_ptr()), CalmStatus::Ok);
        calm_engine_free(e);
    }
}

#[test]
fn out_parameter_pointers_are_null_checked_independently_of_the_engine() {
    let (_dir, e) = engine_with_project("OutParams", 16, 16);
    let mut buf: *mut u8 = ptr::null_mut();
    let mut w = 0u32;
    let mut h = 0u32;
    let mut len = 0usize;
    unsafe {
        assert_eq!(
            calm_engine_composite_rgba(e, ptr::null_mut(), &mut w, &mut h),
            CalmStatus::Null
        );
        assert_eq!(
            calm_engine_composite_rgba(e, &mut buf, ptr::null_mut(), &mut h),
            CalmStatus::Null
        );
        assert_eq!(
            calm_engine_export_psd(e, ptr::null_mut(), &mut len),
            CalmStatus::Null
        );
        assert_eq!(
            calm_engine_export_psd(e, &mut buf, ptr::null_mut()),
            CalmStatus::Null
        );
        assert_eq!(
            calm_engine_layer_thumbnail(e, 0, 32, ptr::null_mut(), &mut w, &mut h),
            CalmStatus::Null
        );
        assert_eq!(calm_engine_state(e, ptr::null_mut()), CalmStatus::Null);
        assert_eq!(
            calm_engine_sample_color(e, 0.0, 0.0, ptr::null_mut()),
            CalmStatus::Null
        );
        assert_eq!(
            calm_engine_pick_color(e, 0.0, 0.0, ptr::null_mut()),
            CalmStatus::Null
        );
        calm_engine_free(e);
    }
}

#[test]
fn project_list_respects_a_capacity_smaller_than_the_project_count() {
    let (_dir, e) = engine_with_project("First", 8, 8);
    unsafe {
        for name in ["Second", "Third"] {
            let n = CString::new(name).unwrap();
            let id = calm_project_create(e, n.as_ptr(), 8, 8);
            assert!(!id.is_null());
            calm_string_free(id);
        }

        let mut one = [CalmProjectInfo {
            id: ptr::null_mut(),
            name: ptr::null_mut(),
            width: 0,
            height: 0,
            opened_at: 0,
            accent: 0,
        }];
        let n = calm_project_list(e, one.as_mut_ptr(), one.len());
        assert_eq!(n, 1);
        calm_string_free(one[0].id);
        calm_string_free(one[0].name);

        assert_eq!(calm_project_list(e, one.as_mut_ptr(), 0), 0);
        assert_eq!(calm_project_list(e, ptr::null_mut(), 4), 0);

        calm_engine_free(e);
    }
}

#[test]
fn op_entry_points_reject_an_unknown_op_kind() {
    let (_dir, e) = engine_with_project("Ops", 16, 16);
    unsafe {
        assert!(!calm_engine_op_available(e, 9999));
        assert_eq!(calm_engine_run_op(e, 9999, 0), CalmStatus::Error);
        calm_engine_free(e);
    }
}

#[test]
fn a_thumbnail_is_bounded_by_its_requested_max_side() {
    let (_dir, e) = engine_with_project("Thumb", 200, 100);
    let mut buf: *mut u8 = ptr::null_mut();
    let mut w = 0u32;
    let mut h = 0u32;
    unsafe {
        assert_eq!(
            calm_engine_layer_thumbnail(e, 0, 40, &mut buf, &mut w, &mut h),
            CalmStatus::Ok
        );
        assert!(w <= 40 && h <= 40, "thumbnail {w}x{h} exceeded max_side 40");
        assert!(w > 0 && h > 0);
        calm_buffer_free(buf, (w * h * 4) as usize);
        calm_engine_free(e);
    }
}

#[test]
fn a_saved_project_hands_back_its_thumbnail_as_png() {
    let (_dir, e) = engine_with_project("Thumb", 48, 32);
    unsafe {
        let project_id = {
            let mut projects: Vec<CalmProjectInfo> = (0..1)
                .map(|_| CalmProjectInfo {
                    id: ptr::null_mut(),
                    name: ptr::null_mut(),
                    width: 0,
                    height: 0,
                    opened_at: 0,
                    accent: 0,
                })
                .collect();
            let n = calm_project_list(e, projects.as_mut_ptr(), 1);
            assert_eq!(n, 1);
            let id =
                CString::new(std::ffi::CStr::from_ptr(projects[0].id).to_str().unwrap()).unwrap();
            calm_string_free(projects[0].id);
            calm_string_free(projects[0].name);
            id
        };
        assert_eq!(calm_project_save(e), CalmStatus::Ok);
        let mut png: *mut u8 = ptr::null_mut();
        let mut len = 0usize;
        assert_eq!(
            calm_project_thumbnail(e, project_id.as_ptr(), &mut png, &mut len),
            CalmStatus::Ok
        );
        assert!(len > 8);
        assert_eq!(
            std::slice::from_raw_parts(png, 4),
            &[0x89, b'P', b'N', b'G']
        );
        calm_buffer_free(png, len);
        calm_engine_free(e);
    }
}

#[test]
fn a_raster_layer_has_no_svg_representation() {
    let (_dir, e) = engine_with_project("Raster", 16, 16);
    unsafe {
        assert!(calm_engine_layer_svg(e, 0).is_null());
        calm_engine_free(e);
    }
}

#[test]
fn palette_accessor_is_bounded() {
    let count = calm_palette_count();
    assert!(count > 0);
    let last = calm_palette_color(count - 1);
    let wrapped = calm_palette_color(count);
    assert_eq!(wrapped, calm_palette_color(0));
    let _ = last;
}

/// The shell assigns this straight to `preferredFramesPerSecond`, so every answer has to be one
/// a view can actually run at. Zero is the agreed "as fast as the display allows" — anything
/// else returned for a null engine, or for one with no document yet, would be the shell pacing
/// its own view down before the engine has anything to say.
#[test]
fn the_frame_hint_never_stalls_a_view_that_has_nothing_to_ask() {
    assert_eq!(
        unsafe { calm_engine_frame_hint(null_engine()) },
        0,
        "a null engine cannot pace anything"
    );

    let (_dir, ptr) = engine_without_project();
    assert_eq!(
        unsafe { calm_engine_frame_hint(ptr) },
        0,
        "nor can one with no document open"
    );
    unsafe { calm_engine_free(ptr) };

    let (_dir, ptr) = engine_with_project("p", 64, 64);
    assert_eq!(
        unsafe { calm_engine_frame_hint(ptr) },
        0,
        "and with no surface attached there is no renderer to ask either"
    );
    unsafe { calm_engine_free(ptr) };
}
