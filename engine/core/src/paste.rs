//! Pasting an image into an already-open document, and what happens when it does not fit.
//!
//! It used to be cropped, silently and unrecoverably: `TileGrid::paint_rect` opens with
//! `rect.intersect(self.bounds())`, a grid is always exactly document-sized, and the blit was
//! anchored top-left — so pasting a 4000px photo into a 1000px board wrote the top-left
//! quarter and threw the rest away. Not a clipped *view* that moving the layer could recover;
//! the pixels were never written. That is the one outcome an import path must never have.
//!
//! Two ways out, and core picks the default rather than the shell:
//!
//! - **`ScaleToFit`** downsamples the incoming pixels so the whole image lands on the paper.
//!   It is lossy in exactly the way the crop was — but it loses *detail* rather than losing
//!   *content*, and what is lost is visible on the board where it can be acted on.
//! - **`GrowCanvas`** grows the paper to hold the image at native size first.
//!
//! `ScaleToFit` is the default because it is the only one of the two that changes nothing the
//! user already has. Silently rewriting a document's dimensions is a bigger surprise than a
//! pasted image arriving smaller than it started.
//!
//! **An image that already fits is not touched by any of this** — it still lands at the
//! selection's top-left the way it always has. Only the oversized path centres.

use crate::document::Document;
use crate::limits::MAX_CANVAS_SIDE;
use crate::resample::{box_downsample, fit_within};

use num_enum::{IntoPrimitive, TryFromPrimitive};

/// What to do with a paste too big for the paper. Crosses the FFI as its discriminant.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u32)]
pub enum PasteFit {
    #[default]
    ScaleToFit = 0,
    GrowCanvas = 1,
}

impl PasteFit {
    pub fn from_u32(value: u32) -> Option<Self> {
        Self::try_from(value).ok()
    }
}

/// What a paste actually did, so the shell can say so and offer the other option. The shell
/// never infers this by comparing sizes itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u32)]
pub enum PasteOutcome {
    #[default]
    Failed = 0,
    /// It fit. Nothing was scaled and nothing grew.
    Native = 1,
    /// Downsampled to fit the paper.
    Scaled = 2,
    /// The paper grew; the image landed at native size.
    Grown = 3,
    /// The paper grew as far as `MAX_CANVAS_SIDE` allows and the remainder was scaled — the
    /// two modes composing rather than one of them failing outright.
    GrownAndScaled = 4,
}

impl Document {
    pub fn set_paste_fit(&mut self, fit: PasteFit) {
        self.paste_fit = fit;
    }

    pub fn paste_fit(&self) -> PasteFit {
        self.paste_fit
    }

    pub fn paste_image_as_layer(
        &mut self,
        name: impl Into<String>,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> PasteOutcome {
        let expected = (width as usize) * (height as usize) * 4;
        if width == 0 || height == 0 || rgba.len() < expected {
            return PasteOutcome::Failed;
        }
        if width <= self.width && height <= self.height {
            let (ox, oy) = self.selection_anchor();
            return self.place(name, rgba, width, height, ox, oy, PasteOutcome::Native);
        }
        match self.paste_fit {
            PasteFit::ScaleToFit => self.paste_scaled(name, rgba, width, height, false),
            PasteFit::GrowCanvas => self.paste_grown(name, rgba, width, height),
        }
    }

    /// Where a paste that fits lands. Predates this module and is left alone deliberately: it
    /// is shipped behaviour, and an image that fits is not what was broken.
    fn selection_anchor(&self) -> (i32, i32) {
        self.selection
            .as_ref()
            .map(|s| {
                let b = s.bounds();
                (b.min_x, b.min_y)
            })
            .unwrap_or((0, 0))
    }

    fn paste_scaled(
        &mut self,
        name: impl Into<String>,
        rgba: &[u8],
        width: u32,
        height: u32,
        grew: bool,
    ) -> PasteOutcome {
        let (w, h) = fit_within(width, height, self.width, self.height);
        let scaled = box_downsample(rgba, width, height, w, h);
        let (ox, oy) = self.centred(w, h);
        let outcome = if grew {
            PasteOutcome::GrownAndScaled
        } else {
            PasteOutcome::Scaled
        };
        self.place(name, &scaled, w, h, ox, oy, outcome)
    }

    /// Grow the paper, then paste. `Document::resize` clamps to `MAX_CANVAS_SIDE`, so an image
    /// past that leaves a remainder — which composes into `paste_scaled` rather than failing.
    ///
    /// The grow is **top-left anchored**, the same as a manual canvas resize: existing artwork
    /// keeps the coordinates it had. The paste itself is centred in the new paper, so the two
    /// do not land on top of each other, and the shell drops the new layer straight into `⌘T`
    /// for the move that follows.
    fn paste_grown(
        &mut self,
        name: impl Into<String>,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> PasteOutcome {
        self.resize(
            self.width.max(width).min(MAX_CANVAS_SIDE),
            self.height.max(height).min(MAX_CANVAS_SIDE),
        );
        if width > self.width || height > self.height {
            return self.paste_scaled(name, rgba, width, height, true);
        }
        let (ox, oy) = self.centred(width, height);
        self.place(name, rgba, width, height, ox, oy, PasteOutcome::Grown)
    }

    fn centred(&self, width: u32, height: u32) -> (i32, i32) {
        (
            (self.width as i32 - width as i32) / 2,
            (self.height as i32 - height as i32) / 2,
        )
    }

    /// Adds the layer and blits into it, taking the layer back out when nothing was written —
    /// an entirely transparent image, or a blit that landed nowhere. Leaving an empty layer
    /// behind to explain a paste that is about to report failure is worse than either.
    #[allow(clippy::too_many_arguments)]
    fn place(
        &mut self,
        name: impl Into<String>,
        rgba: &[u8],
        width: u32,
        height: u32,
        ox: i32,
        oy: i32,
        outcome: PasteOutcome,
    ) -> PasteOutcome {
        self.add_layer(name);
        let index = self.active_layer;
        let touched = match self.active_mut().and_then(|l| l.tiles_mut()) {
            Some(tiles) => tiles.blit_rgba_at(rgba, width, height, ox, oy),
            None => 0,
        };
        if touched > 0 {
            return outcome;
        }
        self.remove_layer(index);
        PasteOutcome::Failed
    }
}
