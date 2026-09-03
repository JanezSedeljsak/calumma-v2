use calumma_core::GuideAxis;
use calumma_ffi::*;
use std::ffi::CString;

const SIDE: u32 = 400;

/// Drives guides through the same C entry points the rulers and the Board menu call, so the
/// ruler drag — which never goes through `calm_engine_pointer_*` — is covered across the
/// bridge rather than only in core.
struct GuideEngine {
    ptr: *mut CalmEngine,
    _dir: tempfile::TempDir,
}

impl GuideEngine {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = CString::new(dir.path().join("g.sqlite").to_str().unwrap()).unwrap();
        let ptr = unsafe { calm_engine_new(path.as_ptr()) };
        assert!(!ptr.is_null());
        let name = CString::new("Guides").unwrap();
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

    fn drag_from_ruler(&self, axis: GuideAxis, from: (f32, f32), to: (f32, f32)) {
        assert_eq!(
            calm_engine_guide_drag_from_ruler(self.ptr, u8::from(axis), from.0, from.1),
            CalmStatus::Ok
        );
        assert_eq!(
            calm_engine_guide_drag_update(self.ptr, to.0, to.1),
            CalmStatus::Ok
        );
        assert_eq!(
            calm_engine_guide_drag_end(self.ptr, to.0, to.1),
            CalmStatus::Ok
        );
    }
}

impl Drop for GuideEngine {
    fn drop(&mut self) {
        unsafe { calm_engine_free(self.ptr) };
    }
}

#[test]
fn a_ruler_drag_onto_the_board_leaves_a_guide() {
    let e = GuideEngine::new();
    assert_eq!(calm_engine_guide_count(e.ptr), 0);
    e.drag_from_ruler(GuideAxis::Horizontal, (200.0, -10.0), (200.0, 180.0));
    assert_eq!(calm_engine_guide_count(e.ptr), 1);
    assert_eq!(calm_engine_guide_axis_at(e.ptr, 200.0, 180.0), 0);
    assert_eq!(calm_engine_guide_axis_at(e.ptr, 200.0, 40.0), -1);
}

#[test]
fn a_ruler_drag_released_back_over_the_ruler_leaves_nothing() {
    let e = GuideEngine::new();
    e.drag_from_ruler(GuideAxis::Vertical, (-10.0, 200.0), (-4.0, 200.0));
    assert_eq!(calm_engine_guide_count(e.ptr), 0);
}

#[test]
fn clear_guides_empties_the_board() {
    let e = GuideEngine::new();
    e.drag_from_ruler(GuideAxis::Horizontal, (200.0, -10.0), (200.0, 120.0));
    e.drag_from_ruler(GuideAxis::Vertical, (-10.0, 200.0), (140.0, 200.0));
    assert_eq!(calm_engine_guide_count(e.ptr), 2);
    assert_eq!(calm_engine_clear_guides(e.ptr), CalmStatus::Ok);
    assert_eq!(calm_engine_guide_count(e.ptr), 0);
}

#[test]
fn guides_survive_closing_and_reopening_the_project() {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("g.sqlite").to_str().unwrap()).unwrap();
    let ptr = unsafe { calm_engine_new(path.as_ptr()) };
    let name = CString::new("Persisted").unwrap();
    let raw_id = unsafe { calm_project_create(ptr, name.as_ptr(), SIDE, SIDE) };
    let id = unsafe { std::ffi::CStr::from_ptr(raw_id) }
        .to_str()
        .unwrap()
        .to_owned();
    unsafe { calm_string_free(raw_id) };
    unsafe { calm_engine_resize(ptr, SIDE, SIDE, 1.0) };
    unsafe { calm_engine_fit(ptr) };

    calm_engine_guide_drag_from_ruler(ptr, u8::from(GuideAxis::Horizontal), 200.0, -10.0);
    calm_engine_guide_drag_end(ptr, 200.0, 175.0);
    assert_eq!(unsafe { calm_project_save(ptr) }, CalmStatus::Ok);

    let cid = CString::new(id).unwrap();
    assert_eq!(
        unsafe { calm_project_open(ptr, cid.as_ptr()) },
        CalmStatus::Ok
    );
    assert_eq!(calm_engine_guide_count(ptr), 1);
    unsafe { calm_engine_free(ptr) };
}

#[test]
fn guide_entry_points_answer_safely_with_no_engine() {
    assert_eq!(calm_engine_guide_count(std::ptr::null_mut()), 0);
    assert_eq!(
        calm_engine_guide_axis_at(std::ptr::null_mut(), 0.0, 0.0),
        -1
    );
    assert_eq!(
        calm_engine_clear_guides(std::ptr::null_mut()),
        CalmStatus::Null
    );
    assert_eq!(
        calm_engine_guide_drag_from_ruler(std::ptr::null_mut(), 0, 0.0, 0.0),
        CalmStatus::Null
    );
}

