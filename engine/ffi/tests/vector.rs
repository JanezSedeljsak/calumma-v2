use calumma_core::Tool;
use calumma_ffi::*;
use std::ffi::CString;

const SIDE: u32 = 400;

/// Drives the shape tools through the same C entry points the Swift shell calls, so vector
/// mode, item picking and item moving are exercised across the bridge rather than only in
/// core.
struct VectorEngine {
    ptr: *mut CalmEngine,
    _dir: tempfile::TempDir,
}

impl VectorEngine {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = CString::new(dir.path().join("v.sqlite").to_str().unwrap()).unwrap();
        let ptr = unsafe { calm_engine_new(path.as_ptr()) };
        assert!(!ptr.is_null());
        let name = CString::new("Vector").unwrap();
        let id = unsafe { calm_project_create(ptr, name.as_ptr(), SIDE, SIDE) };
        assert!(!id.is_null());
        unsafe { calm_string_free(id) };
        assert_eq!(
            unsafe { calm_engine_resize(ptr, SIDE, SIDE, 1.0) },
            CalmStatus::Ok
        );
        assert_eq!(unsafe { calm_engine_fit(ptr) }, CalmStatus::Ok);
        assert_eq!(
            unsafe { calm_engine_set_tool(ptr, Tool::Rect as u32) },
            CalmStatus::Ok
        );
        assert_eq!(calm_engine_set_vector_mode(ptr, 1), CalmStatus::Ok);
        Self { ptr, _dir: dir }
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

    fn vector_layer(&self) -> u32 {
        (0..16)
            .find(|i| calm_engine_layer_is_vector(self.ptr, *i) == 1)
            .expect("a vector layer should exist")
    }

    fn item_count(&self) -> u32 {
        calm_engine_layer_item_count(self.ptr, self.vector_layer())
    }

    fn selected(&self) -> i32 {
        calm_engine_selected_vector_item(self.ptr)
    }
}

impl Drop for VectorEngine {
    fn drop(&mut self) {
        unsafe { calm_engine_free(self.ptr) };
    }
}

#[test]
fn vector_mode_is_a_knob_the_shell_can_read_back() {
    let engine = VectorEngine::new();
    assert_eq!(calm_engine_vector_mode(engine.ptr), 1);
    assert_eq!(calm_engine_set_vector_mode(engine.ptr, 0), CalmStatus::Ok);
    assert_eq!(calm_engine_vector_mode(engine.ptr), 0);
}

#[test]
fn shapes_drawn_in_vector_mode_pile_up_in_one_layer() {
    let engine = VectorEngine::new();
    engine.drag((40.0, 40.0), (120.0, 120.0));
    engine.drag((200.0, 200.0), (300.0, 300.0));
    assert_eq!(engine.item_count(), 2);
    assert_eq!(
        calm_engine_layer_is_vector(engine.ptr, 0),
        0,
        "paper is not a vector layer"
    );
    assert_eq!(
        calm_engine_layer_is_vector(engine.ptr, 99),
        -1,
        "out of range reports -1"
    );
}

#[test]
fn an_item_is_selected_and_moved_by_dragging_it_in_transform_mode() {
    let engine = VectorEngine::new();
    engine.drag((40.0, 40.0), (120.0, 120.0));
    engine.drag((200.0, 200.0), (300.0, 300.0));
    assert_eq!(engine.selected(), -1);

    assert_eq!(
        unsafe { calm_engine_toggle_transform(engine.ptr) },
        CalmStatus::Ok
    );
    engine.drag((80.0, 80.0), (100.0, 90.0));
    assert_eq!(engine.selected(), 0);
    assert_eq!(engine.item_count(), 2, "moving an item adds nothing");

    assert_eq!(
        calm_engine_nudge_selected_vector_item(engine.ptr, 4.0, -3.0),
        CalmStatus::Ok
    );
    assert_eq!(
        calm_engine_delete_selected_vector_item(engine.ptr),
        CalmStatus::Ok
    );
    assert_eq!(engine.item_count(), 1);
    assert_eq!(engine.selected(), -1);
}

#[test]
fn the_selection_is_dropped_on_demand() {
    let engine = VectorEngine::new();
    engine.drag((40.0, 40.0), (120.0, 120.0));
    unsafe { calm_engine_toggle_transform(engine.ptr) };
    engine.drag((80.0, 80.0), (85.0, 85.0));
    assert_ne!(engine.selected(), -1);
    assert_eq!(
        calm_engine_clear_vector_selection(engine.ptr),
        CalmStatus::Ok
    );
    assert_eq!(engine.selected(), -1);
}

#[test]
fn vector_entry_points_survive_a_null_engine() {
    let null = std::ptr::null_mut();
    assert_eq!(calm_engine_set_vector_mode(null, 1), CalmStatus::Null);
    assert_eq!(calm_engine_vector_mode(null), 0);
    assert_eq!(calm_engine_layer_is_vector(null, 0), -1);
    assert_eq!(calm_engine_layer_item_count(null, 0), 0);
    assert_eq!(calm_engine_selected_vector_item(null), -1);
    assert_eq!(calm_engine_clear_vector_selection(null), CalmStatus::Null);
    assert_eq!(
        calm_engine_delete_selected_vector_item(null),
        CalmStatus::Null
    );
    assert_eq!(
        calm_engine_nudge_selected_vector_item(null, 1.0, 0.0),
        CalmStatus::Null
    );
}

#[test]
fn deleting_or_nudging_without_an_open_project_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = CString::new(dir.path().join("empty.sqlite").to_str().unwrap()).unwrap();
    let engine = unsafe { calm_engine_new(path.as_ptr()) };
    assert!(!engine.is_null());

    assert_eq!(
        calm_engine_delete_selected_vector_item(engine),
        CalmStatus::Error
    );
    assert_eq!(
        calm_engine_nudge_selected_vector_item(engine, 1.0, 0.0),
        CalmStatus::Error
    );

    unsafe { calm_engine_free(engine) };
}
