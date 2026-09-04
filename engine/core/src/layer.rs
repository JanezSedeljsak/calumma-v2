use crate::filters::Adjustments;
use crate::history::TileSnapshot;
use crate::limits::PAPER_WHITE;
use crate::tile::{DirtyChannel, DocRect, TileGrid, TileSet};
use crate::transform::{bounds_center, LayerTransform};
use crate::vector::VectorItem;
use calumma_text::TextRun;
use parking_lot::Mutex;
use rayon::prelude::*;
use std::sync::Arc;
use uuid::Uuid;

use num_enum::{IntoPrimitive, TryFromPrimitive};

type MaskedBoundsCache = Arc<Mutex<Option<(DocRect, Option<DocRect>)>>>;
type VectorBoundsCache = Arc<Mutex<Option<(VectorBoundsKey, Option<(f32, f32, f32, f32)>)>>>;

/// A cheap stand-in for "have this path's points changed" that costs nothing to compute,
/// unlike the O(n) scan `VectorItem::geometry_bounds` runs for a `VectorPath`. Sound only
/// because `AGENTS.md`'s "Basic vector editing only" holds: `set_translated`/`set_scaled` are
/// the sole ways an existing item's points change, and both rebuild every point from an affine
/// map applied uniformly — so first and last necessarily move whenever any of them do. A future
/// per-node edit would break that assumption and this key with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VectorBoundsKey {
    len: usize,
    first: (u32, u32),
    last: (u32, u32),
}

impl VectorBoundsKey {
    fn of(points: &[(f32, f32)]) -> Option<Self> {
        let (fx, fy) = *points.first()?;
        let (lx, ly) = *points.last()?;
        Some(Self {
            len: points.len(),
            first: (fx.to_bits(), fy.to_bits()),
            last: (lx.to_bits(), ly.to_bits()),
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u32)]
pub enum BlendMode {
    #[default]
    Normal = 0,
    Multiply = 1,
    Screen = 2,
}

impl BlendMode {
    pub fn from_u32(value: u32) -> Option<Self> {
        Self::try_from(value).ok()
    }

    pub fn as_u32(self) -> u32 {
        self.into()
    }
}

/// `Text` keeps its pixels in a `TileGrid` like any painted layer, but that grid is a
/// *cache* of `run` rather than the content itself — `text_layer::resync` rebuilds it
/// whenever the run changes. That is what lets a text layer stay editable forever while
/// compositing, masks, blend modes, export and the GPU upload path keep reading plain tiles
/// and needing to know nothing about glyphs.
#[derive(Clone, Debug, PartialEq)]
pub enum LayerContent {
    Raster(TileGrid),
    Vector(VectorItem),
    Text { run: Box<TextRun>, tiles: TileGrid },
}

impl LayerContent {
    pub fn raster(width: u32, height: u32) -> Self {
        Self::Raster(TileGrid::new(width, height))
    }

    pub fn text(run: TextRun, width: u32, height: u32) -> Self {
        Self::Text {
            run: Box::new(run),
            tiles: TileGrid::new(width, height),
        }
    }

    pub fn is_raster(&self) -> bool {
        matches!(self, Self::Raster(_))
    }

    pub fn is_vector(&self) -> bool {
        matches!(self, Self::Vector(_))
    }

    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text { .. })
    }

    pub fn tiles(&self) -> Option<&TileGrid> {
        match self {
            Self::Raster(tiles) | Self::Text { tiles, .. } => Some(tiles),
            Self::Vector(_) => None,
        }
    }

    pub fn tiles_mut(&mut self) -> Option<&mut TileGrid> {
        match self {
            Self::Raster(tiles) | Self::Text { tiles, .. } => Some(tiles),
            Self::Vector(_) => None,
        }
    }

    pub fn item(&self) -> Option<&VectorItem> {
        match self {
            Self::Vector(item) => Some(item),
            Self::Raster(_) | Self::Text { .. } => None,
        }
    }

    pub fn item_mut(&mut self) -> Option<&mut VectorItem> {
        match self {
            Self::Vector(item) => Some(item),
            Self::Raster(_) | Self::Text { .. } => None,
        }
    }

    pub fn run(&self) -> Option<&TextRun> {
        match self {
            Self::Text { run, .. } => Some(run),
            Self::Raster(_) | Self::Vector(_) => None,
        }
    }

