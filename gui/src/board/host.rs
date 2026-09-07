use calumma_app::Engine;
use std::cell::RefCell;
use std::rc::Rc;

use super::cursor::{
    should_pan, should_track_hover, BoardCursorInput, CursorController, ModifierState,
};
use calumma_core::Tool;

#[cfg(target_os = "macos")]
use crate::board::surface_macos::BoardSurface;

pub struct BoardHost {
    engine: Rc<RefCell<Engine>>,
    active: bool,
    attached: bool,
    attach_failed: bool,
    panning: bool,
    stroke_active: bool,
    pointer_inside: bool,
    hover: (f32, f32),
    last_pan: (f32, f32),
    cursor: CursorController,
    #[cfg(target_os = "macos")]
    surface: Option<BoardSurface>,
}

impl BoardHost {
    pub fn new(engine: Rc<RefCell<Engine>>, icons_root: std::path::PathBuf) -> Self {
        Self {
            engine,
            active: false,
            attached: false,
            attach_failed: false,
            panning: false,
            stroke_active: false,
            pointer_inside: false,
            hover: (0.0, 0.0),
            last_pan: (0.0, 0.0),
            cursor: CursorController::new(icons_root),
            #[cfg(target_os = "macos")]
            surface: None,
        }
    }

    pub fn set_active(&mut self, active: bool) {
        self.active = active;
        if !active {
            self.pointer_inside = false;
            self.panning = false;
            self.stroke_active = false;
            self.cursor.reset();
            #[cfg(target_os = "macos")]
            if let Some(surface) = &self.surface {
                surface.set_hidden(true);
            }
        }
    }

    pub fn set_pointer_inside(&mut self, inside: bool) {
        self.pointer_inside = inside;
        if !inside && !self.panning && !self.stroke_active {
            self.engine.borrow_mut().clear_pointer_hover();
        }
    }

    pub fn refresh_cursor(&mut self, modal_open: bool, mods: ModifierState) {
        let engine = self.engine.borrow();
        let input = BoardCursorInput {
            pointer_inside: self.pointer_inside,
            panning: self.panning,
            painting: self.stroke_active,
            modal_open,
            hover_x: self.hover.0,
            hover_y: self.hover.1,
            mods,
        };
        self.cursor.refresh(&engine, &input);
    }

