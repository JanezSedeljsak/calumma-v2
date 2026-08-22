use crate::limits::{ALPHA_MAX, ALPHA_ROUND_BIAS, EFFECT_CHUNK_BYTES, LAYER_PREVIEW_MAX_SIDE};
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

pub type TileSet = FxHashSet<TileCoord>;
pub type TileMap<V> = FxHashMap<TileCoord, V>;

/// The tile a resampling walk last looked up, and whether that coordinate held one at all — a
/// miss is worth remembering too, since the transparent parts of a crop come in runs like the
/// painted ones do.
type TileCursor<'a> = Option<(TileCoord, Option<&'a Arc<Vec<u8>>>)>;

pub const TILE_SIZE: u32 = 256;
pub const TILE_BYTES: usize = (TILE_SIZE as usize) * (TILE_SIZE as usize) * 4;
const CHANNELS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TileCoord {
    pub x: i32,
    pub y: i32,
}

impl TileCoord {
    #[inline]
    pub fn from_doc_i32(x: i32, y: i32) -> Self {
        Self {
            x: x.div_euclid(TILE_SIZE as i32),
            y: y.div_euclid(TILE_SIZE as i32),
        }
    }

    #[inline]
    pub fn from_doc(x: f32, y: f32) -> Self {
        Self::from_doc_i32(x.floor() as i32, y.floor() as i32)
    }

