//! Drag-reorder, rename and lock across the boundary.
//!
//! Two things can only go wrong here rather than in core. Rows arrive in the panel's top-first
//! order and have to come out as the stack's bottom-first indices, so a reorder that reads
//! correct in a core test can still land upside down through the FFI. And a rename arrives as
//! a borrowed C string, which is the one place a missing null check or a non-UTF-8 byte turns
//! into a crash instead of an error.

use calumma_ffi::*;
use std::ffi::{CStr, CString};
use std::ptr;

fn engine_with_layers(extra: usize) -> (tempfile::TempDir, *mut CalmEngine) {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("t.sqlite").to_str().unwrap()).unwrap();
    let e = unsafe { calm_engine_new(path.as_ptr()) };
    assert!(!e.is_null());
    let name = CString::new("Stack").unwrap();
    let id = unsafe { calm_project_create(e, name.as_ptr(), 32, 32) };
    assert!(!id.is_null());
    unsafe { calm_string_free(id) };
    for _ in 0..extra {
        assert_eq!(unsafe { calm_engine_add_layer(e) }, CalmStatus::Ok);
    }
    (dir, e)
}

fn layer_names(e: *mut CalmEngine) -> Vec<String> {
    let mut state = unsafe { std::mem::zeroed::<CalmState>() };
    assert_eq!(unsafe { calm_engine_state(e, &mut state) }, CalmStatus::Ok);
    (0..state.layer_count)
        .map(|i| {
            let p = unsafe { calm_engine_layer_name(e, i) };
            assert!(!p.is_null());
            let s = unsafe { CStr::from_ptr(p) }.to_str().unwrap().to_string();
            unsafe { calm_string_free(p) };
            s
        })
        .collect()
}

fn rename(e: *mut CalmEngine, index: u32, name: &str) -> CalmStatus {
    let c = CString::new(name).unwrap();
    unsafe { calm_engine_set_layer_name(e, index, c.as_ptr()) }
}

/// The whole reason the FFI takes rows rather than indices. Row 0 is the *top* of the panel
/// and the *last* layer in the stack; if the flip were missing or inverted this test would
/// move the wrong layer while every core test still passed.
#[test]
fn a_row_move_lands_the_right_way_up() {
    let (_dir, e) = engine_with_layers(2);
    assert_eq!(rename(e, 1, "Bottom"), CalmStatus::Ok);
    assert_eq!(rename(e, 2, "Middle"), CalmStatus::Ok);
    assert_eq!(rename(e, 3, "Top"), CalmStatus::Ok);
    assert_eq!(layer_names(e), ["Paper", "Bottom", "Middle", "Top"]);

    assert_eq!(
        unsafe { calm_engine_move_layer_row(e, 0, 2) },
        CalmStatus::Ok,
        "drag the top row down two rows"
    );
    assert_eq!(
        layer_names(e),
        ["Paper", "Top", "Bottom", "Middle"],
        "which is the bottom of the stack, just above Paper"
    );
    unsafe { calm_engine_free(e) };
}

#[test]
fn a_row_move_that_cannot_happen_is_an_error_not_a_panic() {
    let (_dir, e) = engine_with_layers(1);
    unsafe {
        assert_ne!(calm_engine_move_layer_row(e, 0, 0), CalmStatus::Ok, "no-op");
        assert_ne!(calm_engine_move_layer_row(e, 99, 0), CalmStatus::Ok);
        assert_ne!(calm_engine_move_layer_row(e, 0, 99), CalmStatus::Ok);
        assert_ne!(
            calm_engine_move_layer_row(e, 0, 2),
            CalmStatus::Ok,
            "the last row is Paper and nothing drops onto it"
        );
        calm_engine_free(e);
    }
}

