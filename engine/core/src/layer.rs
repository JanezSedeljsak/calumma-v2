use crate::tile::{DirtyChannel, DocRect, TileCoord, TileGrid, TILE_SIZE};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use uuid::Uuid;

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
            mask: None,
        }
    }

    pub fn vector(name: impl Into<String>, paths: Vec<VectorPath>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            visible: true,
            content: LayerContent::Vector(paths),
            mask: None,
        }
    }

    pub fn paper(width: u32, height: u32) -> Self {
        let w = width.max(1) as f32;
        let h = height.max(1) as f32;
        Self::vector(
            crate::names::PAPER,
            vec![VectorPath {
                points: vec![(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)],
                closed: true,
                fill: true,
                color: [255, 255, 255, 255],
                stroke_width: 0.0,
            }],
        )
    }

    pub fn is_paper(&self) -> bool {
        self.name == crate::names::PAPER && self.content.is_vector()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_layer_is_raster() {
        let layer = Layer::new("L", 64, 64);
        assert!(layer.content.is_raster());
        assert!(layer.tiles().unwrap().is_empty());
    }

    #[test]
    fn vector_layer_reports_path_bounds() {
        let layer = Layer::vector(
            "V",
            vec![VectorPath {
                points: vec![(10.0, 20.0), (30.0, 40.0)],
                closed: false,
                fill: false,
                color: [0, 0, 0, 255],
                stroke_width: 2.0,
            }],
        );
        assert!(layer.content.is_vector());
        assert_eq!(layer.content_bounds(), Some((10.0, 20.0, 30.0, 40.0)));
    }

    #[test]
    fn vector_layer_tile_access_is_none_not_panic() {
        let mut layer = Layer::vector("V", Vec::new());
        assert!(layer.tiles().is_none());
        assert!(layer.tiles_mut().is_none());
        assert!(layer.dirty_tiles(DirtyChannel::Render).is_none());
        assert!(layer.clear().is_empty());
        layer.mark_all_dirty();
    }

    #[test]
    fn setting_mask_dirties_every_tile() {
        let mut layer = Layer::new("L", 1024, 1024);
        layer.tiles_mut().unwrap().set_pixel(10, 10, [1, 2, 3, 255]);
        layer
            .tiles_mut()
            .unwrap()
            .set_pixel(600, 600, [1, 2, 3, 255]);
        layer.clear_dirty(DirtyChannel::Render);
        assert!(layer.dirty_tiles(DirtyChannel::Render).unwrap().is_empty());
        layer.set_mask(Some(vec![255; 1024 * 1024]));
        assert_eq!(layer.dirty_tiles(DirtyChannel::Render).unwrap().len(), 2);
    }
}