    pub fn run_mut(&mut self) -> Option<&mut TextRun> {
        match self {
            Self::Text { run, .. } => Some(run),
            Self::Raster(_) | Self::Vector(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Layer {
    pub id: String,
    pub name: String,
    pub visible: bool,
    pub content: LayerContent,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub adjustments: Option<Adjustments>,
    pub transform: Option<LayerTransform>,
    /// Refuses everything that would change this layer's pixels or where they sit — paint,
    /// fill, clear, transform, move, and being picked by a click on the board. Visibility,
    /// duplicate and export stay available, and so does delete: a lock guards against the
    /// stray stroke, not against a deliberate press of the button next to it.
    pub locked: bool,
    mask: Option<Vec<u8>>,
    masked_bounds: MaskedBoundsCache,
    vector_bounds: VectorBoundsCache,
}

fn fresh_masked_bounds_cache() -> MaskedBoundsCache {
    Arc::new(Mutex::new(None))
}

fn fresh_vector_bounds_cache() -> VectorBoundsCache {
    Arc::new(Mutex::new(None))
}

pub(crate) fn scan_masked_bounds(
    tiles: &TileGrid,
    mask: &[u8],
    transform: Option<LayerTransform>,
) -> Option<DocRect> {
    let crop = tiles.opaque_bounds()?;
    let doc_w = tiles.width();
    let doc_h = tiles.height();
    let pivot = bounds_center((
        crop.min_x as f32,
        crop.min_y as f32,
        crop.max_x as f32 + 1.0,
        crop.max_y as f32 + 1.0,
    ));
    let t = transform.unwrap_or_default();
    let has_transform = transform.is_some_and(|t| !t.is_identity());
    let rows: Vec<_> = (crop.min_y..=crop.max_y)
        .into_par_iter()
        .filter_map(|y| {
            let mut min_x = i32::MAX;
            let mut max_x = i32::MIN;
            let mut any = false;
            for x in crop.min_x..=crop.max_x {
                let px = tiles.get_pixel(x, y);
                if px[3] == 0 {
                    continue;
                }
                let (doc_x, doc_y) = if has_transform {
                    t.forward(pivot, (x as f32, y as f32))
                } else {
                    (x as f32, y as f32)
                };
                let ix = doc_x.floor() as i32;
                let iy = doc_y.floor() as i32;
                if ix < 0 || iy < 0 || (ix as u32) >= doc_w || (iy as u32) >= doc_h {
                    continue;
                }
                let index = (iy as u32 * doc_w + ix as u32) as usize;
                let m = mask.get(index).copied().unwrap_or(255);
                if m == 0 {
                    continue;
                }
                if ((px[3] as u32 * m as u32) / 255) == 0 {
                    continue;
                }
                any = true;
                min_x = min_x.min(x);
                max_x = max_x.max(x);
            }
            any.then_some((y, min_x, max_x))
        })
        .collect();
    if rows.is_empty() {
        return None;
    }
    let min_y = rows.iter().map(|(y, _, _)| *y).min()?;
    let max_y = rows.iter().map(|(y, _, _)| *y).max()?;
    let min_x = rows.iter().map(|(_, x0, _)| *x0).min()?;
    let max_x = rows.iter().map(|(_, _, x1)| *x1).max()?;
    Some(DocRect::new(min_x, min_y, max_x, max_y))
}

impl Layer {
    pub fn new(name: impl Into<String>, width: u32, height: u32) -> Self {
        Self::with_id(Uuid::new_v4().to_string(), name, width, height)
    }

    pub fn with_id(id: String, name: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            id,
            name: name.into(),
            visible: true,
            content: LayerContent::raster(width, height),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            adjustments: None,
            transform: None,
            locked: false,
            mask: None,
            masked_bounds: fresh_masked_bounds_cache(),
            vector_bounds: fresh_vector_bounds_cache(),
        }
    }

    pub fn vector(name: impl Into<String>, item: VectorItem) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            visible: true,
            content: LayerContent::Vector(item),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            adjustments: None,
            transform: None,
            locked: false,
            mask: None,
            masked_bounds: fresh_masked_bounds_cache(),
            vector_bounds: fresh_vector_bounds_cache(),
        }
    }

    pub fn paper(width: u32, height: u32) -> Self {
        let mut layer = Self::new(crate::names::PAPER, width, height);
        let bounds = DocRect::from_size(width.max(1), height.max(1));
        if let Some(tiles) = layer.tiles_mut() {
            tiles.fill_uniform(bounds, PAPER_WHITE);
        }
        layer
    }

    pub fn text(name: impl Into<String>, run: TextRun, width: u32, height: u32) -> Self {
        let mut layer = Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            visible: true,
            content: LayerContent::text(run, width, height),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            adjustments: None,
            transform: None,
            locked: false,
            mask: None,
            masked_bounds: fresh_masked_bounds_cache(),
            vector_bounds: fresh_vector_bounds_cache(),
        };
        crate::text_layer::resync(&mut layer);
        layer
    }