    pub fn try_attach_winit(
        &mut self,
        winit_window: &winit::window::Window,
        width: u32,
        height: u32,
    ) -> bool {
        if !self.active {
            return false;
        }
        if self.attached || self.attach_failed {
            return self.attached;
        }
        #[cfg(target_os = "macos")]
        {
            if let Ok(surface) = BoardSurface::install(winit_window) {
                let scale = surface.scale() as f32;
                let native = calumma_app::NativeSurface::MetalLayer {
                    layer: surface.layer_ptr(),
                };
                match self
                    .engine
                    .borrow_mut()
                    .attach(native, width, height, scale)
                {
                    Ok(()) => {
                        self.surface = Some(surface);
                        self.attached = true;
                    }
                    Err(_) => self.attach_failed = true,
                }
            } else {
                self.attach_failed = true;
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (winit_window, width, height);
            self.attach_failed = true;
        }
        self.attached
    }

    pub fn sync_geometry(
        &mut self,
        winit_window: &winit::window::Window,
        layout: &super::layout::BoardLayout,
        content_height: f32,
        scale: f32,
        overlay: bool,
    ) {
        if !self.active || overlay {
            #[cfg(target_os = "macos")]
            if let Some(surface) = &self.surface {
                surface.set_hidden(true);
            }
            return;
        }
        if !self.attached {
            self.try_attach_winit(winit_window, layout.width, layout.height);
        }
        #[cfg(target_os = "macos")]
        if let Some(surface) = &self.surface {
            surface.set_frame(
                layout.x as f64,
                layout.y as f64,
                layout.width as f64,
                layout.height as f64,
                content_height as f64,
            );
        }
        if self.attached {
            self.engine
                .borrow_mut()
                .resize(layout.width, layout.height, scale);
        }
    }

    pub fn render(&mut self) {
        if self.active && self.attached {
            self.engine.borrow_mut().render();
        }
    }

    pub fn pointer_pressed(&mut self, x: f32, y: f32, mods: ModifierState, middle_button: bool) {
        self.hover = (x, y);
        let tool = self.engine.borrow().active_tool().unwrap_or(Tool::Pen);
        if middle_button || should_pan(mods, tool) {
            self.panning = true;
            self.last_pan = (x, y);
            self.engine.borrow_mut().clear_pointer_hover();
            return;
        }
        self.stroke_active = true;
        let mut engine = self.engine.borrow_mut();
        engine.set_shift_held(mods.shift_held);
        engine.set_alt_held(mods.alt_held);
        engine.pointer_down(x, y);
    }

    pub fn pointer_moved(&mut self, x: f32, y: f32, mods: ModifierState, modal_open: bool) {
        self.hover = (x, y);
        if self.panning {
            let (lx, ly) = self.last_pan;
            self.engine.borrow_mut().pan(x - lx, y - ly);
            self.last_pan = (x, y);
            self.refresh_cursor(modal_open, mods);
            return;
        }
        if self.stroke_active {
            let mut engine = self.engine.borrow_mut();
            engine.set_shift_held(mods.shift_held);
            engine.set_alt_held(mods.alt_held);
            engine.pointer_move(x, y);
            drop(engine);
            self.refresh_cursor(modal_open, mods);
            return;
        }
        if self.engine.borrow().is_dragging_guide() {
            let mut engine = self.engine.borrow_mut();
            engine.set_shift_held(mods.shift_held);
            engine.update_guide_drag(x, y);
            drop(engine);
            self.refresh_cursor(modal_open, mods);
            return;
        }
        let tool = self.engine.borrow().active_tool().unwrap_or(Tool::Pen);
        let input = BoardCursorInput {
            pointer_inside: self.pointer_inside,
            panning: self.panning,
            painting: self.stroke_active,
            modal_open,
            hover_x: x,
            hover_y: y,
            mods,
        };
        if should_track_hover(&input, tool) {
            self.engine.borrow_mut().set_pointer_hover(x, y);
        } else {
            self.engine.borrow_mut().clear_pointer_hover();
        }
        self.refresh_cursor(modal_open, mods);
    }

    pub fn pointer_released(&mut self, x: f32, y: f32, mods: ModifierState, modal_open: bool) {
        self.hover = (x, y);
        if self.panning {
            self.panning = false;
            self.engine.borrow_mut().end_camera_motion();
            self.refresh_cursor(modal_open, mods);
            return;
        }
        if self.stroke_active {
            let mut engine = self.engine.borrow_mut();
            engine.set_shift_held(mods.shift_held);
            engine.set_alt_held(mods.alt_held);
            engine.pointer_up(x, y);
            self.stroke_active = false;
        } else if self.engine.borrow().is_dragging_guide() {
            self.engine.borrow_mut().end_guide_drag();
        }
        let tool = self.engine.borrow().active_tool().unwrap_or(Tool::Pen);
        let input = BoardCursorInput {
            pointer_inside: self.pointer_inside,
            panning: false,
            painting: false,
            modal_open,
            hover_x: x,
            hover_y: y,
            mods,
        };
        if should_track_hover(&input, tool) {
            self.engine.borrow_mut().set_pointer_hover(x, y);
        }
        self.refresh_cursor(modal_open, mods);
    }

    pub fn modifiers_changed(&mut self, mods: ModifierState, modal_open: bool) {
        let dragging = self.engine.borrow().is_dragging_guide();
        if self.stroke_active || dragging {
            let mut engine = self.engine.borrow_mut();
            engine.set_shift_held(mods.shift_held);
            engine.set_alt_held(mods.alt_held);
            if dragging {
                engine.update_guide_drag(self.hover.0, self.hover.1);
            }
        }
        let tool = self.engine.borrow().active_tool().unwrap_or(Tool::Pen);
        let input = BoardCursorInput {
            pointer_inside: self.pointer_inside,
            panning: self.panning,
            painting: self.stroke_active,
            modal_open,
            hover_x: self.hover.0,
            hover_y: self.hover.1,
            mods,
        };
        if should_track_hover(&input, tool) {
            self.engine
                .borrow_mut()
                .set_pointer_hover(self.hover.0, self.hover.1);
        } else {
            self.engine.borrow_mut().clear_pointer_hover();
        }
        self.refresh_cursor(modal_open, mods);
    }

    pub fn scroll(
        &mut self,
        x: f32,
        y: f32,
        delta_x: f32,
        delta_y: f32,
        alt_held: bool,
        meta_held: bool,
    ) {
        if alt_held || meta_held {
            self.engine.borrow_mut().zoom_scroll(x, y, delta_y, true);
        } else {
            self.engine.borrow_mut().pan_scroll(delta_x, delta_y, true);
        }
        self.engine.borrow_mut().end_camera_motion();
    }
}
