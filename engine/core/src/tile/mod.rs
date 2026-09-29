use crate::blur::{premultiply, unpremultiply};
use crate::history_tile::HistoryTile;
use crate::limits::{ALPHA_MAX, ALPHA_ROUND_BIAS, EFFECT_CHUNK_BYTES, LAYER_PREVIEW_MAX_SIDE};
use parking_lot::Mutex;
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::{Arc, OnceLock};

mod geometry;
mod opaque_bounds;
mod paint;
mod pixel;
mod preview;
mod rgba;

use geometry::union;
pub use geometry::{DocRect, TileCoord};
use opaque_bounds::BoundsCache;
use pixel::pixel_index;
pub(crate) use pixel::uniform_tile;
pub use pixel::{blend_over, blend_with_mode, uniform_color, unpremultiply_rgba};
pub use preview::Preview;

pub type TileSet = FxHashSet<TileCoord>;
pub type TileMap<V> = FxHashMap<TileCoord, V>;

pub const TILE_SIZE: u32 = 256;
pub const TILE_BYTES: usize = (TILE_SIZE as usize) * (TILE_SIZE as usize) * 4;
const CHANNELS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DirtyChannel {
    Render,
    Store,
    Preview,
    Overview,
}

impl DirtyChannel {
    pub const COUNT: usize = 4;
    pub const ALL: [DirtyChannel; Self::COUNT] =
        [Self::Render, Self::Store, Self::Preview, Self::Overview];

    #[inline]
    fn slot(self) -> usize {
        match self {
            Self::Render => 0,
            Self::Store => 1,
            Self::Preview => 2,
            Self::Overview => 3,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TileGrid {
    tiles: TileMap<Arc<Vec<u8>>>,
    dirty: [TileSet; DirtyChannel::COUNT],
    preview: Option<Arc<Preview>>,
    bounds: BoundsCache,
    content_revision: u64,
    width: u32,
    height: u32,
    /// What this grid is **allowed to hold**, which is the document rectangle until something
    /// widens it.
    ///
    /// It exists so a pasted image can keep the pixels that fall outside the paper instead of
    /// having them clipped away by `paint_rect` and lost. A layer that overflows still *draws*
    /// only inside the paper — `Camera::paper_scissor` sees to that, and `visible_doc_rect` is
    /// clamped to the board so the off-paper tiles are never even uploaded — but they are there
    /// to be dragged back into view.
    ///
    /// Deliberately not the same thing as `width`/`height`: those stay the **document** size,
    /// which is what masks are sized to, what export walks, and what a paste is measured
    /// against. Only the storage grew.
    extent: DocRect,
}

impl PartialEq for TileGrid {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width && self.height == other.height && self.tiles == other.tiles
    }
}

impl TileGrid {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            tiles: TileMap::default(),
            dirty: std::array::from_fn(|_| TileSet::default()),
            preview: None,
            bounds: BoundsCache::default(),
            content_revision: 0,
            width,
            height,
            extent: DocRect::from_size(width, height),
        }
    }

    /// The document rectangle — the canvas, not the storage.
    pub fn doc_bounds(&self) -> DocRect {
        DocRect::from_size(self.width, self.height)
    }

    /// Widens the storage so `rect` can be written. Only ever grows: a grid that has been
    /// given room for an overflowing paste keeps it, because the pixels out there are the
    /// whole point and nothing else knows to put them back.
    /// What this grid may hold, which is the document until something deliberately opens it
    /// wider — a paste bigger than the paper, or a stroke on a layer that has been moved off it.
    pub fn extent(&self) -> DocRect {
        self.extent
    }

    pub fn grow_extent(&mut self, rect: DocRect) {
        let next = union(self.extent, rect);
        if next == self.extent {
            return;
        }
        self.extent = next;
        self.bounds.invalidate_all();
    }

    /// Room for one tile, for the loader: tiles come back one row at a time and an off-paper
    /// one has to be admitted before `insert_shared` asks whether it is in bounds.
    pub fn grow_extent_to_tile(&mut self, coord: TileCoord) {
        self.grow_extent(Self::tile_rect(coord));
    }

    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The one way a grid's dimensions move. `opaque_bounds` clips its scan to the extent, so a
    /// document that shrinks changes the answer without a single pixel being touched — which is
    /// exactly the case a dirty-tile invalidation would miss.
    ///
    /// The extent only ever grows to meet the new document. A shrink keeps whatever room the
    /// grid already had, which is what makes "shrinking never discards off-canvas tile data"
    /// true for overflowing layers as well as for the tiles that merely straddle the edge.
    pub fn set_size(&mut self, width: u32, height: u32) {
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        self.extent = union(self.extent, DocRect::from_size(width, height));
        self.bounds.invalidate_all();
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    /// What the grid can hold — the document, widened by any overflow it was given.
    pub fn bounds(&self) -> DocRect {
        self.extent
    }

    pub fn coords(&self) -> impl Iterator<Item = TileCoord> + '_ {
        self.tiles.keys().copied()
    }