    pub fn is_paper(&self) -> bool {
        self.name == crate::names::PAPER
    }

    pub fn is_text(&self) -> bool {
        self.content.is_text()
    }

    pub fn run(&self) -> Option<&TextRun> {
        self.content.run()
    }

    /// Replaces a text layer's run and repaints its tile cache from it, so the pixels never
    /// disagree with the string they came from.
    pub fn set_run(&mut self, run: TextRun) -> bool {
        let Some(slot) = self.content.run_mut() else {
            return false;
        };
        *slot = run;
        crate::text_layer::resync(self)
    }

    pub fn tiles(&self) -> Option<&TileGrid> {
        self.content.tiles()
    }

    pub fn tiles_mut(&mut self) -> Option<&mut TileGrid> {
        self.content.tiles_mut()
    }

    pub fn mask(&self) -> Option<&[u8]> {
        self.mask.as_deref()
    }

    pub fn mask_owned(&self) -> Option<Vec<u8>> {
        self.mask.clone()
    }

    pub fn set_mask(&mut self, mask: Option<Vec<u8>>) {
        self.mask = mask;
        *self.masked_bounds.lock() = None;
        self.mark_all_dirty();
    }

    pub fn resize_mask(
        &mut self,
        old_width: u32,
        old_height: u32,
        new_width: u32,
        new_height: u32,
    ) {
        let Some(old) = &self.mask else {
            return;
        };
        let mut next = vec![255u8; (new_width as usize) * (new_height as usize)];
        let copy_w = old_width.min(new_width) as usize;
        let copy_h = old_height.min(new_height) as usize;
        for y in 0..copy_h {
            let src = y * old_width as usize;
            let dst = y * new_width as usize;
            next[dst..dst + copy_w].copy_from_slice(&old[src..src + copy_w]);
        }
        self.set_mask(Some(next));
    }

    pub fn dirty_tiles(&self, channel: DirtyChannel) -> Option<&TileSet> {
        self.tiles().map(|t| t.dirty_tiles(channel))
    }

    pub fn mark_all_dirty(&mut self) {
        *self.masked_bounds.lock() = None;
        if let Some(tiles) = self.tiles_mut() {
            tiles.mark_all_dirty();
        }
    }

    pub fn mark_channel_dirty(&mut self, channel: DirtyChannel) {
        if let Some(tiles) = self.tiles_mut() {
            tiles.mark_channel_dirty(channel);
        }
    }

    pub fn clear_dirty(&mut self, channel: DirtyChannel) {
        if let Some(tiles) = self.tiles_mut() {
            tiles.clear_dirty(channel);
        }
    }

    pub fn clear(&mut self) -> TileSnapshot {
        let Some(tiles) = self.tiles_mut() else {
            return TileSnapshot::default();
        };
        let coords: Vec<_> = tiles.coords().collect();
        let snap = tiles.snapshot_tiles(&coords);
        tiles.clear();
        snap
    }

    /// A document-space point in this layer's own grid — the inverse of what the renderer does
    /// when it draws the layer through its transform. Anything about to be *written* into the
    /// grid at a place the user pointed at has to come through here first, or it lands in the
    /// grid at the document coordinate and the transform then carries it somewhere else.
    pub fn doc_point_to_grid(&self, p: (f32, f32)) -> (f32, f32) {
        let Some(t) = self.transform.filter(|t| !t.is_identity()) else {
            return p;
        };
        let Some(raw) = self.content_bounds() else {
            return p;
        };
        t.inverse(crate::transform::bounds_center(raw), p)
    }

    /// A document-space length in this layer's grid. Scale only: an offset moves a length
    /// nowhere and a rotation does not change it. A non-uniform scale strictly makes a round
    /// brush an ellipse in grid space — the mean of the two axes is used instead, which is exact
    /// for the proportional scaling `⌘T` does by default and close for anything short of a
    /// deliberate squash.
    pub fn doc_length_to_grid(&self, length: f32) -> f32 {
        let Some(t) = self.transform.filter(|t| !t.is_identity()) else {
            return length;
        };
        let scale = (t.scale_x.abs() + t.scale_y.abs()) * 0.5;
        if scale > 1e-6 {
            length / scale
        } else {
            length
        }
    }