#[test]
fn renaming_crosses_the_boundary_and_refuses_what_core_refuses() {
    let (_dir, e) = engine_with_layers(0);
    assert_eq!(rename(e, 1, "  Line art  "), CalmStatus::Ok);
    assert_eq!(layer_names(e)[1], "Line art", "trimmed on the way in");

    assert_ne!(rename(e, 1, "   "), CalmStatus::Ok, "empty after trimming");
    assert_ne!(rename(e, 0, "Background"), CalmStatus::Ok, "Paper is fixed");
    assert_ne!(rename(e, 1, "Paper"), CalmStatus::Ok, "and unique");
    assert_ne!(rename(e, 99, "Nope"), CalmStatus::Ok, "out of range");
    assert_eq!(layer_names(e), ["Paper", "Line art"]);
    unsafe { calm_engine_free(e) };
}

/// A name arrives as a borrowed pointer the shell owns. Null and invalid UTF-8 are the two
/// ways that goes wrong, and both have to be errors rather than a read through a bad pointer.
#[test]
fn a_name_pointer_is_validated_before_it_is_read() {
    let (_dir, e) = engine_with_layers(0);
    let before = layer_names(e);
    unsafe {
        assert_eq!(
            calm_engine_set_layer_name(e, 1, ptr::null()),
            CalmStatus::Null
        );
        let invalid = [0xffu8, 0xfe, 0x00];
        assert_ne!(
            calm_engine_set_layer_name(e, 1, invalid.as_ptr() as *const i8),
            CalmStatus::Ok,
            "not UTF-8, so not a name"
        );
    }
    assert_eq!(layer_names(e), before, "and nothing changed");
    unsafe { calm_engine_free(e) };
}

#[test]
fn a_lock_round_trips_through_its_own_accessor() {
    let (_dir, e) = engine_with_layers(0);
    unsafe {
        assert_eq!(calm_engine_layer_locked(e, 1), 0, "unlocked to start");
        assert_eq!(calm_engine_set_layer_locked(e, 1, 1), CalmStatus::Ok);
        assert_eq!(calm_engine_layer_locked(e, 1), 1);
        assert_ne!(
            calm_engine_set_layer_locked(e, 1, 1),
            CalmStatus::Ok,
            "setting it to what it already is reports no change"
        );
        assert_eq!(calm_engine_set_layer_locked(e, 1, 0), CalmStatus::Ok);
        assert_eq!(calm_engine_layer_locked(e, 1), 0);
        calm_engine_free(e);
    }
}

/// The accessors return a negative rather than a bool so the shell can tell "no" from "could
/// not ask" — an out-of-range index during a refresh must not read as unlocked.
#[test]
fn layer_predicates_report_failure_distinctly_from_false() {
    let (_dir, e) = engine_with_layers(0);
    unsafe {
        assert_eq!(calm_engine_layer_is_paper(e, 0), 1);
        assert_eq!(calm_engine_layer_is_paper(e, 1), 0);
        assert_eq!(calm_engine_layer_is_paper(e, 99), -1, "out of range");
        assert_eq!(calm_engine_layer_locked(e, 99), -1);

        let null: *mut CalmEngine = ptr::null_mut();
        assert_eq!(calm_engine_layer_is_paper(null, 0), -1);
        assert_eq!(calm_engine_layer_locked(null, 0), -1);
        calm_engine_free(e);
    }
}

#[test]
fn layer_stack_entry_points_reject_a_null_engine() {
    let e: *mut CalmEngine = ptr::null_mut();
    let name = CString::new("x").unwrap();
    unsafe {
        assert_eq!(calm_engine_move_layer_row(e, 0, 1), CalmStatus::Null);
        assert_eq!(
            calm_engine_set_layer_name(e, 0, name.as_ptr()),
            CalmStatus::Null
        );
        assert_eq!(calm_engine_set_layer_locked(e, 0, 1), CalmStatus::Null);
    }
}

#[test]
fn layer_stack_entry_points_error_with_no_project_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("t.sqlite").to_str().unwrap()).unwrap();
    let e = unsafe { calm_engine_new(path.as_ptr()) };
    let name = CString::new("x").unwrap();
    unsafe {
        assert_ne!(calm_engine_move_layer_row(e, 0, 1), CalmStatus::Ok);
        assert_ne!(
            calm_engine_set_layer_name(e, 0, name.as_ptr()),
            CalmStatus::Ok
        );
        assert_ne!(calm_engine_set_layer_locked(e, 0, 1), CalmStatus::Ok);
        assert_eq!(calm_engine_layer_locked(e, 0), -1);
        assert_eq!(calm_engine_layer_is_paper(e, 0), -1);
        calm_engine_free(e);
    }
}

