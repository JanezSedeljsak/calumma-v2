//! A selection that is a bitmap rather than a formula.
//!
//! Rect, Ellipse and Lasso all answer `contains` from their own geometry — three floats and a
//! predicate. The magic wand cannot: what it selects is whatever the flood fill reached, which
//! has no closed form. So this is the first selection shape that stores its answer.
//!
//! **One bit per pixel, not one byte.** A full-canvas wand on an 8192×8192 document is 8 MiB
//! at a bit each and 64 MiB at a byte each, and the selection is live for as long as the user
//! leaves it up. `contains` pays a shift and a mask for that, which is nothing next to the
//! `Shape::coverage` call the analytic shapes make.

use crate::tile::DocRect;
use rayon::prelude::*;

/// One edge of the selection boundary, in document coordinates: `[x0, y0, x1, y1]`.
pub type OutlineEdge = [f32; 4];

#[derive(Clone, Debug, PartialEq)]
pub struct SelectionMask {
    origin: (i32, i32),
    width: u32,
    height: u32,
    stride: usize,
    bits: Vec<u8>,
    /// The boundary, traced once when the mask is finished rather than per frame. The outline
    /// is what the marching ants are drawn from, and a wand selection can cover a whole
    /// document — re-walking every pixel of it each frame to find the edge is the one thing
    /// that would make this shape more expensive to *display* than to compute.
    outline: Vec<OutlineEdge>,
}

impl SelectionMask {
    pub fn new(origin: (i32, i32), width: u32, height: u32) -> Self {
        let stride = width.div_ceil(8) as usize;
        Self {
            origin,
            width,
            height,
            stride,
            bits: vec![0u8; stride * height as usize],
            outline: Vec::new(),
        }
    }

    /// A mask built from a predicate, one row per rayon task. Invert asks the predicate
    /// `width × height` times — 67 million times on an 8K document — so unlike the wand's
    /// flood, which only ever visits what it reaches, this one is worth spreading across
    /// cores. Rows are independent because each owns its own `stride` bytes.
    pub fn from_predicate<F>(origin: (i32, i32), width: u32, height: u32, inside: F) -> Self
    where
        F: Fn(i32, i32) -> bool + Sync,
    {
        let mut mask = Self::new(origin, width, height);
        let stride = mask.stride;
        let w = width as i32;
        mask.bits
            .par_chunks_mut(stride)
            .enumerate()
            .for_each(|(row, bytes)| {
                let y = origin.1 + row as i32;
                for lx in 0..w {
                    if inside(origin.0 + lx, y) {
                        bytes[lx as usize / 8] |= 1u8 << (lx % 8);
                    }
                }
            });
        mask
    }

    fn index(&self, x: i32, y: i32) -> Option<(usize, u8)> {
        let lx = x.checked_sub(self.origin.0)?;
        let ly = y.checked_sub(self.origin.1)?;
        if lx < 0 || ly < 0 || lx as u32 >= self.width || ly as u32 >= self.height {
            return None;
        }
        let lx = lx as usize;
        Some((ly as usize * self.stride + lx / 8, 1u8 << (lx % 8)))
    }

    pub fn set(&mut self, x: i32, y: i32) {
        if let Some((byte, bit)) = self.index(x, y) {
            self.bits[byte] |= bit;
        }
    }

    pub fn get(&self, x: i32, y: i32) -> bool {
        match self.index(x, y) {
            Some((byte, bit)) => self.bits[byte] & bit != 0,
            None => false,
        }
    }

    /// The rectangle the bitmap covers — the scope the flood was allowed to reach, not the
    /// tight extent of what it did reach. `finish` crops it to the latter.
    pub fn bounds(&self) -> DocRect {
        DocRect::new(
            self.origin.0,
            self.origin.1,
            self.origin.0 + self.width as i32 - 1,
            self.origin.1 + self.height as i32 - 1,
        )
    }

