use crate::filters::Adjustments;
use crate::limits::PAPER_WHITE;
use crate::tile::{DirtyChannel, DocRect, TileGrid, TileMap, TileSet, TILE_SIZE};
use crate::transform::LayerTransform;
use crate::vector::VectorItem;
use calumma_text::TextRun;
use std::sync::Arc;
use uuid::Uuid;

use num_enum::{IntoPrimitive, TryFromPrimitive};

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

#[derive(Clone, Debug, PartialEq)]
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

    pub fn clear(&mut self) -> TileMap<Option<Arc<Vec<u8>>>> {
        let Some(tiles) = self.tiles_mut() else {
            return TileMap::default();
        };
        let coords: Vec<_> = tiles.coords().collect();
        let snap = tiles.snapshot_tiles(&coords);
        tiles.clear();
        snap
    }

    pub fn content_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        match &self.content {
            LayerContent::Raster(tiles) | LayerContent::Text { tiles, .. } => {
                if tiles.is_empty() {
                    return None;
                }
                let ts = TILE_SIZE as i32;
                let mut acc: Option<DocRect> = None;
                for coord in tiles.coords() {
                    let (ox, oy) = coord.origin();
                    let cell = DocRect::new(ox, oy, ox + ts, oy + ts);
                    acc = Some(match acc {
                        None => cell,
                        Some(r) => DocRect::new(
                            r.min_x.min(cell.min_x),
                            r.min_y.min(cell.min_y),
                            r.max_x.max(cell.max_x),
                            r.max_y.max(cell.max_y),
                        ),
                    });
                }
                let r = acc?;
                Some((
                    r.min_x.max(0) as f32,
                    r.min_y.max(0) as f32,
                    r.max_x.min(tiles.width as i32) as f32,
                    r.max_y.min(tiles.height as i32) as f32,
                ))
            }
            LayerContent::Vector(item) => item.bounds(),
        }
    }

    pub fn opaque_pixel_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        match &self.content {
            LayerContent::Raster(tiles) | LayerContent::Text { tiles, .. } => {
                let r = tiles.opaque_bounds()?;
                Some((
                    r.min_x as f32,
                    r.min_y as f32,
                    r.max_x as f32 + 1.0,
                    r.max_y as f32 + 1.0,
                ))
            }
            LayerContent::Vector(_) => self.content_bounds(),
        }
    }
}