/// A lock is only worth having if it survives the round trip the shell actually performs:
/// edit, autosave, reopen. Core covers the column; this covers the whole path through the FFI.
#[test]
fn a_lock_and_a_rename_survive_save_and_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("t.sqlite").to_str().unwrap()).unwrap();
    let e = unsafe { calm_engine_new(path.as_ptr()) };
    let name = CString::new("Persisted").unwrap();
    let id_ptr = unsafe { calm_project_create(e, name.as_ptr(), 32, 32) };
    let id = unsafe { CStr::from_ptr(id_ptr) }
        .to_str()
        .unwrap()
        .to_string();
    unsafe { calm_string_free(id_ptr) };

    assert_eq!(rename(e, 1, "Ink"), CalmStatus::Ok);
    assert_eq!(
        unsafe { calm_engine_set_layer_locked(e, 1, 1) },
        CalmStatus::Ok
    );
    assert_eq!(unsafe { calm_project_save(e) }, CalmStatus::Ok);

    let id_c = CString::new(id).unwrap();
    assert_eq!(
        unsafe { calm_project_open(e, id_c.as_ptr()) },
        CalmStatus::Ok
    );
    assert_eq!(layer_names(e)[1], "Ink");
    assert_eq!(unsafe { calm_engine_layer_locked(e, 1) }, 1);
    unsafe { calm_engine_free(e) };
}

fn layer_id(e: *mut CalmEngine, index: u32) -> Option<String> {
    let p = unsafe { calm_engine_layer_id(e, index) };
    if p.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(p) }.to_str().unwrap().to_string();
    unsafe { calm_string_free(p) };
    Some(s)
}

fn bounds(e: *mut CalmEngine, index: u32) -> (CalmStatus, CalmLayerBounds) {
    let mut out = CalmLayerBounds::default();
    let status = unsafe { calm_engine_layer_bounds(e, index, &mut out) };
    (status, out)
}

/// Move Up / Move Down are the keyboard-reachable half of drag-reorder, and they take a stack
/// *index* where the drag takes panel rows — a distinction worth pinning, since getting it
/// wrong moves a different layer than the menu item names.
#[test]
fn move_up_and_move_down_are_inverses() {
    let (_dir, e) = engine_with_layers(2);
    rename(e, 1, "Middle");
    rename(e, 2, "Top");
    let before = layer_names(e);

    assert_eq!(unsafe { calm_engine_move_layer_up(e, 1) }, CalmStatus::Ok);
    assert_ne!(layer_names(e), before);
    assert_eq!(unsafe { calm_engine_move_layer_down(e, 2) }, CalmStatus::Ok);
    assert_eq!(layer_names(e), before, "up then down is where it started");
    unsafe { calm_engine_free(e) };
}

/// Paper is the board's backing sheet, so nothing may be pushed beneath it and it may not
/// climb — and the top layer has nowhere further up to go. All three come back as errors
/// rather than silently doing nothing, so the shell can grey the menu item.
#[test]
fn a_move_that_cannot_happen_is_an_error() {
    let (_dir, e) = engine_with_layers(1);
    let before = layer_names(e);
    assert_ne!(
        unsafe { calm_engine_move_layer_down(e, 1) },
        CalmStatus::Ok,
        "layer 1 cannot sink below Paper"
    );
    assert_ne!(
        unsafe { calm_engine_move_layer_up(e, 0) },
        CalmStatus::Ok,
        "Paper cannot climb"
    );
    let top = layer_names(e).len() as u32 - 1;
    assert_ne!(
        unsafe { calm_engine_move_layer_up(e, top) },
        CalmStatus::Ok,
        "the top layer has nowhere to go"
    );
    assert_ne!(unsafe { calm_engine_move_layer_up(e, 99) }, CalmStatus::Ok);
    assert_eq!(layer_names(e), before, "no failed move moved anything");
    unsafe { calm_engine_free(e) };
}

