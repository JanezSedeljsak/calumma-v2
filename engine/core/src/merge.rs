//! The two ways a layer stops being its own layer: **Merge Down** and **Clip to Layer Below**.
//!
//! They differ by exactly one step. A clip multiplies the source's alpha by the base's before
//! compositing, so whatever the source painted outside the base's ink disappears; a merge does
//! not. Every other part — the guards, the text commit, the transform bake, the mask, the
//! opacity and LUT bake, the blend, the stack fixup — is the same, which is why they are one
//! function with a flag rather than two that would have to be kept in step by hand.
//!
//! **A clip is destructive on purpose.** Photoshop's clipping mask is a live compositing rule:
//! the clipped layer reads the base's pixels every frame, so painting on the base re-uploads
//! everything above it. Calumma has no layer whose rendering depends on another layer's
//! contents (`AGENTS.md` → STRICT SCOPE), and this is what that invariant leaves — the clip is
//! applied once and the result is one ordinary layer. The renderer never learns the word, the
//! CPU and GPU have no rule to agree on, and PSD / SVG / PDF export a flat layer because it
//! *is* a flat layer. The cost is that re-editing means undo.

use crate::document::{apply_layer_effects, apply_mask, copy_layer_into_rgba, Document};
use crate::layer::Layer;
use crate::limits::{ALPHA_MAX, ALPHA_ROUND_BIAS};
use crate::tile::{blend_with_mode, DocRect, TileGrid};
use rayon::prelude::*;

impl Document {
    /// Composites layer `index` onto the one below it — honouring its mask, opacity,
    /// adjustments and blend mode — and removes it.
    pub fn merge_layer_down(&mut self, index: usize) -> bool {
        self.flatten_layer_down(index, false)
    }

    /// Merge Down, with the source's alpha first multiplied by the base's, so the result is
    /// the source seen only through the base's ink — Photoshop's clipping mask, baked at the
    /// moment it is asked for.
    ///
    /// The base's *raw* tile alpha is what clips, not its effective alpha: the base keeps its
    /// own opacity, mask and adjustments as layer properties after the merge, and those then
    /// govern the merged result. That is the clipping-group semantics people expect, and
    /// multiplying by the effective alpha here would apply them a second time.
    pub fn clip_layer_down(&mut self, index: usize) -> bool {
        if !self.can_clip_layer_down(index) {
            return false;
        }
        self.flatten_layer_down(index, true)
    }

    /// What Merge Down asks, plus the one thing a clip cannot live with: a base carrying a
    /// transform. The source is baked into document space while the base's tiles sit in its own
    /// untransformed space, so the alpha the clip reads would be offset from the ink it is
    /// supposed to be clipping to by exactly that transform. Merge Down has the same mismatch
    /// and gets away with it because nothing there lines two layers up pixel for pixel; here it
    /// is the whole point, so the action stands down and says to reset the transform first.
    pub fn can_clip_layer_down(&self, index: usize) -> bool {
        if index == 0 || index >= self.layers.len() {
            return false;
        }
        let base = &self.layers[index - 1];
        if base.is_paper() || base.tiles().is_none() {
            return false;
        }
        if base.transform.is_some_and(|t| !t.is_identity()) {
            return false;
        }
        let source = &self.layers[index];
        source.tiles().is_some() || source.content.item().is_some()
    }

    fn flatten_layer_down(&mut self, index: usize, clip: bool) -> bool {
        if index == 0 || index >= self.layers.len() {
            return false;
        }
        if self.layers[index - 1].is_paper() {
            return false;
        }
        self.commit_text();
        self.rasterize_text_layer(index - 1);
        if self.layers[index].tiles().is_none() && self.layers[index].content.item().is_none() {
            return false;
        }
        if self.layers[index - 1].tiles().is_none() {
            return false;
        }
        self.clear_vector_selection();
        self.record_stack_history();
        let mode = self.layers[index].blend_mode;
        let w = self.width.max(1);
        let h = self.height.max(1);
        let mut src_buf = vec![0u8; (w as usize) * (h as usize) * 4];
        let src_buf = &mut src_buf;
        copy_layer_into_rgba(&self.layers[index], src_buf, w, h);
        apply_mask(src_buf, self.layers[index].mask());
        let lut = self.layers[index].adjustments.map(|a| a.lut());
        apply_layer_effects(src_buf, &self.layers[index], lut.as_ref());
        if clip {
            clip_to_base_alpha(src_buf, &self.layers[index - 1], w);
        }

        let src_buf = &*src_buf;
        let Some(dst) = self.layers[index - 1].tiles_mut() else {
            return false;
        };
        dst.paint_rect(DocRect::from_size(w, h), |x, y, dst_px| {
            let i = ((y as usize) * (w as usize) + (x as usize)) * 4;
            let src_px = [src_buf[i], src_buf[i + 1], src_buf[i + 2], src_buf[i + 3]];
            if src_px[3] == 0 {
                return None;
            }
            Some(blend_with_mode(dst_px, src_px, mode))
        });

        self.layers.remove(index);
        if self.active_layer >= self.layers.len() {
            self.active_layer = self.layers.len().saturating_sub(1);
        } else if self.active_layer > index {
            self.active_layer -= 1;
        } else {
            self.active_layer = index - 1;
        }
        true
    }
}

/// Rounds `+ ALPHA_ROUND_BIAS` rather than truncating so that clipping against a fully opaque
/// base is an exact no-op — truncation loses a level on every pixel, and "clip to a solid
/// rectangle" would come back one alpha step darker than the merge it should equal.
fn clip_to_base_alpha(src: &mut [u8], base: &Layer, w: u32) {
    let Some(tiles) = base.tiles() else {
        return;
    };
    let row_bytes = (w as usize) * 4;
    src.par_chunks_mut(row_bytes)
        .enumerate()
        .for_each(|(y, row)| clip_row_to_base_alpha(row, tiles, y as i32, w));
}

fn clip_row_to_base_alpha(row: &mut [u8], tiles: &TileGrid, y: i32, w: u32) {
    for x in 0..w as usize {
        let px = &mut row[x * 4..x * 4 + 4];
        if px[3] == 0 {
            continue;
        }
        let base_alpha = tiles.get_pixel(x as i32, y)[3] as u32;
        px[3] = ((px[3] as u32 * base_alpha + ALPHA_ROUND_BIAS) / ALPHA_MAX) as u8;
    }
}
