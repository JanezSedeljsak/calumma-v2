//! Where tiles are: the tile coordinate and the integer document rectangle every tile walk is
//! clipped by.

use super::*;

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

pub(super) fn union(a: DocRect, b: DocRect) -> DocRect {
    DocRect::new(
        a.min_x.min(b.min_x),
        a.min_y.min(b.min_y),
        a.max_x.max(b.max_x),
        a.max_y.max(b.max_y),
    )
}
