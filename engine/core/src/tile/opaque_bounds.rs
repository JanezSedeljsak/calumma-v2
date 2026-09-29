//! `TileGrid::opaque_bounds` — the tight box around painted pixels — and the per-tile cache
//! that keeps it cheap to ask every frame.

use super::*;

/// Inclusive `(min_x, min_y, max_x, max_y)` of a tile's non-transparent pixels in tile-local
/// coordinates. Position-independent, which is what lets one scan serve every coordinate that
/// shares the buffer.
pub(super) type LocalRect = (i32, i32, i32, i32);

pub(super) fn tile_local_opaque_rect(tile: &[u8]) -> Option<LocalRect> {
    let mut acc: Option<LocalRect> = None;
    for ly in 0..TILE_SIZE as i32 {
        for lx in 0..TILE_SIZE as i32 {
            let i = pixel_index(lx as usize, ly as usize);
            if tile[i + 3] == 0 {
                continue;
            }
            acc = Some(match acc {
                None => (lx, ly, lx, ly),
                Some((x0, y0, x1, y1)) => (x0.min(lx), y0.min(ly), x1.max(lx), y1.max(ly)),
            });
        }
    }
    acc
}

pub(super) fn tile_opaque_rect(coord: TileCoord, tile: &[u8], extent: DocRect) -> Option<DocRect> {
    let (ox, oy) = coord.origin();
    let mut acc: Option<DocRect> = None;
    for ly in 0..TILE_SIZE as i32 {
        for lx in 0..TILE_SIZE as i32 {
            let i = pixel_index(lx as usize, ly as usize);
            if tile[i + 3] == 0 {
                continue;
            }
            let x = ox + lx;
            let y = oy + ly;
            if !extent.contains(x, y) {
                continue;
            }
            acc = Some(match acc {
                None => DocRect::new(x, y, x, y),
                Some(r) => DocRect::new(
                    r.min_x.min(x),
                    r.min_y.min(y),
                    r.max_x.max(x),
                    r.max_y.max(y),
                ),
            });
        }
    }
    acc
}

/// A layer's tight box, and the per-tile scans it is the union of.
///
/// `Layer::content_bounds` is what draws the transform frame, sets the transform pivot on both
/// the CPU and the GPU, and rejects layers during a pick — and it is read for every layer
/// whenever the draw list is rebuilt, which is every frame of a pan and every commit of a
/// stroke. A tight box means scanning pixels, so it has to be a cache; and invalidating the
/// *whole* cache on every edit would just move the cost, so the per-tile answers survive an
/// edit that did not touch their tile.
///
/// Locked rather than `Cell` because `&TileGrid` is handed to rayon workers
/// (`copy_layer_into_rgba` samples it from a parallel row loop), so this has to be `Sync`. The
/// lock is never reached on the warm path: `grid` answers first, and only a rescan opens it.
#[derive(Debug, Default)]
pub(super) struct BoundsCache {
    pub(super) grid: OnceLock<Option<DocRect>>,
    pub(super) tiles: Mutex<TileMap<Option<DocRect>>>,
}

impl BoundsCache {
    pub(super) fn invalidate(&mut self, coord: TileCoord) {
        self.grid.take();
        self.tiles.get_mut().remove(&coord);
    }

    pub(super) fn invalidate_all(&mut self) {
        self.grid.take();
        self.tiles.get_mut().clear();
    }
}

/// Cloning a grid — which history does on every step — carries the cache with it rather than
/// making the copy pay to rediscover a box the original already knew.
impl Clone for BoundsCache {
    fn clone(&self) -> Self {
        Self {
            grid: self.grid.clone(),
            tiles: Mutex::new(self.tiles.lock().clone()),
        }
    }
}

impl TileGrid {
    /// The tightest rectangle covering every non-transparent pixel, in document coordinates.
    ///
    /// A grid can hold the same pixel buffer at many coordinates — a filled paper layer is one
    /// `Arc` repeated across the whole canvas — so the scan is keyed by buffer identity rather
    /// than by coordinate. Tiles that lie wholly inside the document reuse one scan of their
    /// buffer, translated to each coordinate; only the tiles straddling the document edge, where
    /// clipping makes the answer depend on position, are scanned individually.
    pub fn opaque_bounds(&self) -> Option<DocRect> {
        if let Some(cached) = self.bounds.grid.get() {
            return *cached;
        }
        let answer = self.rescan_opaque_bounds();
        let _ = self.bounds.grid.set(answer);
        answer
    }

    /// Rebuilds the whole-grid box from the per-tile ones, scanning only the tiles whose cached
    /// contribution is missing — which after an ordinary edit is the handful a stroke touched,
    /// not the whole layer. On a 4096×3072 layer of distinct tiles that is the difference
    /// between ~3.5 ms and a few tens of microseconds, every time the draw list is rebuilt.
    ///
    /// The tiles that *are* missing still get the original two-way split: one scan per unique
    /// buffer for tiles lying wholly inside the document (a solid fill shares one `Arc` across
    /// every coordinate it covers, and a position-independent scan serves all of them), and an
    /// individual clipped scan for the tiles straddling the document edge, where the answer
    /// depends on position. Only a shrink can put pixels outside the document — `paint_rect`
    /// clips — but `Document::resize` keeps them on purpose, so the clip has to be exact
    /// rather than an intersection applied afterwards.
    pub(super) fn rescan_opaque_bounds(&self) -> Option<DocRect> {
        let extent = self.extent;
        if extent.is_empty() {
            return None;
        }
        let mut cache = self.bounds.tiles.lock();
        cache.retain(|coord, _| self.tiles.contains_key(coord));

        let mut inside: Vec<(TileCoord, &Arc<Vec<u8>>)> = Vec::new();
        let mut clipped: Vec<(TileCoord, &Arc<Vec<u8>>)> = Vec::new();
        let mut seen: FxHashSet<usize> = FxHashSet::default();
        let mut unique: Vec<&Arc<Vec<u8>>> = Vec::new();
        for (coord, pixels) in self.iter() {
            if cache.contains_key(&coord) {
                continue;
            }
            if !extent.contains_rect(Self::tile_rect(coord)) {
                clipped.push((coord, pixels));
                continue;
            }
            inside.push((coord, pixels));
            if seen.insert(Arc::as_ptr(pixels) as usize) {
                unique.push(pixels);
            }
        }

        let locals: FxHashMap<usize, Option<LocalRect>> = unique
            .into_par_iter()
            .map(|pixels| {
                (
                    Arc::as_ptr(pixels) as usize,
                    tile_local_opaque_rect(pixels.as_slice()),
                )
            })
            .collect();

        let placed: Vec<(TileCoord, Option<DocRect>)> = inside
            .par_iter()
            .map(|(coord, pixels)| {
                let local = locals
                    .get(&(Arc::as_ptr(*pixels) as usize))
                    .copied()
                    .flatten();
                let (ox, oy) = coord.origin();
                let rect = local.map(|(lx0, ly0, lx1, ly1)| {
                    DocRect::new(ox + lx0, oy + ly0, ox + lx1, oy + ly1)
                });
                (*coord, rect)
            })
            .collect();
        let edges: Vec<(TileCoord, Option<DocRect>)> = clipped
            .par_iter()
            .map(|(coord, pixels)| (*coord, tile_opaque_rect(*coord, pixels.as_slice(), extent)))
            .collect();
        cache.extend(placed);
        cache.extend(edges);

        cache.values().flatten().copied().reduce(union)
    }
}
