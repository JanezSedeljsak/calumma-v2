use crate::filters::Adjustments;
use crate::tile::{DirtyChannel, DocRect, TileCoord, TileGrid, TILE_SIZE};
use crate::transform::LayerTransform;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
}

impl BlendMode {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Normal),
            1 => Some(Self::Multiply),
            2 => Some(Self::Screen),
            _ => None,
        }
    }

    pub fn as_u32(self) -> u32 {
        match self {
            Self::Normal => 0,
            Self::Multiply => 1,
            Self::Screen => 2,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VectorPath {
    pub points: Vec<(f32, f32)>,
    pub closed: bool,
    pub fill: bool,
    pub color: [u8; 4],
    pub stroke_width: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LayerContent {
    Raster(TileGrid),
    Vector(Vec<VectorPath>),
}

impl LayerContent {
    pub fn raster(width: u32, height: u32) -> Self {
        Self::Raster(TileGrid::new(width, height))
    }

    pub fn is_raster(&self) -> bool {
        matches!(self, Self::Raster(_))
    }

    pub fn is_vector(&self) -> bool {
        matches!(self, Self::Vector(_))
    }

    pub fn tiles(&self) -> Option<&TileGrid> {
        match self {
            Self::Raster(tiles) => Some(tiles),
            Self::Vector(_) => None,
        }
    }

    pub fn tiles_mut(&mut self) -> Option<&mut TileGrid> {
        match self {
            Self::Raster(tiles) => Some(tiles),
            Self::Vector(_) => None,
        }
    }

    pub fn paths(&self) -> Option<&[VectorPath]> {
        match self {
            Self::Vector(paths) => Some(paths.as_slice()),
            Self::Raster(_) => None,
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
            mask: None,
        }
    }

    pub fn vector(name: impl Into<String>, paths: Vec<VectorPath>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            visible: true,
            content: LayerContent::Vector(paths),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            adjustments: None,
            transform: None,
            mask: None,
        }
    }

    pub fn paper(width: u32, height: u32) -> Self {
        let mut layer = Self::new(crate::names::PAPER, width, height);
        let bounds = DocRect::from_size(width.max(1), height.max(1));
        if let Some(tiles) = layer.tiles_mut() {
            tiles.paint_rect(bounds, |_, _, _| Some([255, 255, 255, 255]));
        }
        layer
    }

    pub fn is_paper(&self) -> bool {
        self.name == crate::names::PAPER
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

    pub fn dirty_tiles(&self, channel: DirtyChannel) -> Option<&HashSet<TileCoord>> {
        self.tiles().map(|t| t.dirty_tiles(channel))
    }

    pub fn mark_all_dirty(&mut self) {
        if let Some(tiles) = self.tiles_mut() {
            tiles.mark_all_dirty();
        }
    }

    pub fn clear_dirty(&mut self, channel: DirtyChannel) {
        if let Some(tiles) = self.tiles_mut() {
            tiles.clear_dirty(channel);
        }
    }

    pub fn clear(&mut self) -> HashMap<TileCoord, Option<Arc<Vec<u8>>>> {
        let Some(tiles) = self.tiles_mut() else {
            return HashMap::new();
        };
        let coords: Vec<_> = tiles.coords().collect();
        let snap = tiles.snapshot_tiles(&coords);
        tiles.clear();
        snap
    }

    pub fn content_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        match &self.content {
            LayerContent::Raster(tiles) => {
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
            LayerContent::Vector(paths) => {
                if paths.is_empty() {
                    return None;
                }
                let mut min_x = f32::INFINITY;
                let mut min_y = f32::INFINITY;
                let mut max_x = f32::NEG_INFINITY;
                let mut max_y = f32::NEG_INFINITY;
                for path in paths {
                    for &(x, y) in &path.points {
                        min_x = min_x.min(x);
                        min_y = min_y.min(y);
                        max_x = max_x.max(x);
                        max_y = max_y.max(y);
                    }
                }
                if !min_x.is_finite() {
                    None
                } else {
                    Some((min_x, min_y, max_x, max_y))
                }
            }
        }
    }
}
