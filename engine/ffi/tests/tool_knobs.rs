//! The tool knobs added with the brush, blur and magic-wand work: their setters, the limits
//! the shell reads its slider ranges from, and the predicates that decide which controls a
//! tool shows at all.
//!
//! These are pure boundary tests. The behaviour behind them is covered in core; what can only
//! break here is the crossing — a discriminant the engine does not know, a value outside the
//! range, or a call arriving with no project open.

use calumma_core::{Brush, Tool};
use calumma_ffi::*;
use std::ffi::CString;
use std::ptr;

fn engine_with_project(w: u32, h: u32) -> (tempfile::TempDir, *mut CalmEngine) {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("t.sqlite").to_str().unwrap()).unwrap();
    let ptr = unsafe { calm_engine_new(path.as_ptr()) };
    assert!(!ptr.is_null());
    let name = CString::new("Knobs").unwrap();
    let id = unsafe { calm_project_create(ptr, name.as_ptr(), w, h) };
    assert!(!id.is_null());
    unsafe { calm_string_free(id) };
    (dir, ptr)
}

#[test]
fn every_brush_id_is_accepted_and_nothing_else_is() {
    let (_dir, e) = engine_with_project(32, 32);
    unsafe {
        for brush in [Brush::Pen, Brush::Marker, Brush::Crayon, Brush::Airbrush] {
            assert_eq!(
                calm_engine_set_brush_kind(e, brush as u32),
                CalmStatus::Ok,
                "{brush:?} is a brush the engine knows"
            );
        }
        assert_ne!(
            calm_engine_set_brush_kind(e, 9999),
            CalmStatus::Ok,
            "an id from a newer shell is refused, not silently taken as Pen"
        );
        calm_engine_free(e);
    }
}

/// Sliders are bounded by what the engine reports, so a value outside the range means the
/// shell and the engine disagree — clamping is the only behaviour that cannot lose an edit.
#[test]
fn continuous_knobs_clamp_rather_than_failing() {
    let (_dir, e) = engine_with_project(32, 32);
    unsafe {
        for value in [-5.0f32, 0.0, 0.5, 1.0, 9.0] {
            assert_eq!(calm_engine_set_blur_strength(e, value), CalmStatus::Ok);
            assert_eq!(calm_engine_set_eraser_hardness(e, value), CalmStatus::Ok);
        }
        for value in [0u8, 24, 128, 255] {
            assert_eq!(calm_engine_set_tolerance(e, value), CalmStatus::Ok);
        }
        calm_engine_free(e);
    }
}

#[test]
fn knob_setters_no_op_with_no_project_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("t.sqlite").to_str().unwrap()).unwrap();
    let e = unsafe { calm_engine_new(path.as_ptr()) };
    unsafe {
        assert_eq!(calm_engine_set_blur_strength(e, 0.5), CalmStatus::Ok);
        assert_eq!(calm_engine_set_tolerance(e, 40), CalmStatus::Ok);
        assert_eq!(calm_engine_set_eraser_hardness(e, 0.5), CalmStatus::Ok);
        assert_ne!(
            calm_engine_set_brush_kind(e, 99),
            CalmStatus::Ok,
            "an unknown id is still an error even before a project exists"
        );
        calm_engine_free(e);
    }
}

#[test]
fn knob_setters_reject_a_null_engine() {
    let e: *mut CalmEngine = ptr::null_mut();
    unsafe {
        assert_eq!(calm_engine_set_blur_strength(e, 0.5), CalmStatus::Null);
        assert_eq!(calm_engine_set_tolerance(e, 10), CalmStatus::Null);
        assert_eq!(calm_engine_set_brush_kind(e, 0), CalmStatus::Null);
        assert_eq!(calm_engine_set_eraser_hardness(e, 0.5), CalmStatus::Null);
    }
}

/// The shell builds its slider ranges from these, so a min above a max would silently invert
/// a control, and a default outside the range would open it in an unreachable position.
#[test]
fn limit_accessors_bracket_their_defaults() {
    let checks: [(f32, f32, f32, &str); 3] = [
        (
            calm_blur_strength_min(),
            calm_blur_strength_max(),
            calm_blur_strength_default(),
            "blur strength",
        ),
        (
            calm_eraser_hardness_min(),
            calm_eraser_hardness_max(),
            calm_eraser_hardness_default(),
            "eraser hardness",
        ),
        (
            calm_ink_opacity_min(),
            calm_ink_opacity_max(),
            calm_ink_opacity_default(),
            "ink opacity",
        ),
    ];
    for (min, max, default, what) in checks {
        assert!(min < max, "{what}: min below max");
        assert!(
            (min..=max).contains(&default),
            "{what}: default {default} inside {min}..={max}"
        );
    }

    let (min, max, default) = (
        calm_tolerance_min(),
        calm_tolerance_max(),
        calm_tolerance_default(),
    );
    assert!(min < max, "tolerance: min below max");
    assert!((min..=max).contains(&default));
}

/// Which controls a tool shows is a product rule, so the shell asks rather than deciding. Each
/// predicate has to be true for something and false for something else — a predicate stuck at
/// one answer would show every control on every tool, or none of them anywhere.
#[test]
fn tool_predicates_answer_per_tool() {
    let cases: [(&str, extern "C" fn(u32) -> u8, Tool, Tool); 5] = [
        ("brush", calm_tool_takes_brush, Tool::Pen, Tool::Eraser),
        (
            "eraser hardness",
            calm_tool_takes_eraser_hardness,
            Tool::Eraser,
            Tool::Pen,
        ),
        (
            "blur strength",
            calm_tool_takes_blur_strength,
            Tool::Blur,
            Tool::Pen,
        ),
        (
            "tolerance",
            calm_tool_takes_tolerance,
            Tool::Fill,
            Tool::Pen,
        ),
        (
            "selection",
            calm_tool_is_selection,
            Tool::MagicWand,
            Tool::Pen,
        ),
    ];
    for (what, predicate, yes, no) in cases {
        assert_eq!(predicate(yes as u32), 1, "{what} is shown for {yes:?}");
        assert_eq!(predicate(no as u32), 0, "{what} is hidden for {no:?}");
        assert_eq!(
            predicate(9999),
            0,
            "{what}: an unknown tool id answers no rather than panicking"
        );
    }
}

/// The wand joined the select family, so it has to be reachable the way the other three are —
/// picking it must make it the tool the `M` shortcut and the flyout return to.
#[test]
fn the_magic_wand_is_remembered_as_the_last_select_tool() {
    let (_dir, e) = engine_with_project(32, 32);
    unsafe {
        assert_eq!(
            calm_engine_set_tool(e, Tool::MagicWand as u32),
            CalmStatus::Ok
        );
        let mut state = std::mem::zeroed::<CalmState>();
        assert_eq!(calm_engine_state(e, &mut state), CalmStatus::Ok);
        assert_eq!(state.last_select_tool, Tool::MagicWand as u32);
        calm_engine_free(e);
    }
}

/// Blur is a stamping tool, so it takes a size; it has no ink, so it takes neither colour
/// opacity nor a brush. Getting this pair wrong is what puts a meaningless slider on a panel.
#[test]
fn blur_takes_a_size_but_no_ink() {
    assert_eq!(calm_tool_takes_brush_size(Tool::Blur as u32), 1);
    assert_eq!(calm_tool_takes_ink_opacity(Tool::Blur as u32), 0);
    assert_eq!(calm_tool_takes_brush(Tool::Blur as u32), 0);
}
