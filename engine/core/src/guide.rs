use crate::document::Document;
use crate::limits::{GUIDES_LIMIT, GUIDE_MIN_SEPARATION, GUIDE_PICK_SLACK_PX, GUIDE_SNAP_PX};
use num_enum::{IntoPrimitive, TryFromPrimitive};

/// Which way a guide runs. A `Horizontal` guide is a horizontal rule at document *y*, the one
/// you pull off the top ruler; a `Vertical` guide is a vertical rule at document *x*, off the
/// left ruler. The axis names the line, not the coordinate it stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum GuideAxis {
    Horizontal = 0,
    Vertical = 1,
}

impl GuideAxis {
    pub fn from_u8(v: u8) -> Option<Self> {
        Self::try_from(v).ok()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Guide {
    pub axis: GuideAxis,
    /// Document pixels along the axis the guide crosses — `y` for a horizontal rule, `x` for a
    /// vertical one.
    pub position: f32,
}

/// A guide being dragged, by index into `Document.guides`. Nothing else may add or remove a
/// guide while one is in flight, so an index is a stable address for the length of the drag —
/// and the guide itself already knows its axis.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GuideDrag {
    pub(crate) index: usize,
}

/// The nearest guide on `axis` to any of `edges`, as the displacement that would put that edge
/// on it — `0.0` when nothing is close enough. Every snap in the engine is this function: a
/// point is a box with one edge, a moving layer is a box with three (both sides and the
/// middle), and the winner is whichever pairing moves the least.
fn nearest_delta(guides: &[Guide], axis: GuideAxis, edges: &[f32], threshold: f32) -> f32 {
    let mut best = 0.0f32;
    let mut best_dist = threshold;
    for guide in guides.iter().filter(|g| g.axis == axis) {
        for edge in edges {
            let delta = guide.position - edge;
            if delta.abs() <= best_dist {
                best_dist = delta.abs();
                best = delta;
            }
        }
    }
    best
}

fn box_edges(lo: f32, hi: f32) -> [f32; 3] {
    [lo, (lo + hi) * 0.5, hi]
}

impl Document {
    pub fn guides(&self) -> &[Guide] {
        &self.guides
    }

    /// Drops a guide at a document position. Refuses a duplicate — dragging one guide onto
    /// another leaves one rule, not two you can never separate again — and refuses to grow the
    /// list past `GUIDES_LIMIT`. Returns the index of the guide that now holds that position.
    pub fn add_guide(&mut self, axis: GuideAxis, position: f32) -> Option<usize> {
        if !position.is_finite() {
            return None;
        }
        if let Some(existing) = self.guide_index_near(axis, position, GUIDE_MIN_SEPARATION) {
            return Some(existing);
        }
        if self.guides.len() >= GUIDES_LIMIT {
            return None;
        }
        self.guides.push(Guide { axis, position });
        Some(self.guides.len() - 1)
    }

    pub fn remove_guide(&mut self, index: usize) -> bool {
        if index >= self.guides.len() {
            return false;
        }
        self.guides.remove(index);
        if let Some(drag) = self.guide_drag {
            if drag.index == index {
                self.guide_drag = None;
            }
        }
        true
    }

    pub fn clear_guides(&mut self) -> bool {
        if self.guides.is_empty() {
            return false;
        }
        self.guides.clear();
        self.guide_drag = None;
        true
    }

    /// Replaces the whole list, as a project load does. Positions arrive already trusted from
    /// the store, so this only enforces the ceiling and drops the live drag.
    pub fn set_guides(&mut self, guides: Vec<Guide>) {
        self.guides = guides;
        self.guides.truncate(GUIDES_LIMIT);
        self.guide_drag = None;
    }

    /// The guide under a **screen** point, or `None`. Slack is a fixed number of screen pixels,
    /// so a hairline stays as easy to grab zoomed out as zoomed in.
    pub fn guide_at(&self, screen_x: f32, screen_y: f32) -> Option<usize> {
        if self.guides.is_empty() {
            return None;
        }
        let (doc_x, doc_y) = self.camera.to_doc(screen_x, screen_y);
        let slack = self.doc_units(GUIDE_PICK_SLACK_PX);
        self.guides
            .iter()
            .enumerate()
            .map(|(index, guide)| {
                let along = match guide.axis {
                    GuideAxis::Horizontal => doc_y,
                    GuideAxis::Vertical => doc_x,
                };
                (index, (guide.position - along).abs())
            })
            .filter(|&(_, dist)| dist <= slack)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index)
    }

