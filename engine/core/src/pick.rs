//! Which layer is under this point — and, when the honest answer is "none", why.
//!
//! A click is not a point. It is a finger-sized region a person aimed at something with, and
//! the thing they aimed at may be one document pixel wide at a zoom where a document pixel is
//! a fraction of a screen pixel. Vector items have known this since they shipped
//! (`VECTOR_PICK_SLACK_PX`); raster layers tested exactly one pixel and lost the coin flip.
//!
//! Three rules, in the order they were three separate bugs:
//!
//! 1. **Slack.** The probe is a neighbourhood measured in *screen* pixels and converted to
//!    document units here, never by the shell, so a stroke stays as grabbable zoomed out as it
//!    is zoomed in.
//! 2. **A threshold, not `!= 0`.** An alpha of 1 is not something anyone can see, and it was
//!    claiming clicks well past a soft brush's visible edge.
//! 3. **A locked layer says so.** It stays unpickable — that is what lock means, and
//!    Photoshop falls through to what is underneath the same way — but the click no longer
//!    vanishes: the layer that refused it goes out through the same notice channel a blocked
//!    tool press uses, so it lands in the same toast rather than needing UI of its own.

use crate::document::{layer_alpha_at, Document};
use crate::layer::Layer;
use crate::limits::{
    LAYER_PICK_MAX_SLACK, LAYER_PICK_MIN_ALPHA, LAYER_PICK_SLACK_PX, MOVE_PICK_HALF,
};
use crate::tool_gate::ToolBlock;

/// A click widened into the region it actually means, in document units.
#[derive(Clone, Copy)]
pub(crate) struct PickProbe {
    x: f32,
    y: f32,
    slack: f32,
    doc_w: u32,
    doc_h: u32,
}

impl PickProbe {
    /// Whether anything in the probe clears the alpha threshold.
    ///
    /// The scan is a **one-document-pixel grid**, not a ring: painted content can be a single
    /// pixel wide, so any sampling coarser than that has gaps a hairline falls into — which
    /// is the bug, restated. It walks outward in square rings so the common case (the click
    /// actually landed on the thing) returns on the first sample, and it stops at the first
    /// hit rather than finding the strongest one, because the answer is a yes/no.
    fn hits(&self, layer: &Layer) -> bool {
        if self.sample(layer, 0, 0) {
            return true;
        }
        let reach = self.slack as i32;
        let radius_sq = self.slack * self.slack;
        for ring in 1..=reach {
            for dy in -ring..=ring {
                let step = if dy.abs() == ring { 1 } else { 2 * ring };
                let mut dx = -ring;
                while dx <= ring {
                    let offset_sq = (dx * dx + dy * dy) as f32;
                    if offset_sq <= radius_sq && self.sample(layer, dx, dy) {
                        return true;
                    }
                    dx += step;
                }
            }
        }
        false
    }

    fn hits_move(&self, layer: &Layer) -> bool {
        for dy in -MOVE_PICK_HALF..MOVE_PICK_HALF {
            for dx in -MOVE_PICK_HALF..MOVE_PICK_HALF {
                if self.sample(layer, dx, dy) {
                    return true;
                }
            }
        }
        false
    }

    fn sample(&self, layer: &Layer, dx: i32, dy: i32) -> bool {
        let alpha = layer_alpha_at(
            layer,
            self.x + dx as f32,
            self.y + dy as f32,
            self.doc_w,
            self.doc_h,
        );
        alpha >= LAYER_PICK_MIN_ALPHA
    }

    /// A cheap reject before the scan: the layer's painted box, transformed into document
    /// space and grown by the probe's reach. Most layers in a stack are nowhere near the
    /// click, and this is what keeps them from paying for the grid.
    fn may_reach(&self, layer: &Layer) -> bool {
        let Some(raw) = layer.content_bounds() else {
            return false;
        };
        let (x0, y0, x1, y1) = layer.transform.unwrap_or_default().transformed_aabb(raw);
        self.x >= x0 - self.slack
            && self.x <= x1 + self.slack
            && self.y >= y0 - self.slack
            && self.y <= y1 + self.slack
    }
}

