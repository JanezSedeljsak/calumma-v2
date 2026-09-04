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
use crate::limits::PASTE_STAGGER_PX;
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

pub struct PasteImage<'a> {
    pub name: &'a str,
    pub rgba: &'a [u8],
    pub width: u32,
    pub height: u32,
}

impl Document {
    pub fn paste_image_as_layer(
        &mut self,
        name: impl Into<String>,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> PasteOutcome {
        let (ox, oy) = self.paste_origin(width, height, 0);
        let before = self.snapshot_stack();
        let outcome = self.paste_image_as_layer_at_inner(name, rgba, width, height, ox, oy, true);
        if outcome == PasteOutcome::Failed {
            self.restore_stack(before);
            return PasteOutcome::Failed;
        }
        let bytes = stack_snapshot_bytes(&before);
        self.history
            .push_stack(before, Some(self.active_layer), bytes);
        outcome
    }

    pub fn paste_images_as_layers(&mut self, images: &[PasteImage<'_>]) -> (usize, PasteOutcome) {
        if images.is_empty() {
            return (0, PasteOutcome::Failed);
        }
        let before = self.snapshot_stack();
        let mut pasted = 0usize;
        let mut outcome = PasteOutcome::Failed;
        for (i, image) in images.iter().enumerate() {
            let (ox, oy) = self.paste_origin(image.width, image.height, i);
            let layer_name = if image.name.is_empty() {
                crate::names::numbered_pasted_layer(self.layers.len() + pasted + 1)
            } else {
                image.name.to_string()
            };
            let one = self.paste_image_as_layer_at_inner(
                layer_name,
                image.rgba,
                image.width,
                image.height,
                ox,
                oy,
                true,
            );
            if one != PasteOutcome::Failed {
                pasted += 1;
                outcome = merge_paste_outcome(outcome, one);
            }
        }
        if pasted == 0 {
            self.restore_stack(before);
            return (0, PasteOutcome::Failed);
        }
        let bytes = stack_snapshot_bytes(&before);
        self.history
            .push_stack(before, Some(self.active_layer), bytes);
        (pasted, outcome)
    }

    pub fn install_images_staggered(&mut self, images: &[PasteImage<'_>]) -> usize {
        let mut placed = 0usize;
        for (i, image) in images.iter().enumerate() {
            let d = (i as i32) * PASTE_STAGGER_PX;
            let ok = if i == 0 {
                self.place_image_at(image.rgba, image.width, image.height, d, d)
            } else {
                let layer_name = if image.name.is_empty() {
                    crate::names::numbered_layer(self.layers.len() + 1)
                } else {
                    image.name.to_string()
                };
                self.paste_image_as_layer_at_inner(
                    layer_name,
                    image.rgba,
                    image.width,
                    image.height,
                    d,
                    d,
                    true,
                ) != PasteOutcome::Failed
            };
            if ok {
                placed += 1;
            }
        }
        placed
    }

    fn paste_image_as_layer_at_inner(
        &mut self,
        name: impl Into<String>,
        rgba: &[u8],
        width: u32,
        height: u32,
        ox: i32,
        oy: i32,
        new_layer: bool,
    ) -> PasteOutcome {
        let expected = (width as usize) * (height as usize) * 4;
        if width == 0 || height == 0 || rgba.len() < expected {
            return PasteOutcome::Failed;
        }
        if new_layer {
            self.push_layer(name);
        }
        let placed = DocRect::new(ox, oy, ox + width as i32 - 1, oy + height as i32 - 1);
        let touched = match self.active_mut().and_then(|l| l.tiles_mut()) {
            Some(tiles) => {
                tiles.grow_extent(placed);
                tiles.blit_rgba_at(rgba, width, height, ox, oy)
            }
            None => 0,
        };
        if touched == 0 {
            if new_layer {
                self.layers.pop();
                if !self.layers.is_empty() {
                    self.active_layer = self.layers.len() - 1;
                }
            }
            return PasteOutcome::Failed;
        }
        let fits = width <= self.width && height <= self.height;
        if fits {
            PasteOutcome::Native
        } else {
            PasteOutcome::Overflowing
        }
    }

    fn paste_origin(&self, width: u32, height: u32, index: usize) -> (i32, i32) {
        let (base_x, base_y) = if width <= self.width && height <= self.height {
            self.selection_anchor()
        } else {
            self.centred(width, height)
        };
        let d = (index as i32) * PASTE_STAGGER_PX;
        (base_x + d, base_y + d)
    }

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

fn merge_paste_outcome(a: PasteOutcome, b: PasteOutcome) -> PasteOutcome {
    if a == PasteOutcome::Failed {
        return b;
    }
    if b == PasteOutcome::Failed {
        return a;
    }
    if a == PasteOutcome::Overflowing || b == PasteOutcome::Overflowing {
        PasteOutcome::Overflowing
    } else {
        PasteOutcome::Native
    }
}