    pub fn memory_bytes(&self) -> usize {
        self.bits.capacity() + self.outline.capacity() * std::mem::size_of::<OutlineEdge>()
    }

    pub fn outline(&self) -> &[OutlineEdge] {
        &self.outline
    }

    /// Crop to what was actually reached and trace the boundary. Returns `None` for a mask
    /// with nothing set, so an empty wand click leaves no selection rather than an invisible
    /// one that clips every subsequent paint stroke to nothing.
    ///
    /// Cropping matters beyond memory: `bounds()` is what copy, cut and delete iterate over,
    /// and an uncropped mask would make every one of them walk the whole flood scope.
    pub fn finish(self) -> Option<Self> {
        let (mut min_x, mut min_y) = (i32::MAX, i32::MAX);
        let (mut max_x, mut max_y) = (i32::MIN, i32::MIN);
        for y in 0..self.height as i32 {
            for x in 0..self.width as i32 {
                let (dx, dy) = (self.origin.0 + x, self.origin.1 + y);
                if self.get(dx, dy) {
                    min_x = min_x.min(dx);
                    min_y = min_y.min(dy);
                    max_x = max_x.max(dx);
                    max_y = max_y.max(dy);
                }
            }
        }
        if min_x > max_x {
            return None;
        }
        let mut cropped = Self::new(
            (min_x, min_y),
            (max_x - min_x + 1) as u32,
            (max_y - min_y + 1) as u32,
        );
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                if self.get(x, y) {
                    cropped.set(x, y);
                }
            }
        }
        cropped.outline = cropped.trace_outline();
        Some(cropped)
    }

    /// Every pixel edge with a selected pixel on one side and an unselected one on the other,
    /// merged into maximal runs.
    ///
    /// Merging is not a micro-optimisation: each edge becomes one GPU stroke instance, and an
    /// unmerged trace of a 4000-pixel-wide region emits 4000 one-pixel segments for a single
    /// straight boundary. A wand selection is usually mostly straight boundaries, so the runs
    /// collapse it by orders of magnitude. Holes come out for free — a run is emitted wherever
    /// the neighbour is unselected, whichever side of the region that neighbour is on.
    ///
    /// Two walks: horizontal runs take the top edge of every pixel whose upper neighbour is
    /// out and the bottom edge of every pixel whose lower neighbour is out; vertical runs are
    /// the same walk transposed.
    fn trace_outline(&self) -> Vec<OutlineEdge> {
        let mut out = Vec::new();
        let (ox, oy) = self.origin;
        let (w, h) = (self.width as i32, self.height as i32);

        for y in 0..h {
            for (dy, edge_y) in [(-1, 0.0), (1, 1.0)] {
                let mut run: Option<i32> = None;
                for x in 0..=w {
                    let doc_x = ox + x;
                    let doc_y = oy + y;
                    let boundary = x < w && self.get(doc_x, doc_y) && !self.get(doc_x, doc_y + dy);
                    match (boundary, run) {
                        (true, None) => run = Some(doc_x),
                        (false, Some(start)) => {
                            let ey = (doc_y as f32) + edge_y;
                            out.push([start as f32, ey, doc_x as f32, ey]);
                            run = None;
                        }
                        _ => {}
                    }
                }
            }
        }

        for x in 0..w {
            for (dx, edge_x) in [(-1, 0.0), (1, 1.0)] {
                let mut run: Option<i32> = None;
                for y in 0..=h {
                    let doc_x = ox + x;
                    let doc_y = oy + y;
                    let boundary = y < h && self.get(doc_x, doc_y) && !self.get(doc_x + dx, doc_y);
                    match (boundary, run) {
                        (true, None) => run = Some(doc_y),
                        (false, Some(start)) => {
                            let ex = (doc_x as f32) + edge_x;
                            out.push([ex, start as f32, ex, doc_y as f32]);
                            run = None;
                        }
                        _ => {}
                    }
                }
            }
        }
        out
    }
}
