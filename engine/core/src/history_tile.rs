use crate::limits::HISTORY_COMPRESSION_LEVEL;
use crate::tile::{uniform_color, uniform_tile, TILE_BYTES};
use std::sync::Arc;

/// One tile as the undo stack holds it.
///
/// The variant is a storage fact, not a lifecycle stage: whether the live document still
/// shares a `Pixels` tile is answered by `Arc::strong_count` at sweep time, never by a flag
/// that could go stale the moment the document is painted on again.
#[derive(Clone, Debug)]
pub enum HistoryTile {
    /// The tile's real bytes — very often the *same allocation* the live document is still
    /// using, because `TileGrid::snapshot_tiles` clones the handle rather than the pixels.
    /// That is why a freshly pushed diff costs nothing until the live tile is repainted.
    Pixels(Arc<Vec<u8>>),
    /// A tile that is entirely one color, kept as that color. 4 bytes instead of 262,144 —
    /// and a drawing app's history is mostly these, since an untouched region of a layer is
    /// transparent and a filled one is flat.
    Uniform([u8; 4]),
    /// A zstd frame over the tile's bytes, produced only once history is the tile's sole
    /// owner (see `compact`).
    Compressed(Box<[u8]>),
}

impl HistoryTile {
    pub fn from_pixels(pixels: Arc<Vec<u8>>) -> Self {
        Self::Pixels(pixels)
    }

    /// The tile's bytes, ready to hand back to a `TileGrid`. `uniform` is a per-restore cache
    /// keyed by color so that undoing a flat fill rebuilds **one** allocation shared by every
    /// tile it covered, exactly as `TileGrid::fill_uniform` laid it down — without it, undo
    /// would quietly turn Paper's single shared tile into one allocation per tile.
    pub fn materialize(&self, uniform: &mut Vec<([u8; 4], Arc<Vec<u8>>)>) -> Arc<Vec<u8>> {
        match self {
            Self::Pixels(pixels) => Arc::clone(pixels),
            Self::Uniform(rgba) => {
                if let Some((_, cached)) = uniform.iter().find(|(c, _)| c == rgba) {
                    return Arc::clone(cached);
                }
                let made = Arc::new(uniform_tile(*rgba));
                uniform.push((*rgba, Arc::clone(&made)));
                made
            }
            Self::Compressed(frame) => {
                Arc::new(zstd::decode_all(frame.as_ref()).unwrap_or_else(|_| vec![0u8; TILE_BYTES]))
            }
        }
    }

    /// What `History`'s budget charges for this tile. A `Pixels` tile pays the full tile
    /// whether or not it is currently shared, because sharing ends the moment the live tile
    /// is painted again and the budget has to have already assumed it would.
    pub fn budget_bytes(&self) -> usize {
        match self {
            Self::Pixels(_) => TILE_BYTES,
            Self::Uniform(_) => std::mem::size_of::<[u8; 4]>(),
            Self::Compressed(frame) => frame.len(),
        }
    }

    /// The bytes this tile really owns. `Pixels` defers to the caller's counter so an
    /// allocation the live document also holds is charged to the document once rather than
    /// to both — the same address-keyed dedup `memory::document_memory` does everywhere.
    pub fn held_bytes(&self, mut pixels_bytes: impl FnMut(&Arc<Vec<u8>>) -> usize) -> usize {
        match self {
            Self::Pixels(pixels) => pixels_bytes(pixels),
            Self::Uniform(_) => std::mem::size_of::<[u8; 4]>(),
            Self::Compressed(frame) => frame.len(),
        }
    }

    /// Whether this tile still costs a full tile and could be made smaller.
    pub fn is_compactable(&self) -> bool {
        matches!(self, Self::Pixels(_))
    }

    /// Shrink a cold tile, returning the budget bytes reclaimed.
    ///
    /// **The gate is unique ownership, not age.** A `Pixels` tile the live document still
    /// shares costs history nothing today; compressing it would force the very copy the
    /// sharing was avoiding, making memory strictly worse. So a tile is a candidate only when
    /// `Arc::strong_count` says history is its sole owner. The uniform check runs first
    /// because it is both the bigger win and the cheaper test — a tile with real drawing in
    /// it bails within the first few pixels.
    pub fn compact(&mut self) -> usize {
        let Self::Pixels(pixels) = self else {
            return 0;
        };
        if Arc::strong_count(pixels) != 1 {
            return 0;
        }
        let before = TILE_BYTES;
        if let Some(rgba) = uniform_color(pixels) {
            *self = Self::Uniform(rgba);
            return before - self.budget_bytes();
        }
        let Ok(frame) = zstd::encode_all(pixels.as_slice(), HISTORY_COMPRESSION_LEVEL) else {
            return 0;
        };
        if frame.len() >= before {
            return 0;
        }
        *self = Self::Compressed(frame.into_boxed_slice());
        before - self.budget_bytes()
    }
}