    #[inline]
    pub fn origin(&self) -> (i32, i32) {
        (self.x * TILE_SIZE as i32, self.y * TILE_SIZE as i32)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocRect {
    pub min_x: i32,
    pub min_y: i32,
    pub max_x: i32,
    pub max_y: i32,
}

impl DocRect {
    pub fn expanded_by_tiles(&self, margin_tiles: i32) -> DocRect {
        let m = margin_tiles * TILE_SIZE as i32;
        DocRect::new(
            self.min_x - m,
            self.min_y - m,
            self.max_x + m,
            self.max_y + m,
        )
    }

    pub fn intersects(&self, other: DocRect) -> bool {
        self.intersect(other).is_some()
    }

    pub fn new(min_x: i32, min_y: i32, max_x: i32, max_y: i32) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    pub fn from_size(width: u32, height: u32) -> Self {
        Self::new(0, 0, width as i32 - 1, height as i32 - 1)
    }

    pub fn from_floats(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Self {
        Self::new(
            min_x.floor() as i32,
            min_y.floor() as i32,
            max_x.ceil() as i32,
            max_y.ceil() as i32,
        )
    }

    pub fn is_empty(&self) -> bool {
        self.min_x > self.max_x || self.min_y > self.max_y
    }

    pub fn intersect(&self, other: DocRect) -> Option<DocRect> {
        let out = DocRect::new(
            self.min_x.max(other.min_x),
            self.min_y.max(other.min_y),
            self.max_x.min(other.max_x),
            self.max_y.min(other.max_y),
        );
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }

    pub fn tile_span(&self) -> (i32, i32, i32, i32) {
        let ts = TILE_SIZE as i32;
        (
            self.min_x.div_euclid(ts),
            self.min_y.div_euclid(ts),
            self.max_x.div_euclid(ts),
            self.max_y.div_euclid(ts),
        )
    }

    pub fn contains_rect(&self, other: DocRect) -> bool {
        self.min_x <= other.min_x
            && self.min_y <= other.min_y
            && self.max_x >= other.max_x
            && self.max_y >= other.max_y
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }
}

pub fn blend_over(dst: [u8; 4], src: [u8; 4]) -> [u8; 4] {
    let src_a = src[3] as u32;
    if src_a == 0 {
        return dst;
    }
    if src_a == ALPHA_MAX {
        return src;
    }
    let inv = ALPHA_MAX - src_a;
    let dst_a = dst[3] as u32;
    let out_scaled = src_a * ALPHA_MAX + dst_a * inv;
    if out_scaled == 0 {
        return [0; 4];
    }
    let bias = out_scaled / 2;
    let channel = |i: usize| {
        let numerator = src[i] as u32 * src_a * ALPHA_MAX + dst[i] as u32 * dst_a * inv;
        ((numerator + bias) / out_scaled) as u8
    };
    [
        channel(0),
        channel(1),
        channel(2),
        ((out_scaled + ALPHA_ROUND_BIAS) / ALPHA_MAX) as u8,
    ]
}

pub fn blend_with_mode(dst: [u8; 4], src: [u8; 4], mode: crate::layer::BlendMode) -> [u8; 4] {
    use crate::layer::BlendMode;
    let blended_rgb = match mode {
        BlendMode::Normal => [src[0], src[1], src[2]],
        BlendMode::Multiply => {
            std::array::from_fn(|i| ((dst[i] as u32 * src[i] as u32) / ALPHA_MAX) as u8)
        }
        BlendMode::Screen => std::array::from_fn(|i| {
            (ALPHA_MAX - ((ALPHA_MAX - dst[i] as u32) * (ALPHA_MAX - src[i] as u32) / ALPHA_MAX))
                as u8
        }),
    };
    blend_over(
        dst,
        [blended_rgb[0], blended_rgb[1], blended_rgb[2], src[3]],
    )
}

pub fn unpremultiply_rgba(rgba: &mut [u8]) {
    rgba.par_chunks_mut(EFFECT_CHUNK_BYTES).for_each(|block| {
        for px in block.chunks_exact_mut(CHANNELS) {
            let alpha = px[3] as u32;
            if alpha == ALPHA_MAX {
                continue;
            }
            if alpha == 0 {
                px[0] = 0;
                px[1] = 0;
                px[2] = 0;
                continue;
            }
            for channel in px[..3].iter_mut() {
                let scaled = (*channel as u32 * ALPHA_MAX + alpha / 2) / alpha;
                *channel = scaled.min(ALPHA_MAX) as u8;
            }
        }
    });
}

fn uniform_tile(rgba: [u8; 4]) -> Vec<u8> {
    rgba.repeat(TILE_SIZE as usize * TILE_SIZE as usize)
}

/// The one colour a tile is painted in, or `None` the moment a second one turns up. A mixed
/// tile — every tile with actual drawing in it — bails within the first few pixels, so asking
/// this of every tile on load costs nothing worth measuring, while the tiles it *does* answer
/// for are exactly the ones worth sharing.
pub fn uniform_color(pixels: &[u8]) -> Option<[u8; 4]> {
    let first: [u8; 4] = pixels.get(..CHANNELS)?.try_into().ok()?;
    pixels
        .chunks_exact(CHANNELS)
        .all(|px| px == first)
        .then_some(first)
}

#[inline]
fn pixel_index(local_x: usize, local_y: usize) -> usize {
    (local_y * TILE_SIZE as usize + local_x) * CHANNELS
}

/// Inclusive `(min_x, min_y, max_x, max_y)` of a tile's non-transparent pixels in tile-local
/// coordinates. Position-independent, which is what lets one scan serve every coordinate that
/// shares the buffer.
type LocalRect = (i32, i32, i32, i32);

fn tile_local_opaque_rect(tile: &[u8]) -> Option<LocalRect> {
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

fn tile_opaque_rect(coord: TileCoord, tile: &[u8], width: i32, height: i32) -> Option<DocRect> {
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
            if x < 0 || y < 0 || x >= width || y >= height {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DirtyChannel {
    Render,
    Store,
    Preview,
}

impl DirtyChannel {
    pub const COUNT: usize = 3;
    pub const ALL: [DirtyChannel; Self::COUNT] = [Self::Render, Self::Store, Self::Preview];

    #[inline]
    fn slot(self) -> usize {
        match self {
            Self::Render => 0,
            Self::Store => 1,
            Self::Preview => 2,
        }
    }
}

/// A layer's cached picture of itself, cropped to its painted pixels and capped at
/// [`LAYER_PREVIEW_MAX_SIDE`] on the long side. Held behind an `Arc` so cloning a grid — which
/// history does — copies a pointer rather than up to a megabyte of pixels.
#[derive(Clone, Debug)]
pub struct Preview {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Preview {
    /// Point-samples down to `max_side` on the long side, or hands back a copy unchanged when
    /// the caller wants at least what is cached. The preview is already the crop, so every size
    /// derived from it frames the layer identically.
    pub fn scaled(&self, max_side: u32) -> (u32, u32, Vec<u8>) {
        let max_side = max_side.max(1);
        if self.width.max(self.height) <= max_side {
            return (self.width, self.height, self.rgba.clone());
        }
        let scale = (max_side as f32 / self.width as f32).min(max_side as f32 / self.height as f32);
        let tw = ((self.width as f32) * scale).round().max(1.0) as u32;
        let th = ((self.height as f32) * scale).round().max(1.0) as u32;
        let mut out = vec![0u8; (tw as usize) * (th as usize) * CHANNELS];
        for ty in 0..th {
            let sy = nearest_source(ty, th, self.height);
            for tx in 0..tw {
                let sx = nearest_source(tx, tw, self.width);
                let src = ((sy as usize) * (self.width as usize) + (sx as usize)) * CHANNELS;
                let dst = ((ty as usize) * (tw as usize) + (tx as usize)) * CHANNELS;
                out[dst..dst + CHANNELS].copy_from_slice(&self.rgba[src..src + CHANNELS]);
            }
        }
        (tw, th, out)
    }

    pub fn bytes(&self) -> usize {
        self.rgba.capacity()
    }
}

/// Maps output index `i` of `out_len` onto the source index it samples, spreading the samples
/// across the full source extent so the first and last output pixels land on the source's first
/// and last.
#[inline]
fn nearest_source(i: u32, out_len: u32, src_len: u32) -> u32 {
    if out_len <= 1 {
        return 0;
    }
    let t = (i as f32) * ((src_len - 1) as f32) / ((out_len - 1) as f32);
    (t.round() as u32).min(src_len.saturating_sub(1))
}

#[derive(Clone, Debug)]
pub struct TileGrid {
    tiles: TileMap<Arc<Vec<u8>>>,
    dirty: [TileSet; DirtyChannel::COUNT],
    preview: Option<Arc<Preview>>,
    content_revision: u64,
    pub width: u32,
    pub height: u32,
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
            content_revision: 0,
            width,
            height,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn bounds(&self) -> DocRect {
        DocRect::from_size(self.width, self.height)
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
        for set in &mut self.dirty {
            set.insert(coord);
        }
    }

    pub fn mark_all_dirty(&mut self) {
        self.content_revision = self.content_revision.wrapping_add(1);
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
        let (ox, oy) = coord.origin();
        ox < self.width as i32
            && oy < self.height as i32
            && ox + TILE_SIZE as i32 > 0
            && oy + TILE_SIZE as i32 > 0
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
    /// loader uses this to give every solid-colour tile in a project the *same* allocation,
    /// which is what keeps a reopened document as cheap as a freshly created one.
    pub fn insert_shared(&mut self, coord: TileCoord, pixels: Arc<Vec<u8>>) -> bool {
        if !self.tile_in_bounds(coord) || pixels.len() != TILE_BYTES {
            return false;
        }
        self.tiles.insert(coord, pixels);
        self.mark_dirty(coord);
        true
    }

    pub fn snapshot_tiles(&self, coords: &[TileCoord]) -> TileMap<Option<Arc<Vec<u8>>>> {
        let mut out = TileMap::default();
        out.reserve(coords.len());
        for c in coords {
            out.insert(*c, self.tiles.get(c).cloned());
        }
        out
    }

    pub fn restore_tiles(&mut self, snapshot: &TileMap<Option<Arc<Vec<u8>>>>) {
        for (coord, maybe) in snapshot {
            match maybe {
                Some(pixels) => {
                    self.tiles.insert(*coord, Arc::clone(pixels));
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
        x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32
    }

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

    /// Fill a region with one colour, sharing a **single** allocation across every tile the
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

    pub fn blend_pixel(&mut self, x: i32, y: i32, rgba: [u8; 4]) {
        if rgba[3] == 0 {
            return;
        }
        self.paint_rect(DocRect::new(x, y, x, y), |_, _, dst| {
            Some(blend_over(dst, rgba))
        });
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

    /// The tightest rectangle covering every non-transparent pixel, in document coordinates.
    ///
    /// A grid can hold the same pixel buffer at many coordinates — a filled paper layer is one
    /// `Arc` repeated across the whole canvas — so the scan is keyed by buffer identity rather
    /// than by coordinate. Tiles that lie wholly inside the document reuse one scan of their
    /// buffer, translated to each coordinate; only the tiles straddling the document edge, where
    /// clipping makes the answer depend on position, are scanned individually.
    pub fn opaque_bounds(&self) -> Option<DocRect> {
        let width = self.width as i32;
        let height = self.height as i32;
        if width <= 0 || height <= 0 {
            return None;
        }
        let ts = TILE_SIZE as i32;

        let mut inside: Vec<(TileCoord, &Arc<Vec<u8>>)> = Vec::new();
        let mut clipped: Vec<(TileCoord, &Arc<Vec<u8>>)> = Vec::new();
        let mut seen: FxHashSet<usize> = FxHashSet::default();
        let mut unique: Vec<&Arc<Vec<u8>>> = Vec::new();
        for (coord, pixels) in self.iter() {
            let (ox, oy) = coord.origin();
            if ox < 0 || oy < 0 || ox + ts > width || oy + ts > height {
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

        let placed = inside.par_iter().filter_map(|(coord, pixels)| {
            let (lx0, ly0, lx1, ly1) = (*locals.get(&(Arc::as_ptr(*pixels) as usize))?)?;
            let (ox, oy) = coord.origin();
            Some(DocRect::new(ox + lx0, oy + ly0, ox + lx1, oy + ly1))
        });
        let edges = clipped.par_iter().filter_map(|(coord, pixels)| {
            tile_opaque_rect(*coord, pixels.as_slice(), width, height)
        });

        placed.chain(edges).reduce_with(|a, b| {
            DocRect::new(
                a.min_x.min(b.min_x),
                a.min_y.min(b.min_y),
                a.max_x.max(b.max_x),
                a.max_y.max(b.max_y),
            )
        })
    }

    /// The cached [`Preview`] of this grid, rebuilt only when a tile has changed since the last
    /// time it was asked for. Every thumbnail the shell wants is a resample of this, so a layer
    /// is scanned at full resolution once per edit instead of once per request.
    pub fn preview(&mut self) -> Arc<Preview> {
        if self.preview.is_none() || !self.dirty[DirtyChannel::Preview.slot()].is_empty() {
            let (width, height, rgba) = self.thumbnail(LAYER_PREVIEW_MAX_SIDE);
            self.preview = Some(Arc::new(Preview {
                width,
                height,
                rgba,
            }));
            self.clear_dirty(DirtyChannel::Preview);
        }
        Arc::clone(self.preview.as_ref().expect("just rebuilt when missing"))
    }

    pub fn preview_bytes(&self) -> usize {
        self.preview.as_ref().map_or(0, |p| p.bytes())
    }

    /// Point-samples the grid down to `max_side`, cropped to its painted pixels. Prefer
    /// [`TileGrid::preview`] for anything the UI shows repeatedly — this walks the layer at full
    /// resolution every call.
    pub fn thumbnail(&self, max_side: u32) -> (u32, u32, Vec<u8>) {
        let max_side = max_side.max(1);
        let dw = self.width.max(1);
        let dh = self.height.max(1);
        let crop = self.opaque_bounds().unwrap_or(DocRect::from_size(dw, dh));
        let crop_w = (crop.max_x - crop.min_x + 1).max(1) as u32;
        let crop_h = (crop.max_y - crop.min_y + 1).max(1) as u32;
        let scale = (max_side as f32 / crop_w as f32)
            .min(max_side as f32 / crop_h as f32)
            .min(1.0);
        let tw = ((crop_w as f32) * scale).round().max(1.0) as u32;
        let th = ((crop_h as f32) * scale).round().max(1.0) as u32;
        let mut rgba = vec![0u8; (tw as usize) * (th as usize) * CHANNELS];
        let mut cursor = TileCursor::default();
        for ty in 0..th {
            let sy = crop.min_y + nearest_source(ty, th, crop_h) as i32;
            for tx in 0..tw {
                let sx = crop.min_x + nearest_source(tx, tw, crop_w) as i32;
                let px = self.sample_pixel(sx, sy, &mut cursor);
                let i = ((ty as usize) * (tw as usize) + (tx as usize)) * CHANNELS;
                rgba[i..i + CHANNELS].copy_from_slice(&px);
            }
        }
        (tw, th, rgba)
    }

    /// `get_pixel` with the last tile carried forward. Resampling walks the crop row-major, so
    /// run after run of samples land in the tile the one before them did — hashing the map for
    /// each of them is what made a 512² preview a quarter of a million probes.
    fn sample_pixel<'a>(&'a self, x: i32, y: i32, cursor: &mut TileCursor<'a>) -> [u8; 4] {
        if !self.contains_doc_point(x, y) {
            return [0; 4];
        }
        let coord = TileCoord::from_doc_i32(x, y);
        let tile = match cursor {
            Some((at, tile)) if *at == coord => *tile,
            _ => {
                let found = self.tiles.get(&coord);
                *cursor = Some((coord, found));
                found
            }
        };
        let Some(tile) = tile else {
            return [0; 4];
        };
        let (ox, oy) = coord.origin();
        let i = pixel_index((x - ox) as usize, (y - oy) as usize);
        let mut out = [0u8; 4];
        out.copy_from_slice(&tile[i..i + CHANNELS]);
        out
    }

    pub fn stamp_disc(&mut self, cx: f32, cy: f32, radius: f32, rgba: [u8; 4]) -> usize {
        if radius <= 0.0 || rgba[3] == 0 {
            return 0;
        }
        let pad = radius + crate::limits::STAMP_COVERAGE_PADDING;
        let rect = DocRect::from_floats(cx - pad, cy - pad, cx + pad, cy + pad);
        let r2 = radius * radius;
        self.paint_rect(rect, |px, py, dst| {
            let dx = px as f32 + 0.5 - cx;
            let dy = py as f32 + 0.5 - cy;
            if dx * dx + dy * dy <= r2 {
                Some(blend_over(dst, rgba))
            } else {
                None
            }
        })
    }

    pub fn stamp_disc_erase(&mut self, cx: f32, cy: f32, radius: f32) -> usize {
        if radius <= 0.0 {
            return 0;
        }
        let pad = radius + crate::limits::STAMP_COVERAGE_PADDING;
        let rect = DocRect::from_floats(cx - pad, cy - pad, cx + pad, cy + pad);
        let r2 = radius * radius;
        self.paint_rect(rect, |px, py, _| {
            let dx = px as f32 + 0.5 - cx;
            let dy = py as f32 + 0.5 - cy;
            if dx * dx + dy * dy <= r2 {
                Some([0, 0, 0, 0])
            } else {
                None
            }
        })
    }

    pub fn blit_rgba(&mut self, rgba: &[u8], width: u32, height: u32) -> usize {
        self.blit_rgba_at(rgba, width, height, 0, 0)
    }

    pub fn blit_rgba_at(
        &mut self,
        rgba: &[u8],
        width: u32,
        height: u32,
        offset_x: i32,
        offset_y: i32,
    ) -> usize {
        if width == 0 || height == 0 {
            return 0;
        }
        let rect = DocRect::new(
            offset_x,
            offset_y,
            offset_x + width as i32 - 1,
            offset_y + height as i32 - 1,
        );
        self.paint_rect(rect, |x, y, _| {
            let sx = x - offset_x;
            let sy = y - offset_y;
            let i = ((sy as usize) * (width as usize) + sx as usize) * CHANNELS;
            let px = rgba.get(i..i + CHANNELS)?;
            if px[3] == 0 {
                return None;
            }
            Some([px[0], px[1], px[2], px[3]])
        })
    }

    pub fn copy_into_rgba(&self, rgba: &mut [u8], width: u32, height: u32) {
        let doc = DocRect::from_size(width, height);
        for (coord, pixels) in self.iter() {
            let (ox, oy) = coord.origin();
            let ts = TILE_SIZE as i32;
            let Some(span) = doc.intersect(DocRect::new(ox, oy, ox + ts - 1, oy + ts - 1)) else {
                continue;
            };
            for y in span.min_y..=span.max_y {
                let src_row = pixel_index(0, (y - oy) as usize);
                let src_start = src_row + ((span.min_x - ox) as usize) * CHANNELS;
                let run = ((span.max_x - span.min_x + 1) as usize) * CHANNELS;
                let dst_start = ((y as usize) * (width as usize) + span.min_x as usize) * CHANNELS;
                rgba[dst_start..dst_start + run]
                    .copy_from_slice(&pixels[src_start..src_start + run]);
            }
        }
    }

    /// Copy an arbitrary document-space rectangle out into a tightly packed RGBA buffer,
    /// `rect` wide and transparent wherever the grid has no tile. Row-at-a-time per tile, the
    /// same way `copy_into_rgba` walks the whole grid — a read-modify-write brush needs its
    /// neighbourhood in one flat buffer, and doing that with `get_pixel` would be a hash
    /// lookup per pixel.
    ///
    /// Pixels of `rect` that fall outside the document are left transparent rather than
    /// clamped; the caller decides what a document edge means to it.
    pub fn copy_rect_rgba(&self, rect: DocRect) -> Vec<u8> {
        let width = (rect.max_x - rect.min_x + 1).max(0) as usize;
        let height = (rect.max_y - rect.min_y + 1).max(0) as usize;
        let mut out = vec![0u8; width * height * CHANNELS];
        if width == 0 || height == 0 {
            return out;
        }
        let Some(span) = rect.intersect(self.bounds()) else {
            return out;
        };
        let ts = TILE_SIZE as i32;
        let (tx0, ty0, tx1, ty1) = span.tile_span();
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                let coord = TileCoord { x: tx, y: ty };
                let Some(pixels) = self.tiles.get(&coord) else {
                    continue;
                };
                let (ox, oy) = coord.origin();
                let Some(cell) = span.intersect(DocRect::new(ox, oy, ox + ts - 1, oy + ts - 1))
                else {
                    continue;
                };
                let run = ((cell.max_x - cell.min_x + 1) as usize) * CHANNELS;
                for y in cell.min_y..=cell.max_y {
                    let src = pixel_index((cell.min_x - ox) as usize, (y - oy) as usize);
                    let dst = (((y - rect.min_y) as usize) * width
                        + (cell.min_x - rect.min_x) as usize)
                        * CHANNELS;
                    out[dst..dst + run].copy_from_slice(&pixels[src..src + run]);
                }
            }
        }
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
