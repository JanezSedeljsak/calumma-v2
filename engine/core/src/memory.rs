use crate::document::Document;
use crate::layer::LayerContent;
use crate::vector::VectorItem;
use rustc_hash::FxHashSet;
use std::sync::Arc;

/// What the open document is holding, by category. Counting is exact rather than estimated:
/// tiles are copy-on-write, so the same allocation reached from two layers — or from the
/// history — is counted **once**, which is the only way the number matches what the process
/// actually owns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DocumentMemory {
    pub tile_bytes: usize,
    pub history_bytes: usize,
    pub mask_bytes: usize,
    pub vector_bytes: usize,
    pub text_bytes: usize,
    pub tile_count: usize,
    /// Tiles reachable from a layer whose storage another tile already paid for — Paper's
    /// shared white fill, and anything history still shares with the live document.
    pub shared_tile_count: usize,
}

impl DocumentMemory {
    pub fn total(&self) -> usize {
        self.tile_bytes + self.history_bytes + self.mask_bytes + self.vector_bytes + self.text_bytes
    }
}

/// Tracks which allocations have already been counted, by address. Two `Arc`s to one buffer
/// share an address, so this is what turns "bytes referenced" into "bytes owned".
#[derive(Default)]
struct Seen(FxHashSet<usize>);

impl Seen {
    fn count(&mut self, tile: &Arc<Vec<u8>>) -> Option<usize> {
        if self.0.insert(Arc::as_ptr(tile) as usize) {
            Some(tile.capacity())
        } else {
            None
        }
    }
}

pub fn document_memory(doc: &Document) -> DocumentMemory {
    let mut out = DocumentMemory::default();
    let mut seen = Seen::default();

    for layer in &doc.layers {
        if let Some(tiles) = layer.tiles() {
            for coord in tiles.coords() {
                let Some(tile) = tiles.get(coord) else {
                    continue;
                };
                out.tile_count += 1;
                match seen.count(tile) {
                    Some(bytes) => out.tile_bytes += bytes,
                    None => out.shared_tile_count += 1,
                }
            }
        }
        if let Some(mask) = layer.mask() {
            out.mask_bytes += mask.len();
        }
        match &layer.content {
            LayerContent::Vector(items) => out.vector_bytes += vector_bytes(items),
            LayerContent::Text { run, .. } => out.text_bytes += run.text.capacity(),
            LayerContent::Raster(_) => {}
        }
    }

    out.history_bytes = doc.history.held_bytes(|tile| seen.count(tile).unwrap_or(0));
    out
}

fn vector_bytes(items: &[VectorItem]) -> usize {
    items
        .iter()
        .map(|item| {
            std::mem::size_of::<VectorItem>()
                + match item {
                    VectorItem::Path(p) => p.points.capacity() * std::mem::size_of::<(f32, f32)>(),
                    VectorItem::Shape(_) => 0,
                }
        })
        .sum()
}
