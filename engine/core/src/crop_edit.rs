//! The Crop tool: a drag-anywhere rectangle that shrinks or grows the canvas, with an optional
//! locked aspect ratio and a composition-guide overlay. Committing hands off to
//! `Document::apply_canvas_shift` (`document.rs`), which is where the actual — non-destructive —
//! canvas resize happens; this module is only the interactive rectangle and its geometry.
//!
//! The rectangle is free to extend past the current canvas on any side (that's expansion) and
//! is not required to start at the canvas origin (that's a crop with a shifted origin) — both
//! are exactly what `apply_canvas_shift` was generalized to handle.

use crate::document::{point_dist, Document, HANDLE_HIT_RADIUS_PX};
use crate::transform::bounds_center;
use num_enum::{IntoPrimitive, TryFromPrimitive};

/// Which composition guide the crop overlay draws over the rect while dragging. Pure display —
/// none of these feed back into the commit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u32)]
pub enum CropOverlayStyle {
    #[default]
    Off = 0,
    RuleOfThirds = 1,
    Grid = 2,
    Diagonal = 3,
    GoldenRatio = 4,
}

impl CropOverlayStyle {
    pub fn from_u32(value: u32) -> Option<Self> {
        Self::try_from(value).ok()
    }
}

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
    /// knobs that persist across entries on purpose.
    pub fn enter_crop(&mut self) {
        self.crop_rect = Some((0.0, 0.0, self.width as f32, self.height as f32));
        self.crop_drag = None;
        self.straighten_line = None;
        self.straighten_active = false;
    }

    /// Leaves the Crop tool without applying anything.
    pub fn exit_crop(&mut self) {
        self.crop_rect = None;
        self.crop_drag = None;
        self.straighten_line = None;
        self.straighten_active = false;
    }

    /// The rect the render overlay draws, in document space.
    pub fn crop_overlay_rect(&self) -> Option<(f32, f32, f32, f32)> {
        self.crop_rect
    }

    /// The reference line straighten is levelling, while it is being dragged.
    pub fn straighten_overlay_line(&self) -> Option<((f32, f32), (f32, f32))> {
        self.straighten_line
    }

    /// The guide segments the overlay draws over the current rect for `crop_overlay_style`.
    pub fn crop_overlay_lines(&self) -> Vec<((f32, f32), (f32, f32))> {
        let Some(rect) = self.crop_rect else {
            return Vec::new();
        };
        overlay_lines_for(rect, self.crop_overlay_style)
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

    /// Starts a straighten drag: the reference line the user is dragging across a tilted
    /// feature (a horizon, say), both ends at the press point until it moves.
    pub fn begin_straighten(&mut self, doc_x: f32, doc_y: f32) {
        self.straighten_line = Some(((doc_x, doc_y), (doc_x, doc_y)));
    }

    pub fn update_straighten(&mut self, doc_x: f32, doc_y: f32) {
        if let Some((p0, _)) = self.straighten_line {
            self.straighten_line = Some((p0, (doc_x, doc_y)));
        }
    }

    /// Releasing the drag levels the line immediately — the canvas visibly rotates and the crop
    /// rect stays exactly where it was, corners and all, for the user to trim inward before the
    /// final `commit_crop`. Matches Photoshop's own two-step flow (straighten, then adjust the
    /// crop) rather than trying to compute the largest rect the rotated canvas could still fill.
    pub fn end_straighten(&mut self) {
        self.commit_straighten();
        self.straighten_active = false;
    }

    /// Rotates every layer, uniformly, about the canvas center by the angle that levels
    /// `straighten_line` — see `LayerTransform::composed_with_rotation`. Left as a live
    /// transform rather than baked into fresh tile pixels: the same non-destructive shape
    /// `apply_canvas_shift` already uses, so undo is the ordinary transform-drag path and a
    /// second straighten composes cleanly with the first instead of compounding resampling
    /// error.
    fn commit_straighten(&mut self) {
        let Some((p0, p1)) = self.straighten_line.take() else {
            return;
        };
        let theta = (p1.1 - p0.1).atan2(p1.0 - p0.0);
        if !theta.is_finite() || theta.abs() < 1e-6 {
            return;
        }
        self.record_stack_history();
        let canvas_center = (self.width as f32 * 0.5, self.height as f32 * 0.5);
        for layer in &mut self.layers {
            let Some(bounds) = layer.content_bounds() else {
                continue;
            };
            let pivot = bounds_center(bounds);
            let t = layer.transform.unwrap_or_default();
            layer.transform = Some(
                t.composed_with_rotation(canvas_center, pivot, theta)
                    .clamped(),
            );
        }
    }

    /// Applies the current rect: rounds it to whole pixels and hands off to
    /// `apply_canvas_shift`, which is where the actual resize happens.
    pub fn commit_crop(&mut self) {
        let Some((x0, y0, x1, y1)) = self.crop_rect else {
            return;
        };
        let origin_x = x0.round() as i32;
        let origin_y = y0.round() as i32;
        let new_width = (x1.round() as i32 - origin_x).max(1) as u32;
        let new_height = (y1.round() as i32 - origin_y).max(1) as u32;
        self.apply_canvas_shift(origin_x, origin_y, new_width, new_height);
        self.crop_rect = None;
        self.crop_drag = None;
        self.straighten_line = None;
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

fn overlay_lines_for(
    rect: (f32, f32, f32, f32),
    style: CropOverlayStyle,
) -> Vec<((f32, f32), (f32, f32))> {
    let (x0, y0, x1, y1) = rect;
    let (w, h) = (x1 - x0, y1 - y0);
    let fractions: Vec<f32> = match style {
        CropOverlayStyle::Off => return Vec::new(),
        CropOverlayStyle::Diagonal => {
            return vec![((x0, y0), (x1, y1)), ((x1, y0), (x0, y1))];
        }
        CropOverlayStyle::RuleOfThirds => vec![1.0 / 3.0, 2.0 / 3.0],
        CropOverlayStyle::Grid => vec![0.25, 0.5, 0.75],
        CropOverlayStyle::GoldenRatio => {
            // 1/φ, the golden section — the same split on both axes, mirrored, the way
            // Photoshop's own Golden Ratio overlay divides the rect.
            let inv_phi = 2.0 / (1.0 + 5f32.sqrt());
            vec![1.0 - inv_phi, inv_phi]
        }
    };
    fractions
        .into_iter()
        .flat_map(|f| {
            [
                ((x0 + w * f, y0), (x0 + w * f, y1)),
                ((x0, y0 + h * f), (x1, y0 + h * f)),
            ]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;

    fn doc() -> Document {
        let mut d = Document::new("p".into(), "t", 200, 100);
        d.resize_viewport(200.0, 100.0, 1.0);
        d.fit_to_view();
        d
    }

    #[test]
    fn entering_crop_starts_at_the_full_canvas() {
        let mut d = doc();
        d.enter_crop();
        assert_eq!(d.crop_overlay_rect(), Some((0.0, 0.0, 200.0, 100.0)));
    }

    #[test]
    fn exiting_crop_discards_the_rect() {
        let mut d = doc();
        d.enter_crop();
        d.exit_crop();
        assert_eq!(d.crop_overlay_rect(), None);
    }

    #[test]
    fn dragging_the_bottom_right_corner_keeps_the_top_left_fixed() {
        let mut d = doc();
        d.enter_crop();
        assert!(d.begin_crop_drag(200.0, 100.0));
        d.update_crop_drag(150.0, 260.0);
        assert_eq!(d.crop_overlay_rect(), Some((0.0, 0.0, 150.0, 260.0)));
    }

    #[test]
    fn dragging_the_top_left_corner_keeps_the_bottom_right_fixed() {
        let mut d = doc();
        d.enter_crop();
        assert!(d.begin_crop_drag(0.0, 0.0));
        d.update_crop_drag(40.0, 20.0);
        assert_eq!(d.crop_overlay_rect(), Some((40.0, 20.0, 200.0, 100.0)));
    }

    #[test]
    fn a_corner_drag_may_expand_the_canvas_past_its_own_edge() {
        let mut d = doc();
        d.enter_crop();
        assert!(d.begin_crop_drag(200.0, 100.0));
        d.update_crop_drag(260.0, 140.0);
        assert_eq!(d.crop_overlay_rect(), Some((0.0, 0.0, 260.0, 140.0)));
    }

    #[test]
    fn locked_aspect_keeps_every_corner_drag_on_ratio() {
        let mut d = doc();
        d.enter_crop();
        d.crop_aspect_lock = Some(2.0);
        assert!(d.begin_crop_drag(200.0, 100.0));
        // Reaches further on x than a 2:1 box would need for that y, so x drives.
        d.update_crop_drag(300.0, 130.0);
        let (x0, y0, x1, y1) = d.crop_overlay_rect().unwrap();
        assert_eq!((x0, y0), (0.0, 0.0));
        assert!((((x1 - x0) / (y1 - y0)) - 2.0).abs() < 1e-4);
    }

    #[test]
    fn dragging_an_edge_with_a_locked_ratio_grows_the_other_axis_around_the_center() {
        let mut d = doc();
        d.enter_crop();
        d.crop_aspect_lock = Some(2.0);
        assert!(d.begin_crop_drag(200.0, 50.0)); // right edge midpoint
        d.update_crop_drag(300.0, 999.0); // y component of the pointer must not matter here
        let (x0, y0, x1, y1) = d.crop_overlay_rect().unwrap();
        assert_eq!((x0, x1), (0.0, 300.0));
        let cy = (y0 + y1) * 0.5;
        assert!(
            (cy - 50.0).abs() < 1e-4,
            "the vertical center must not move"
        );
        assert!((((x1 - x0) / (y1 - y0)) - 2.0).abs() < 1e-4);
    }

    #[test]
    fn dragging_inside_the_rect_moves_it_without_resizing() {
        let mut d = doc();
        d.enter_crop();
        assert!(d.begin_crop_drag(100.0, 50.0));
        d.update_crop_drag(130.0, 70.0);
        assert_eq!(d.crop_overlay_rect(), Some((30.0, 20.0, 230.0, 120.0)));
    }

    #[test]
    fn a_handle_cannot_drag_the_rect_through_itself() {
        let mut d = doc();
        d.enter_crop();
        assert!(d.begin_crop_drag(200.0, 100.0)); // bottom-right, anchored at (0,0)
        d.update_crop_drag(-500.0, -500.0);
        let (x0, y0, x1, y1) = d.crop_overlay_rect().unwrap();
        assert!(x1 - x0 >= CROP_MIN_SIZE - 1e-4);
        assert!(y1 - y0 >= CROP_MIN_SIZE - 1e-4);
    }

    #[test]
    fn clicking_outside_the_rect_hits_no_handle() {
        let mut d = doc();
        d.enter_crop();
        assert_eq!(d.crop_handle_at(-50.0, -50.0), None);
    }

    #[test]
    fn commit_crop_applies_the_rounded_rect_and_clears_state() {
        let mut d = doc();
        d.enter_crop();
        assert!(d.begin_crop_drag(200.0, 100.0));
        d.update_crop_drag(150.4, 80.6);
        d.commit_crop();
        assert_eq!((d.width, d.height), (150, 81));
        assert_eq!(d.crop_overlay_rect(), None);
    }

    #[test]
    fn overlay_lines_are_off_by_default_and_populate_per_style() {
        let mut d = doc();
        d.enter_crop();
        assert!(d.crop_overlay_lines().is_empty());

        d.crop_overlay_style = CropOverlayStyle::RuleOfThirds;
        assert_eq!(d.crop_overlay_lines().len(), 4);

        d.crop_overlay_style = CropOverlayStyle::Grid;
        assert_eq!(d.crop_overlay_lines().len(), 6);

        d.crop_overlay_style = CropOverlayStyle::Diagonal;
        assert_eq!(d.crop_overlay_lines().len(), 2);

        d.crop_overlay_style = CropOverlayStyle::GoldenRatio;
        assert_eq!(d.crop_overlay_lines().len(), 4);
    }

    #[test]
    fn rule_of_thirds_lines_sit_at_the_thirds() {
        let mut d = doc();
        d.enter_crop();
        d.crop_overlay_style = CropOverlayStyle::RuleOfThirds;
        let lines = d.crop_overlay_lines();
        let xs: Vec<f32> = lines
            .iter()
            .filter(|(a, b)| a.0 == b.0)
            .map(|(a, _)| a.0)
            .collect();
        assert!(xs.iter().any(|&x| (x - 200.0 / 3.0).abs() < 1e-3));
        assert!(xs.iter().any(|&x| (x - 400.0 / 3.0).abs() < 1e-3));
    }
}