    pub fn coords_intersecting(&self, rect: DocRect) -> impl Iterator<Item = TileCoord> + '_ {
        let (tx0, ty0, tx1, ty1) = rect.tile_span();
        (ty0..=ty1).flat_map(move |ty| {
            (tx0..=tx1).filter_map(move |tx| {
                let coord = TileCoord { x: tx, y: ty };
                self.tiles.contains_key(&coord).then_some(coord)
            })
        })
    }

    pub fn get(&self, coord: TileCoord) -> Option<&Arc<Vec<u8>>> {
        self.tiles.get(&coord)
    }

    pub fn dirty_tiles(&self, channel: DirtyChannel) -> &TileSet {
        &self.dirty[channel.slot()]
    }

    /// Bumped by every write to these pixels, and by nothing else. This is what lets a caller
    /// tell whether a layer's picture has actually changed — a layer being shown or hidden, its
    /// opacity or blend mode moved, or a *different* layer being edited all leave it alone.
    pub fn content_revision(&self) -> u64 {
        self.content_revision
    }

    pub fn mark_dirty(&mut self, coord: TileCoord) {
        self.content_revision = self.content_revision.wrapping_add(1);
        self.bounds.invalidate(coord);
        for set in &mut self.dirty {
            set.insert(coord);
        }
    }

    pub fn mark_all_dirty(&mut self) {
        self.content_revision = self.content_revision.wrapping_add(1);
        self.bounds.invalidate_all();
        let coords: Vec<TileCoord> = self.tiles.keys().copied().collect();
        for set in &mut self.dirty {
            set.extend(coords.iter().copied());
        }
    }

    pub fn mark_channel_dirty(&mut self, channel: DirtyChannel) {
        let coords: Vec<TileCoord> = self.tiles.keys().copied().collect();
        self.dirty[channel.slot()].extend(coords);
    }

    pub fn clear_dirty(&mut self, channel: DirtyChannel) {
        self.dirty[channel.slot()].clear();
    }

    pub fn clear_dirty_tile(&mut self, channel: DirtyChannel, coord: TileCoord) {
        self.dirty[channel.slot()].remove(&coord);
    }

    pub fn tile_rect(coord: TileCoord) -> DocRect {
        let (ox, oy) = coord.origin();
        let ts = TILE_SIZE as i32;
        DocRect::new(ox, oy, ox + ts - 1, oy + ts - 1)
    }

    pub fn tile_in_bounds(&self, coord: TileCoord) -> bool {
        Self::tile_rect(coord).intersects(self.extent)
    }

    pub fn ensure_mut(&mut self, coord: TileCoord) -> Option<&mut Vec<u8>> {
        if !self.tile_in_bounds(coord) {
            return None;
        }
        self.mark_dirty(coord);
        let entry = self
            .tiles
            .entry(coord)
            .or_insert_with(|| Arc::new(vec![0u8; TILE_BYTES]));
        Some(Arc::make_mut(entry))
    }

    /// Adopt a buffer that already exists instead of copying pixels into a fresh one. The
    /// loader uses this to give every solid-color tile in a project the *same* allocation,
    /// which is what keeps a reopened document as cheap as a freshly created one.
    pub fn insert_shared(&mut self, coord: TileCoord, pixels: Arc<Vec<u8>>) -> bool {
        if !self.tile_in_bounds(coord) || pixels.len() != TILE_BYTES {
            return false;
        }
        self.tiles.insert(coord, pixels);
        self.mark_dirty(coord);
        true
    }

    /// A snapshot stays exactly as cheap as it has always been: every tile is stored as a
    /// cloned `Arc` handle, so a fresh diff costs **zero** real bytes until the live tile is
    /// painted again. Nothing is inspected or compressed here — that is `History`'s cold
    /// sweep, deliberately off the paint-commit path this sits on.
    pub fn snapshot_tiles(&self, coords: &[TileCoord]) -> TileMap<Option<HistoryTile>> {
        let mut out = TileMap::default();
        out.reserve(coords.len());
        for c in coords {
            out.insert(*c, self.tiles.get(c).cloned().map(HistoryTile::from_pixels));
        }
        out
    }

    pub fn restore_tiles(&mut self, snapshot: &TileMap<Option<HistoryTile>>) {
        let mut uniform = Vec::new();
        for (coord, maybe) in snapshot {
            match maybe {
                Some(tile) => {
                    let pixels = tile.materialize(&mut uniform);
                    self.tiles.insert(*coord, pixels);
                }
                None => {
                    self.tiles.remove(coord);
                }
            }
            self.mark_dirty(*coord);
        }
    }

    #[inline]
    pub fn contains_doc_point(&self, x: i32, y: i32) -> bool {
        self.extent.contains(x, y)
    }

    pub fn get_pixel(&self, x: i32, y: i32) -> [u8; 4] {
        if !self.contains_doc_point(x, y) {
            return [0; 4];
        }
        let coord = TileCoord::from_doc_i32(x, y);
        let Some(tile) = self.tiles.get(&coord) else {
            return [0; 4];
        };
        let (ox, oy) = coord.origin();
        let i = pixel_index((x - ox) as usize, (y - oy) as usize);
        let mut out = [0u8; 4];
        out.copy_from_slice(&tile[i..i + CHANNELS]);
        out
    }

    pub fn clear(&mut self) {
        self.mark_all_dirty();
        self.tiles.clear();
    }

    pub fn memory_bytes(&self) -> usize {
        self.tiles.len() * TILE_BYTES
    }

    pub fn iter(&self) -> impl Iterator<Item = (TileCoord, &Arc<Vec<u8>>)> {
        self.tiles.iter().map(|(c, p)| (*c, p))
    }

    pub fn whole_tiles_share_one_arc(&self) -> bool {
        let bounds = self.bounds();
        let mut shared: Option<*const Vec<u8>> = None;
        for (coord, pixels) in self.iter() {
            let cell = Self::tile_rect(coord);
            if !bounds.contains_rect(cell) {
                continue;
            }
            let ptr = Arc::as_ptr(pixels);
            match shared {
                None => shared = Some(ptr),
                Some(p) if p == ptr => {}
                _ => return false,
            }
        }
        shared.is_some()
    }

    pub fn pixels_ref(&self, coord: TileCoord) -> Option<&[u8]> {
        self.tiles.get(&coord).map(|t| t.as_slice())
    }
}
