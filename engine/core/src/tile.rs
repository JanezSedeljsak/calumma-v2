use crate::limits::{ALPHA_MAX, ALPHA_ROUND_BIAS};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

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
    for px in rgba.chunks_exact_mut(CHANNELS) {
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
}

#[inline]
fn pixel_index(local_x: usize, local_y: usize) -> usize {
    (local_y * TILE_SIZE as usize + local_x) * CHANNELS
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DirtyChannel {
    Render,
    Store,
}

impl DirtyChannel {
    pub const COUNT: usize = 2;
    pub const ALL: [DirtyChannel; Self::COUNT] = [Self::Render, Self::Store];

    #[inline]
    fn slot(self) -> usize {
        match self {
            Self::Render => 0,
            Self::Store => 1,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TileGrid {
    tiles: HashMap<TileCoord, Arc<Vec<u8>>>,
    dirty: [HashSet<TileCoord>; DirtyChannel::COUNT],
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
            tiles: HashMap::new(),
            dirty: std::array::from_fn(|_| HashSet::new()),
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

    pub fn get(&self, coord: TileCoord) -> Option<&Arc<Vec<u8>>> {
        self.tiles.get(&coord)
    }

    pub fn dirty_tiles(&self, channel: DirtyChannel) -> &HashSet<TileCoord> {
        &self.dirty[channel.slot()]
    }

    pub fn mark_dirty(&mut self, coord: TileCoord) {
        for set in &mut self.dirty {
            set.insert(coord);
        }
    }

    pub fn mark_all_dirty(&mut self) {
        let coords: Vec<TileCoord> = self.tiles.keys().copied().collect();
        for set in &mut self.dirty {
            set.extend(coords.iter().copied());
        }
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

    pub fn snapshot_tiles(&self, coords: &[TileCoord]) -> HashMap<TileCoord, Option<Arc<Vec<u8>>>> {
        let mut out = HashMap::with_capacity(coords.len());
        for c in coords {
            out.insert(*c, self.tiles.get(c).cloned());
        }
        out
    }

    pub fn restore_tiles(&mut self, snapshot: &HashMap<TileCoord, Option<Arc<Vec<u8>>>>) {
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

    pub fn thumbnail(&self, max_side: u32) -> (u32, u32, Vec<u8>) {
        let max_side = max_side.max(1);
        let dw = self.width.max(1);
        let dh = self.height.max(1);
        let scale = (max_side as f32 / dw as f32)
            .min(max_side as f32 / dh as f32)
            .min(1.0);
        let tw = ((dw as f32) * scale).round().max(1.0) as u32;
        let th = ((dh as f32) * scale).round().max(1.0) as u32;
        let mut rgba = vec![0u8; (tw as usize) * (th as usize) * CHANNELS];
        for ty in 0..th {
            for tx in 0..tw {
                let sx = if tw <= 1 {
                    0
                } else {
                    ((tx as f32) * ((dw - 1) as f32) / ((tw - 1) as f32)).round() as i32
                };
                let sy = if th <= 1 {
                    0
                } else {
                    ((ty as f32) * ((dh - 1) as f32) / ((th - 1) as f32)).round() as i32
                };
                let px = self.get_pixel(sx, sy);
                let i = ((ty as usize) * (tw as usize) + (tx as usize)) * CHANNELS;
                rgba[i..i + CHANNELS].copy_from_slice(&px);
            }
        }
        (tw, th, rgba)
    }

    pub fn stamp_disc(&mut self, cx: f32, cy: f32, radius: f32, rgba: [u8; 4]) -> usize {
        if radius <= 0.0 || rgba[3] == 0 {
            return 0;
        }
        let pad = radius + crate::limits::STAMP_COVERAGE_PADDING;
        let rect = DocRect::from_floats(cx - pad, cy - pad, cx + pad, cy + pad);
        let r2 = radius * radius;
        self.paint_rect(rect, |px, py, _| {
            let dx = px as f32 + 0.5 - cx;
            let dy = py as f32 + 0.5 - cy;
            if dx * dx + dy * dy <= r2 {
                Some(rgba)
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

    pub fn pixels_ref(&self, coord: TileCoord) -> Option<&[u8]> {
        self.tiles.get(&coord).map(|t| t.as_slice())
    }
}