/// The shell keys thumbnail caches off the layer id, so two layers must never share one and a
/// duplicate has to get its own — otherwise one layer's preview would show for both.
#[test]
fn every_layer_carries_its_own_id() {
    let (_dir, e) = engine_with_layers(2);
    let ids: Vec<_> = (0..3).map(|i| layer_id(e, i).unwrap()).collect();
    assert!(ids.iter().all(|id| !id.is_empty()));
    assert_eq!(
        ids.iter().collect::<std::collections::HashSet<_>>().len(),
        3,
        "{ids:?}"
    );

    assert_eq!(unsafe { calm_engine_duplicate_layer(e, 1) }, CalmStatus::Ok);
    let copy = layer_id(e, 2).unwrap();
    assert!(!ids.contains(&copy), "a duplicate gets a fresh id");

    assert_eq!(layer_id(e, 99), None, "out of range is a null pointer");
    assert_eq!(layer_id(ptr::null_mut(), 0), None);
    unsafe { calm_engine_free(e) };
}

/// The bounds strip reads painted pixels, not the tile grid, so an untouched layer has no
/// bounds at all rather than a zero-sized box at the origin.
#[test]
fn layer_bounds_are_empty_until_something_is_painted() {
    let (_dir, e) = engine_with_layers(1);
    let (status, out) = bounds(e, 1);
    assert_ne!(status, CalmStatus::Ok, "nothing painted, so no bounds");
    assert_eq!(
        out,
        CalmLayerBounds::default(),
        "and the out param is zeroed"
    );

    assert_eq!(
        unsafe { calm_engine_set_active_layer(e, 1) },
        CalmStatus::Ok
    );
    assert_eq!(unsafe { calm_engine_set_tool(e, 2) }, CalmStatus::Ok);
    assert_eq!(unsafe { calm_engine_set_fill(e, 1) }, CalmStatus::Ok);
    unsafe {
        calm_engine_pointer_down(e, 4.0, 4.0);
        calm_engine_pointer_move(e, 20.0, 20.0);
        calm_engine_pointer_up(e, 20.0, 20.0);
    }
    let (status, painted) = bounds(e, 1);
    assert_eq!(status, CalmStatus::Ok);
    assert!(painted.width > 0.0 && painted.height > 0.0, "{painted:?}");
    unsafe { calm_engine_free(e) };
}

#[test]
fn layer_bounds_guards_a_null_engine_and_a_null_out() {
    let (_dir, e) = engine_with_layers(0);
    let mut out = CalmLayerBounds::default();
    assert_eq!(
        unsafe { calm_engine_layer_bounds(ptr::null_mut(), 0, &mut out) },
        CalmStatus::Null
    );
    assert_eq!(
        unsafe { calm_engine_layer_bounds(e, 0, ptr::null_mut()) },
        CalmStatus::Null
    );
    assert_ne!(
        unsafe { calm_engine_set_layer_bounds(e, 99, 0.0, 0.0, 4.0, 4.0) },
        CalmStatus::Ok,
        "an index with no bounds to set is an error, not a panic"
    );
    unsafe { calm_engine_free(e) };
}