    /// The part of this layer's own grid that a document-space rectangle is showing.
    ///
    /// A transform moves where a layer's tiles *land*, so "which tiles are on screen" is a
    /// different question from "which document coordinates are on screen" the moment a layer
    /// has been dragged. Without this the renderer enumerates tiles by document coordinate and
    /// uploads the wrong ones — invisible while transforms were small nudges inside the paper,
    /// and immediately visible now that a pasted layer can be mostly off-canvas and dragged in.
    ///
    /// The AABB of the inverse-mapped corners, so a rotation returns the whole span it could
    /// have come from rather than a rotated rectangle nothing can iterate.
    pub fn doc_rect_to_grid(&self, doc_rect: DocRect) -> DocRect {
        let Some(t) = self.transform.filter(|t| !t.is_identity()) else {
            return doc_rect;
        };
        let Some(raw) = self.content_bounds() else {
            return doc_rect;
        };
        let pivot = crate::transform::bounds_center(raw);
        let (x0, y0) = (doc_rect.min_x as f32, doc_rect.min_y as f32);
        let (x1, y1) = (doc_rect.max_x as f32 + 1.0, doc_rect.max_y as f32 + 1.0);
        let corners = [
            t.inverse(pivot, (x0, y0)),
            t.inverse(pivot, (x1, y0)),
            t.inverse(pivot, (x0, y1)),
            t.inverse(pivot, (x1, y1)),
        ];
        let min_x = corners.iter().map(|c| c.0).fold(f32::INFINITY, f32::min);
        let min_y = corners.iter().map(|c| c.1).fold(f32::INFINITY, f32::min);
        let max_x = corners
            .iter()
            .map(|c| c.0)
            .fold(f32::NEG_INFINITY, f32::max);
        let max_y = corners
            .iter()
            .map(|c| c.1)
            .fold(f32::NEG_INFINITY, f32::max);
        DocRect::from_floats(min_x, min_y, max_x, max_y)
    }

    /// The box the layer actually occupies: tight to its non-transparent pixels for anything
    /// with tiles, and the item's parametric bounds for a vector.
    ///
    /// **This used to be tile-granular** — the union of the 256×256 cells that held anything —
    /// which meant a pasted 300×200 photo reported a 512×256 box and the `⌘T` frame drew
    /// visibly wider than the picture inside it. Worse, the transform *pivot* is the centre of
    /// this box on both the CPU and the GPU, so scaling and rotation turned about a point that
    /// depended on where the content happened to fall against the tile grid.
    ///
    /// Everything that answers "where is this layer" reads this one function — the transform
    /// frame and its handles, the pivot in `vs_tile` and in the flatten walk, the hover
    /// outline, the pick reject, `Move`. They have to agree, so there is one definition rather
    /// than a cheap one for the hot path and a tight one for the UI.
    ///
    /// A layer whose tiles exist but hold nothing opaque now reports `None`, the same as an
    /// empty one. That is the honest answer: there is nothing there to frame, transform or pick.
    pub fn content_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        match &self.content {
            LayerContent::Raster(tiles) | LayerContent::Text { tiles, .. } => {
                let r = if let Some(mask) = self.mask.as_deref() {
                    let opaque = tiles.opaque_bounds()?;
                    let mut cache = self.masked_bounds.lock();
                    if let Some((key, answer)) = *cache {
                        if key == opaque {
                            return answer.map(|r| {
                                (
                                    r.min_x as f32,
                                    r.min_y as f32,
                                    r.max_x as f32 + 1.0,
                                    r.max_y as f32 + 1.0,
                                )
                            });
                        }
                    }
                    let answer = scan_masked_bounds(tiles, mask, self.transform);
                    *cache = Some((opaque, answer));
                    answer?
                } else {
                    tiles.opaque_bounds()?
                };
                Some((
                    r.min_x as f32,
                    r.min_y as f32,
                    r.max_x as f32 + 1.0,
                    r.max_y as f32 + 1.0,
                ))
            }
            LayerContent::Vector(item) => match item {
                VectorItem::Path(path) => {
                    let key = VectorBoundsKey::of(&path.points)?;
                    let mut cache = self.vector_bounds.lock();
                    if let Some((cached_key, answer)) = *cache {
                        if cached_key == key {
                            return answer;
                        }
                    }
                    let answer = item.bounds();
                    *cache = Some((key, answer));
                    answer
                }
                VectorItem::Shape(_) => item.bounds(),
            },
        }
    }
}

impl PartialEq for Layer {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.name == other.name
            && self.visible == other.visible
            && self.content == other.content
            && self.opacity == other.opacity
            && self.blend_mode == other.blend_mode
            && self.adjustments == other.adjustments
            && self.transform == other.transform
            && self.locked == other.locked
            && self.mask == other.mask
    }
}