/// Everything about a layer that makes it pickable *except* its lock, which is split out so
/// the same walk can find the locked layer that swallowed a click and name it.
///
/// `visible`, `opacity > 0.0` and `!is_paper()` are deliberate and predate this module: an
/// invisible layer that still had paint under the cursor used to grab every click there, which
/// made dragging look like a no-op because the thing that moved could not be seen.
fn eligible(layer: &Layer) -> bool {
    layer.visible
        && !layer.is_paper()
        && layer.opacity > 0.0
        && (layer.tiles().is_some() || layer.content.item().is_some())
}

#[derive(Clone, Copy)]
enum PickScan {
    Slack,
    Move,
}

impl PickScan {
    fn hits(self, probe: &PickProbe, layer: &Layer) -> bool {
        match self {
            Self::Slack => probe.hits(layer),
            Self::Move => probe.hits_move(layer),
        }
    }
}

impl Document {
    /// Screen pixels into document units, then clamped. Without the clamp, the 20% zoom floor
    /// on a large board turns six screen pixels into a hundred document pixels, and a pick
    /// that reaches that far is grabbing something the user did not point at. Past the clamp
    /// the honest answer is to zoom in.
    pub(crate) fn pick_probe(&self, doc_x: f32, doc_y: f32) -> PickProbe {
        PickProbe {
            x: doc_x,
            y: doc_y,
            slack: (LAYER_PICK_SLACK_PX / self.camera.zoom.max(1e-6)).min(LAYER_PICK_MAX_SLACK),
            doc_w: self.width,
            doc_h: self.height,
        }
    }

    /// The topmost unlocked layer under the point, or `None`.
    ///
    /// The document-bounds early-out stays: the board scissors every layer to the paper
    /// (`Camera::paper_scissor`), so content pushed outside it is not visible, and refusing to
    /// pick it is right rather than stingy.
    pub fn layer_at(&self, doc_x: f32, doc_y: f32) -> Option<usize> {
        self.topmost(doc_x, doc_y, false, PickScan::Slack)
    }

    pub fn layer_at_for_move(&self, doc_x: f32, doc_y: f32) -> Option<usize> {
        self.topmost(doc_x, doc_y, false, PickScan::Move)
    }

    /// The topmost *locked* layer under the point. Nothing picks with this — it exists so a
    /// click that fell through a locked layer can name what it fell through.
    pub fn locked_layer_at(&self, doc_x: f32, doc_y: f32) -> Option<usize> {
        self.topmost(doc_x, doc_y, true, PickScan::Slack)
    }

    pub fn locked_layer_at_for_move(&self, doc_x: f32, doc_y: f32) -> Option<usize> {
        self.topmost(doc_x, doc_y, true, PickScan::Move)
    }

    fn topmost(
        &self,
        doc_x: f32,
        doc_y: f32,
        locked: bool,
        scan: PickScan,
    ) -> Option<usize> {
        if doc_x < 0.0 || doc_y < 0.0 || doc_x >= self.width as f32 || doc_y >= self.height as f32 {
            return None;
        }
        let probe = self.pick_probe(doc_x, doc_y);
        self.layers
            .iter()
            .enumerate()
            .rev()
            .find(|(_, layer)| {
                layer.locked == locked
                    && eligible(layer)
                    && probe.may_reach(layer)
                    && scan.hits(&probe, layer)
            })
            .map(|(index, _)| index)
    }

    pub fn pick_layer(&mut self, doc_x: f32, doc_y: f32) -> Option<usize> {
        let index = self.layer_at(doc_x, doc_y)?;
        self.active_layer = index;
        Some(index)
    }

    pub fn pick_layer_for_move(&mut self, doc_x: f32, doc_y: f32) -> Option<usize> {
        let index = self.layer_at_for_move(doc_x, doc_y)?;
        self.active_layer = index;
        Some(index)
    }

    /// Called when a pick found nothing. If a locked layer is what the click landed on, that is
    /// the answer the user needs — routed through `blocked_notice` so it arrives as the same
    /// toast a blocked tool press produces, and keyed the same way so a run of presses on the
    /// same locked layer says it once rather than every time.
    pub(crate) fn note_locked_pick_for_move(&mut self, doc_x: f32, doc_y: f32) {
        let Some(index) = self.locked_layer_at_for_move(doc_x, doc_y) else {
            return;
        };
        let key = (index, self.tool);
        if self.blocked_notice_key != Some(key) {
            self.blocked_notice_key = Some(key);
            self.blocked_notice = Some(ToolBlock::LayerLocked);
        }
    }
}