/// Position moves the layer; size only ever crops, so asking for a box bigger than the
/// content clamps rather than scaling pixels that do not exist.
#[test]
fn setting_bounds_moves_by_position_and_only_crops_by_size() {
    let (_dir, e) = engine_with_layers(1);
    assert_eq!(
        unsafe { calm_engine_set_active_layer(e, 1) },
        CalmStatus::Ok
    );
    assert_eq!(unsafe { calm_engine_set_tool(e, 2) }, CalmStatus::Ok);
    assert_eq!(unsafe { calm_engine_set_fill(e, 1) }, CalmStatus::Ok);
    unsafe {
        calm_engine_pointer_down(e, 4.0, 4.0);
        calm_engine_pointer_move(e, 20.0, 20.0);
        calm_engine_pointer_up(e, 20.0, 20.0);
    }
    let (_, before) = bounds(e, 1);

    assert_eq!(
        unsafe {
            calm_engine_set_layer_bounds(
                e,
                1,
                before.x + 5.0,
                before.y,
                before.width,
                before.height,
            )
        },
        CalmStatus::Ok
    );
    let (_, moved) = bounds(e, 1);
    assert!((moved.x - (before.x + 5.0)).abs() < 0.01, "{moved:?}");
    assert!((moved.width - before.width).abs() < 0.01);

    assert_eq!(
        unsafe {
            calm_engine_set_layer_bounds(
                e,
                1,
                moved.x,
                moved.y,
                moved.width * 4.0,
                moved.height * 4.0,
            )
        },
        CalmStatus::Ok
    );
    let (_, grown) = bounds(e, 1);
    assert!(
        grown.width <= moved.width + 0.01,
        "a size larger than the content clamps instead of scaling it up: {grown:?}"
    );
    unsafe { calm_engine_free(e) };
}

/// The shell redraws a layer's thumbnail when this number changes, so it has to move when the
/// pixels do and stay put when they do not — a revision that never changed would freeze every
/// preview, and one that changed every frame would rebuild them all continuously.
#[test]
fn the_preview_revision_tracks_content_not_frames() {
    let (_dir, e) = engine_with_layers(1);
    let quiet = unsafe { calm_engine_layer_preview_revision(e, 1) };
    assert_eq!(
        unsafe { calm_engine_layer_preview_revision(e, 1) },
        quiet,
        "reading it twice does not move it"
    );

    assert_eq!(
        unsafe { calm_engine_set_active_layer(e, 1) },
        CalmStatus::Ok
    );
    assert_eq!(unsafe { calm_engine_set_tool(e, 2) }, CalmStatus::Ok);
    assert_eq!(unsafe { calm_engine_set_fill(e, 1) }, CalmStatus::Ok);
    unsafe {
        calm_engine_pointer_down(e, 2.0, 2.0);
        calm_engine_pointer_move(e, 18.0, 18.0);
        calm_engine_pointer_up(e, 18.0, 18.0);
    }
    assert_ne!(unsafe { calm_engine_layer_preview_revision(e, 1) }, quiet);

    assert_eq!(unsafe { calm_engine_layer_preview_revision(e, 99) }, 0);
    assert_eq!(
        unsafe { calm_engine_layer_preview_revision(ptr::null_mut(), 0) },
        0
    );
    unsafe { calm_engine_free(e) };
}

/// A vector layer has no tile grid to take a revision from, so it reports its item count —
/// which is what has to move when a shape is added.
#[test]
fn a_vector_layer_reports_its_item_count_as_its_revision() {
    let (_dir, e) = engine_with_layers(0);
    assert_eq!(calm_engine_set_vector_mode(e, 1), CalmStatus::Ok);
    assert_eq!(unsafe { calm_engine_set_tool(e, 2) }, CalmStatus::Ok);
    unsafe {
        calm_engine_pointer_down(e, 2.0, 2.0);
        calm_engine_pointer_move(e, 18.0, 18.0);
        calm_engine_pointer_up(e, 18.0, 18.0);
    }
    let mut state = unsafe { std::mem::zeroed::<CalmState>() };
    assert_eq!(unsafe { calm_engine_state(e, &mut state) }, CalmStatus::Ok);
    let vector = state.active_layer;
    assert_eq!(calm_engine_layer_is_vector(e, vector), 1);
    assert_eq!(unsafe { calm_engine_layer_preview_revision(e, vector) }, 1);

    unsafe {
        calm_engine_pointer_down(e, 20.0, 20.0);
        calm_engine_pointer_move(e, 28.0, 28.0);
        calm_engine_pointer_up(e, 28.0, 28.0);
    }
    assert_eq!(
        unsafe { calm_engine_layer_preview_revision(e, vector) },
        2,
        "a second item is a second revision"
    );
    unsafe { calm_engine_free(e) };
}
