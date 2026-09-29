//! Writing pixels into a grid: the rect painter every commit goes through, uniform fills, and
//! single pixels.

use super::*;

impl TileGrid {
    pub fn paint_rect<F>(&mut self, rect: DocRect, mut paint: F) -> usize
    where
        F: FnMut(i32, i32, [u8; 4]) -> Option<[u8; 4]>,
    {
        let Some(rect) = rect.intersect(self.bounds()) else {
            return 0;
        };
        let ts = TILE_SIZE as i32;
        let (tx0, ty0, tx1, ty1) = rect.tile_span();
        let mut pending: Vec<(usize, [u8; 4])> = Vec::new();
        let mut tiles_touched = 0;

        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                let coord = TileCoord { x: tx, y: ty };
                if !self.tile_in_bounds(coord) {
                    continue;
                }
                let (ox, oy) = coord.origin();
                let Some(span) = rect.intersect(DocRect::new(ox, oy, ox + ts - 1, oy + ts - 1))
                else {
                    continue;
                };

                if self.tiles.contains_key(&coord) {
                    let Some(slot) = self.tiles.get_mut(&coord) else {
                        continue;
                    };
                    let tile = Arc::make_mut(slot);
                    let mut touched = false;
                    for y in span.min_y..=span.max_y {
                        for x in span.min_x..=span.max_x {
                            let i = pixel_index((x - ox) as usize, (y - oy) as usize);
                            let current = [tile[i], tile[i + 1], tile[i + 2], tile[i + 3]];
                            if let Some(next) = paint(x, y, current) {
                                if next != current {
                                    tile[i..i + CHANNELS].copy_from_slice(&next);
                                    touched = true;
                                }
                            }
                        }
                    }
                    if touched {
                        self.mark_dirty(coord);
                        tiles_touched += 1;
                    }
                    continue;
                }

                pending.clear();
                for y in span.min_y..=span.max_y {
                    for x in span.min_x..=span.max_x {
                        if let Some(next) = paint(x, y, [0; 4]) {
                            if next != [0; 4] {
                                pending.push((
                                    pixel_index((x - ox) as usize, (y - oy) as usize),
                                    next,
                                ));
                            }
                        }
                    }
                }
                if pending.is_empty() {
                    continue;
                }
                let slot = self
                    .tiles
                    .entry(coord)
                    .or_insert_with(|| Arc::new(vec![0u8; TILE_BYTES]));
                let tile = Arc::make_mut(slot);
                for (i, px) in &pending {
                    tile[*i..*i + CHANNELS].copy_from_slice(px);
                }
                self.mark_dirty(coord);
                tiles_touched += 1;
            }
        }
        tiles_touched
    }

    /// Fill a region with one color, sharing a **single** allocation across every tile the
    /// region covers whole. Tiles are copy-on-write `Arc`s, so the first stroke on any of them
    /// forks its own copy and nothing downstream can tell the difference — the sharing shows
    /// up only in memory, where a 4096×4096 white Paper layer costs one 256 KB tile instead of
    /// 256 separate ones. Partially covered tiles (the document's ragged right and bottom
    /// edges) keep their own storage, since their remainder has to stay transparent.
    pub fn fill_uniform(&mut self, rect: DocRect, rgba: [u8; 4]) -> usize {
        let Some(rect) = rect.intersect(self.bounds()) else {
            return 0;
        };
        let mut shared: Option<Arc<Vec<u8>>> = None;
        let mut partial: Vec<DocRect> = Vec::new();
        let (tx0, ty0, tx1, ty1) = rect.tile_span();
        let mut tiles_touched = 0;

        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                let coord = TileCoord { x: tx, y: ty };
                if !self.tile_in_bounds(coord) {
                    continue;
                }
                let cell = Self::tile_rect(coord);
                if rect.contains_rect(cell) {
                    let tile = shared.get_or_insert_with(|| Arc::new(uniform_tile(rgba)));
                    self.tiles.insert(coord, Arc::clone(tile));
                    self.mark_dirty(coord);
                    tiles_touched += 1;
                } else if let Some(span) = rect.intersect(cell) {
                    partial.push(span);
                }
            }
        }
        for span in partial {
            tiles_touched += self.paint_rect(span, |_, _, _| Some(rgba));
        }
        tiles_touched
    }

    pub fn set_pixel(&mut self, x: i32, y: i32, rgba: [u8; 4]) {
        self.paint_rect(DocRect::new(x, y, x, y), |_, _, _| Some(rgba));
    }
}
