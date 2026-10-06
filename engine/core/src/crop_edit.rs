//! The Crop tool: a drag-anywhere rectangle that shrinks or grows the canvas, with an optional
//! locked aspect ratio and a composition-guide overlay. Committing hands off to
//! `Document::apply_canvas_shift` (`document.rs`), which is where the actual — non-destructive —
//! canvas resize happens; this module is only the interactive rectangle and its geometry.
//!
//! The rectangle is free to extend past the current canvas on any side (that's expansion) and
//! is not required to start at the canvas origin (that's a crop with a shifted origin) — both
//! are exactly what `apply_canvas_shift` was generalized to handle.

use crate::document::{point_dist, Document, HANDLE_HIT_RADIUS_PX};
use crate::limits::CROP_ZOOM_PADDING;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CropHandle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    Move,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CropDrag {
    pub(crate) handle: CropHandle,
    pub(crate) start_pointer: (f32, f32),
    pub(crate) start_rect: (f32, f32, f32, f32),
}

/// Floor on the rect's live size while dragging, in document units. Independent of
/// `MIN_CANVAS_SIDE` — that is enforced once, at commit; this only keeps a handle from dragging
/// the rectangle through itself mid-gesture.
const CROP_MIN_SIZE: f32 = 8.0;

impl Document {
    /// Enters the Crop tool: the rect starts as the whole canvas, and any left-over drag or
    /// aspect lock from a previous crop is cleared. Aspect lock and overlay style are shell
    /// knobs that persist across entries on purpose. Also zooms the camera out enough to leave
    /// room around the canvas to drag a handle past its edge — see `start_crop_session`.
    pub fn enter_crop(&mut self) {
        self.crop_saved_camera = Some(self.camera);
        self.start_crop_session();
    }

    /// Leaves the Crop tool without applying anything, restoring the camera `enter_crop` (or
    /// the last `commit_crop`) saved before it zoomed out for drag room.
    pub fn exit_crop(&mut self) {
        self.crop_rect = None;
        self.crop_drag = None;
        if let Some(camera) = self.crop_saved_camera.take() {
            self.camera = camera;
        }
    }

    /// Resets the crop rect to the current full canvas and, if the camera is zoomed in enough
    /// that the canvas already fills most of the viewport, zooms out to `CROP_ZOOM_PADDING` so
    /// there is real desk on screen around every edge — otherwise a corner or edge handle has
    /// nowhere on screen to drag *to* in order to expand the canvas, since the paper already
    /// fills almost the whole view at a normal fit. Only ever zooms out, never in, so a user
    /// already showing more desk than that keeps their own view. Shared by `enter_crop` and
    /// `commit_crop`, which re-arms a fresh crop session on the resized canvas.
    fn start_crop_session(&mut self) {
        self.crop_rect = Some((0.0, 0.0, self.width as f32, self.height as f32));
        self.crop_drag = None;
        let (w, h) = (self.width as f32, self.height as f32);
        let target_zoom = self.camera.fill_zoom(w, h, CROP_ZOOM_PADDING);
        if self.camera.zoom > target_zoom {
            self.camera.zoom = target_zoom;
            self.camera.center(w, h);
            self.camera.clamp_to_board(w, h);
        }
    }

    /// The rect the render overlay draws, in document space.
    pub fn crop_overlay_rect(&self) -> Option<(f32, f32, f32, f32)> {
        self.crop_rect
    }

    pub(crate) fn crop_handle_at(&self, doc_x: f32, doc_y: f32) -> Option<CropHandle> {
        let (x0, y0, x1, y1) = self.crop_rect?;
        let zoom = self.camera.zoom.max(1e-6);
        let hit_r = HANDLE_HIT_RADIUS_PX / zoom;
        let (mx, my) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
        let p = (doc_x, doc_y);
        let candidates = [
            (CropHandle::TopLeft, (x0, y0)),
            (CropHandle::Top, (mx, y0)),
            (CropHandle::TopRight, (x1, y0)),
            (CropHandle::Right, (x1, my)),
            (CropHandle::BottomRight, (x1, y1)),
            (CropHandle::Bottom, (mx, y1)),
            (CropHandle::BottomLeft, (x0, y1)),
            (CropHandle::Left, (x0, my)),
        ];
        for (handle, pos) in candidates {
            if point_dist(pos, p) <= hit_r {
                return Some(handle);
            }
        }
        (p.0 >= x0 && p.0 <= x1 && p.1 >= y0 && p.1 <= y1).then_some(CropHandle::Move)
    }

    pub(crate) fn begin_crop_drag(&mut self, doc_x: f32, doc_y: f32) -> bool {
        let Some(handle) = self.crop_handle_at(doc_x, doc_y) else {
            return false;
        };
        let Some(start_rect) = self.crop_rect else {
            return false;
        };
        self.crop_drag = Some(CropDrag {
            handle,
            start_pointer: (doc_x, doc_y),
            start_rect,
        });
        true
    }

    pub(crate) fn end_crop_drag(&mut self) {
        self.crop_drag = None;
    }

