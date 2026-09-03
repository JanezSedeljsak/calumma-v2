//! Pasting an image into an already-open document, at the size it actually is.
//!
//! An oversized paste used to be cropped, silently and unrecoverably: `TileGrid::paint_rect`
//! opens with `rect.intersect(self.bounds())`, a grid was always exactly document-sized, and
//! the blit was anchored top-left — so pasting a 4000px photo into a 1000px board wrote the
//! top-left quarter and threw the rest away. Not a clipped *view* that moving the layer could
//! recover; the pixels were never written.
//!
//! The answer is neither to shrink the image nor to grow the paper. **The layer overflows.**
//! `TileGrid::grow_extent` opens the layer's storage wide enough for the whole image, which is
//! blitted at native resolution and centred on the canvas — so the middle of it is on the paper
//! and the rest hangs off the edges, where it stays until it is dragged into view. Nothing is
//! resampled and nothing is lost, and the document keeps the dimensions the user chose for it.
//!
//! Only the *storage* grew. The document is still `width` × `height`: masks are sized to it,
//! export walks it, and `Camera::paper_scissor` still clips every layer to it, so an
//! overflowing layer draws exactly the part of itself that is on the paper.

use crate::document::Document;
use crate::history::stack_snapshot_bytes;
use crate::tile::DocRect;

use num_enum::{IntoPrimitive, TryFromPrimitive};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u32)]
pub enum PasteOutcome {
    #[default]
    Failed = 0,
    Native = 1,
    Overflowing = 2,
}

impl Document {
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
        let fits = width <= self.width && height <= self.height;
        let (ox, oy) = if fits {
            self.selection_anchor()
        } else {
            self.centred(width, height)
        };

        let before = self.snapshot_stack();
        self.push_layer(name);
        let _index = self.active_layer;
        let placed = DocRect::new(ox, oy, ox + width as i32 - 1, oy + height as i32 - 1);
        let touched = match self.active_mut().and_then(|l| l.tiles_mut()) {
            Some(tiles) => {
                tiles.grow_extent(placed);
                tiles.blit_rgba_at(rgba, width, height, ox, oy)
            }
            None => 0,
        };
        if touched > 0 {
            let bytes = stack_snapshot_bytes(&before);
            self.history
                .push_stack(before, Some(self.active_layer), bytes);
            return if fits {
                PasteOutcome::Native
            } else {
                PasteOutcome::Overflowing
            };
        }
        self.restore_stack(before);
        PasteOutcome::Failed
    }

    /// Where a paste that fits lands. Predates the overflow work and is left alone
    /// deliberately: an image that fits on the paper is not what was broken.
    fn selection_anchor(&self) -> (i32, i32) {
        self.selection
            .as_ref()
            .map(|s| {
                let b = s.bounds();
                (b.min_x, b.min_y)
            })
            .unwrap_or((0, 0))
    }

    fn centred(&self, width: u32, height: u32) -> (i32, i32) {
        (
            (self.width as i32 - width as i32) / 2,
            (self.height as i32 - height as i32) / 2,
        )
    }
}
