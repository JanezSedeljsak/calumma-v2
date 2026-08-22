//! One stroke's ink coverage, accumulated before any of it reaches the layer.
//!
//! A stroke used to stamp discs straight into the tiles, one `blend_over` per stamp. At full
//! opacity that is invisible — an opaque stamp over an opaque stamp is the same pixel — but
//! every stamp along a stroke overlaps its neighbours by half a radius, so at *any* opacity
//! below 1 the overlaps compounded and the stroke came out as a dark, beaded rope instead of
//! an even wash. This is the fix: coverage accumulates as a **maximum**, and the whole stroke
//! composites onto the layer exactly once.
//!
//! Storage is sparse and tile-shaped for the same reason the document is. A stroke covers a
//! ribbon, not its bounding box, so a diagonal flick across a large board allocates the tiles
//! along the ribbon rather than the rectangle enclosing it.

use crate::brush::{segment_distance, stroke_coverage, BrushProfile};
use crate::tile::{blend_over, DocRect, TileCoord, TileGrid, TileMap, TILE_SIZE};

const TILE_PIXELS: usize = (TILE_SIZE as usize) * (TILE_SIZE as usize);

pub struct CoverageGrid {
    tiles: TileMap<Vec<u8>>,
    bounds: DocRect,
}

impl CoverageGrid {
    pub fn new(bounds: DocRect) -> Self {
        Self {
            tiles: TileMap::default(),
            bounds,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    pub fn tile_coords(&self) -> impl Iterator<Item = TileCoord> + '_ {
        self.tiles.keys().copied()
    }

    /// Lay one segment of the stroke down. A single recorded point is a degenerate capsule —
    /// pass it as both ends and it comes out as the round dab a tap should make.
    pub fn add_segment(
        &mut self,
        a: (f32, f32),
        b: (f32, f32),
        radius: f32,
        profile: &BrushProfile,
    ) {
        if radius <= 0.0 {
            return;
        }
        let pad = radius + 1.0;
        let rect = DocRect::from_floats(
            a.0.min(b.0) - pad,
            a.1.min(b.1) - pad,
            a.0.max(b.0) + pad,
            a.1.max(b.1) + pad,
        );
        let Some(rect) = rect.intersect(self.bounds) else {
            return;
        };
        let ts = TILE_SIZE as i32;
        let (tx0, ty0, tx1, ty1) = rect.tile_span();
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                let coord = TileCoord { x: tx, y: ty };
                let (ox, oy) = coord.origin();
                let Some(span) = rect.intersect(DocRect::new(ox, oy, ox + ts - 1, oy + ts - 1))
                else {
                    continue;
                };
                let cell = self
                    .tiles
                    .entry(coord)
                    .or_insert_with(|| vec![0u8; TILE_PIXELS]);
                for y in span.min_y..=span.max_y {
                    let row = ((y - oy) as usize) * TILE_SIZE as usize;
                    for x in span.min_x..=span.max_x {
                        let px = x as f32 + 0.5;
                        let py = y as f32 + 0.5;
                        let distance = segment_distance((px, py), a, b);
                        let cov = stroke_coverage(profile, distance, radius, px, py);
                        if cov <= 0.0 {
                            continue;
                        }
                        let scaled = (cov * 255.0).round().clamp(0.0, 255.0) as u8;
                        let slot = &mut cell[row + (x - ox) as usize];
                        if scaled > *slot {
                            *slot = scaled;
                        }
                    }
                }
            }
        }
    }

    /// Composite the finished stroke onto a layer in one pass, tile by tile so the coverage
    /// lookup is a plain index rather than a hash probe per pixel. Returns tiles touched.
    ///
    /// `erase` swaps ink for its opposite: coverage takes alpha away instead of adding it, so
    /// the eraser gets the same even, non-compounding stroke the brushes do.
    pub fn paint_into(&self, grid: &mut TileGrid, ink: [u8; 4], erase: bool) -> usize {
        let mut touched = 0;
        for (coord, cell) in &self.tiles {
            let (ox, oy) = coord.origin();
            touched += grid.paint_rect(TileGrid::tile_rect(*coord), |x, y, dst| {
                let cov = cell[((y - oy) as usize) * TILE_SIZE as usize + (x - ox) as usize];
                if cov == 0 {
                    return None;
                }
                let alpha = (ink[3] as u32 * cov as u32 + 127) / 255;
                if alpha == 0 {
                    return None;
                }
                if !erase {
                    return Some(blend_over(dst, [ink[0], ink[1], ink[2], alpha as u8]));
                }
                let left = (dst[3] as u32 * (255 - alpha) + 127) / 255;
                if left == 0 {
                    return Some([0, 0, 0, 0]);
                }
                Some([dst[0], dst[1], dst[2], left as u8])
            });
        }
        touched
    }
}