    /// Resizes or repositions `crop_rect` from the handle grabbed in `begin_crop_drag`. A corner
    /// keeps the *opposite* corner fixed and, under a locked ratio, grows along whichever axis
    /// the pointer moved further on — the same "diagonal reach" idea `corner_scale`
    /// (`transform.rs`) uses for the Transform box, just anchored at a corner instead of the
    /// center, since a crop rect's opposite corner is what a Photoshop-style drag holds still.
    /// An edge keeps the two edges perpendicular to it fixed and, under a locked ratio, grows
    /// the other axis symmetrically about the rect's own center — a deliberate, simpler
    /// convention than trying to match every tool's exact edge-drag behavior pixel for pixel.
    pub(crate) fn update_crop_drag(&mut self, doc_x: f32, doc_y: f32) {
        let Some(drag) = self.crop_drag else {
            return;
        };
        let (x0, y0, x1, y1) = drag.start_rect;
        let dx = doc_x - drag.start_pointer.0;
        let dy = doc_y - drag.start_pointer.1;
        let ratio = self.crop_aspect_lock.filter(|r| r.is_finite() && *r > 1e-6);

        let rect = match drag.handle {
            CropHandle::Move => (x0 + dx, y0 + dy, x1 + dx, y1 + dy),
            CropHandle::TopLeft
            | CropHandle::TopRight
            | CropHandle::BottomRight
            | CropHandle::BottomLeft => {
                let (anchor, signs): ((f32, f32), (f32, f32)) = match drag.handle {
                    CropHandle::TopLeft => ((x1, y1), (-1.0, -1.0)),
                    CropHandle::TopRight => ((x0, y1), (1.0, -1.0)),
                    CropHandle::BottomRight => ((x0, y0), (1.0, 1.0)),
                    CropHandle::BottomLeft => ((x1, y0), (-1.0, 1.0)),
                    _ => unreachable!("only the four corners reach this arm"),
                };
                let mut w = ((doc_x - anchor.0) * signs.0).max(CROP_MIN_SIZE);
                let mut h = ((doc_y - anchor.1) * signs.1).max(CROP_MIN_SIZE);
                if let Some(r) = ratio {
                    if w / r >= h {
                        h = w / r;
                    } else {
                        w = h * r;
                    }
                }
                let corner = (anchor.0 + w * signs.0, anchor.1 + h * signs.1);
                let (nx0, nx1) = order(anchor.0, corner.0);
                let (ny0, ny1) = order(anchor.1, corner.1);
                (nx0, ny0, nx1, ny1)
            }
            CropHandle::Left | CropHandle::Right => {
                let (mut nx0, mut nx1) = if drag.handle == CropHandle::Left {
                    (x0 + dx, x1)
                } else {
                    (x0, x1 + dx)
                };
                clamp_min_size(&mut nx0, &mut nx1, drag.handle == CropHandle::Left);
                match ratio {
                    Some(r) => {
                        let h = ((nx1 - nx0) / r).max(CROP_MIN_SIZE);
                        let cy = (y0 + y1) * 0.5;
                        (nx0, cy - h * 0.5, nx1, cy + h * 0.5)
                    }
                    None => (nx0, y0, nx1, y1),
                }
            }
            CropHandle::Top | CropHandle::Bottom => {
                let (mut ny0, mut ny1) = if drag.handle == CropHandle::Top {
                    (y0 + dy, y1)
                } else {
                    (y0, y1 + dy)
                };
                clamp_min_size(&mut ny0, &mut ny1, drag.handle == CropHandle::Top);
                match ratio {
                    Some(r) => {
                        let w = ((ny1 - ny0) * r).max(CROP_MIN_SIZE);
                        let cx = (x0 + x1) * 0.5;
                        (cx - w * 0.5, ny0, cx + w * 0.5, ny1)
                    }
                    None => (x0, ny0, x1, ny1),
                }
            }
        };
        self.crop_rect = Some(rect);
    }

    /// Applies the current rect: rounds it to whole pixels and hands off to
    /// `apply_canvas_shift`, which is where the actual resize happens. Leaves `Tool::Crop`
    /// armed with a fresh full-canvas rect on the resized document — including a fresh
    /// zoomed-out camera baseline for `exit_crop` to fall back to — so committing reads like
    /// committing a shape or a fill: the tool stays selected and ready to go again.
    pub fn commit_crop(&mut self) {
        let Some((x0, y0, x1, y1)) = self.crop_rect else {
            return;
        };
        let origin_x = x0.round() as i32;
        let origin_y = y0.round() as i32;
        let new_width = (x1.round() as i32 - origin_x).max(1) as u32;
        let new_height = (y1.round() as i32 - origin_y).max(1) as u32;
        self.apply_canvas_shift(origin_x, origin_y, new_width, new_height);
        self.crop_saved_camera = Some(self.camera);
        self.start_crop_session();
    }
}

fn order(a: f32, b: f32) -> (f32, f32) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Keeps the dragged edge from crossing the fixed one closer than `CROP_MIN_SIZE`.
/// `moving_min` is true when it is `min` (the low edge) being dragged — `Left`/`Top`.
fn clamp_min_size(min: &mut f32, max: &mut f32, moving_min: bool) {
    if *max - *min < CROP_MIN_SIZE {
        if moving_min {
            *min = *max - CROP_MIN_SIZE;
        } else {
            *max = *min + CROP_MIN_SIZE;
        }
    }
}