#[test]
fn an_unknown_axis_is_refused_rather_than_guessed() {
    let e = GuideEngine::new();
    assert_eq!(
        calm_engine_guide_drag_from_ruler(e.ptr, 9, 10.0, 10.0),
        CalmStatus::Error
    );
    assert_eq!(calm_engine_guide_count(e.ptr), 0);
}

fn guide_buffer<const N: usize>() -> [CalmGuide; N] {
    std::array::from_fn(|_| CalmGuide {
        axis: 0,
        position: 0.0,
        color: 0,
    })
}

/// A guide arrives in the default color and keeps whichever one it is given — the card reads
/// this back to fill its swatch, so a list that reported the wrong color would show the wrong
/// one on every row.
#[test]
fn a_guide_is_listed_in_the_color_it_is_drawn_in() {
    let e = GuideEngine::new();
    unsafe {
        assert_eq!(calm_engine_add_guide(e.ptr, 0, 40.0), CalmStatus::Ok);
        let mut buffer = guide_buffer::<4>();
        calm_engine_guide_list(e.ptr, buffer.as_mut_ptr(), buffer.len());
        assert_eq!(
            buffer[0].color,
            calm_default_guide_color(),
            "a new guide starts in the default color the card offers first"
        );

        assert_eq!(
            calm_engine_set_guide_color(e.ptr, 0, 0x0C_C8_5A),
            CalmStatus::Ok
        );
        calm_engine_guide_list(e.ptr, buffer.as_mut_ptr(), buffer.len());
        assert_eq!(buffer[0].color, 0x0C_C8_5A);
        assert_eq!(
            buffer[0].position, 40.0,
            "recoloring a guide leaves it where it was"
        );
    }
}

/// Every setter here is index-addressed and the indices shift as guides come and go, so an out
/// of range one has to be refused rather than land on whatever is there now.
#[test]
fn recoloring_a_guide_that_is_not_there_changes_nothing() {
    let e = GuideEngine::new();
    assert_eq!(
        calm_engine_set_guide_color(e.ptr, 7, 0x0C_C8_5A),
        CalmStatus::Ok
    );
    assert_eq!(calm_engine_guide_count(e.ptr), 0);
}

#[test]
fn the_guides_card_can_list_add_move_and_remove() {
    let e = GuideEngine::new();
    unsafe {
        assert_eq!(calm_engine_add_guide(e.ptr, 1, 40.0), CalmStatus::Ok);
        assert_eq!(calm_engine_add_guide(e.ptr, 0, 60.0), CalmStatus::Ok);

        let mut buf = guide_buffer::<8>();
        let n = calm_engine_guide_list(e.ptr, buf.as_mut_ptr(), buf.len());
        assert_eq!(n, 2);
        assert_eq!(buf[0].axis, 1);
        assert_eq!(buf[0].position, 40.0);
        assert_eq!(buf[1].axis, 0);
        assert_eq!(buf[1].position, 60.0);

        // Typed positions clamp onto the paper rather than being discarded the way a drag off
        // the edge is — someone typing a huge number meant the far edge.
        assert_eq!(
            calm_engine_set_guide_position(e.ptr, 1, 5000.0),
            CalmStatus::Ok
        );
        let n = calm_engine_guide_list(e.ptr, buf.as_mut_ptr(), buf.len());
        assert_eq!(n, 2);
        assert_eq!(buf[1].position, SIDE as f32);

        assert_eq!(calm_engine_remove_guide(e.ptr, 0), CalmStatus::Ok);
        let n = calm_engine_guide_list(e.ptr, buf.as_mut_ptr(), buf.len());
        assert_eq!(n, 1);
        assert_eq!(buf[0].axis, 0);
    }
}

#[test]
fn guide_list_survives_a_null_buffer_and_a_zero_cap() {
    let e = GuideEngine::new();
    unsafe {
        assert_eq!(calm_engine_add_guide(e.ptr, 0, 10.0), CalmStatus::Ok);
        assert_eq!(calm_engine_guide_list(e.ptr, std::ptr::null_mut(), 4), 0);
        let mut buf = guide_buffer::<1>();
        assert_eq!(calm_engine_guide_list(e.ptr, buf.as_mut_ptr(), 0), 0);
    }
}

#[test]
fn the_card_can_flip_a_guide_to_the_other_edge() {
    let e = GuideEngine::new();
    unsafe {
        assert_eq!(calm_engine_add_guide(e.ptr, 1, 60.0), CalmStatus::Ok);
        assert_eq!(calm_engine_set_guide_axis(e.ptr, 0, 0), CalmStatus::Ok);

        let mut buf = guide_buffer::<4>();
        let n = calm_engine_guide_list(e.ptr, buf.as_mut_ptr(), buf.len());
        assert_eq!(n, 1);
        assert_eq!(buf[0].axis, 0);
        assert_eq!(buf[0].position, 60.0);
    }
}