    /// Grabs the guide under a screen point, if there is one. The gesture that follows is
    /// `update_guide_drag` / `end_guide_drag`, the same shape as every other drag here.
    pub fn begin_guide_drag(&mut self, screen_x: f32, screen_y: f32) -> bool {
        let Some(index) = self.guide_at(screen_x, screen_y) else {
            return false;
        };
        self.guide_drag = Some(GuideDrag { index });
        true
    }

    /// Pulls a new guide off a ruler. The screen point is the pointer in *board* coordinates,
    /// so a drag that has not left the ruler strip yet is simply a negative one — which is also
    /// what makes releasing it there throw the guide away (`end_guide_drag`).
    pub fn begin_guide_drag_from_ruler(
        &mut self,
        axis: GuideAxis,
        screen_x: f32,
        screen_y: f32,
    ) -> bool {
        let position = self.guide_position_at(axis, screen_x, screen_y);
        let Some(index) = self.add_guide(axis, position) else {
            return false;
        };
        self.guide_drag = Some(GuideDrag { index });
        true
    }

    pub fn update_guide_drag(&mut self, screen_x: f32, screen_y: f32) -> bool {
        let Some(drag) = self.guide_drag else {
            return false;
        };
        let Some(&guide) = self.guides.get(drag.index) else {
            return false;
        };
        let position = self.guide_position_at(guide.axis, screen_x, screen_y);
        self.guides[drag.index].position = position;
        true
    }

    /// Ends the drag, keeping the guide only if it landed on the paper. Dragging a guide back
    /// onto its ruler — or off any other edge — puts it outside the board, and a guide outside
    /// the board is one you threw away.
    pub fn end_guide_drag(&mut self) -> bool {
        let Some(drag) = self.guide_drag.take() else {
            return false;
        };
        let Some(&guide) = self.guides.get(drag.index) else {
            return false;
        };
        let extent = match guide.axis {
            GuideAxis::Horizontal => self.height as f32,
            GuideAxis::Vertical => self.width as f32,
        };
        if guide.position < 0.0 || guide.position > extent {
            self.guides.remove(drag.index);
        }
        true
    }

    pub fn is_dragging_guide(&self) -> bool {
        self.guide_drag.is_some()
    }

    /// The guide currently being dragged, so the board can pick it out of the rest.
    pub fn dragged_guide(&self) -> Option<usize> {
        self.guide_drag.map(|d| d.index)
    }

    /// Snaps a bare document point onto the guides near it — the pointer position a shape drag
    /// or a scale handle is about to be built from. Each axis is answered independently, so a
    /// corner can land on a horizontal and a vertical guide at once.
    pub(crate) fn snap_doc_point(&self, p: (f32, f32)) -> (f32, f32) {
        if self.guides.is_empty() {
            return p;
        }
        let threshold = self.doc_units(GUIDE_SNAP_PX);
        (
            p.0 + nearest_delta(&self.guides, GuideAxis::Vertical, &[p.0], threshold),
            p.1 + nearest_delta(&self.guides, GuideAxis::Horizontal, &[p.1], threshold),
        )
    }

    /// How far a box has to move for one of its edges — or its centre line — to land on a
    /// guide. This is what a *move* snaps with: the pointer keeps its grip on the layer and
    /// the layer's own outline is what sticks, which is the difference between Photoshop's
    /// snapping and a cursor that jumps.
    pub(crate) fn snap_box_offset(&self, aabb: (f32, f32, f32, f32)) -> (f32, f32) {
        if self.guides.is_empty() {
            return (0.0, 0.0);
        }
        let threshold = self.doc_units(GUIDE_SNAP_PX);
        (
            nearest_delta(
                &self.guides,
                GuideAxis::Vertical,
                &box_edges(aabb.0, aabb.2),
                threshold,
            ),
            nearest_delta(
                &self.guides,
                GuideAxis::Horizontal,
                &box_edges(aabb.1, aabb.3),
                threshold,
            ),
        )
    }

    fn doc_units(&self, screen_px: f32) -> f32 {
        screen_px / self.camera.zoom.max(1e-6)
    }

    fn guide_position_at(&self, axis: GuideAxis, screen_x: f32, screen_y: f32) -> f32 {
        let (doc_x, doc_y) = self.camera.to_doc(screen_x, screen_y);
        match axis {
            GuideAxis::Horizontal => doc_y,
            GuideAxis::Vertical => doc_x,
        }
    }

    fn guide_index_near(&self, axis: GuideAxis, position: f32, slack: f32) -> Option<usize> {
        self.guides
            .iter()
            .position(|g| g.axis == axis && (g.position - position).abs() <= slack)
    }
}
