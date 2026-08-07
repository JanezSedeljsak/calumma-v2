use crate::camera::Camera;
use crate::history::{History, TileSnapshot};
use crate::layer::Layer;
use crate::limits::{
    BRUSH_SIZE_DEFAULT, DEFAULT_INK, MIN_STAMP_SPACING, MIN_STROKE_POINT_DISTANCE,
    STAMP_COVERAGE_PADDING, STAMP_SPACING_RATIO, STROKE_POINT_CAPACITY,
};
use crate::shape::{Shape, Tool};
use crate::tile::{blend_over, DirtyChannel, DocRect, TileCoord, TILE_SIZE};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokePoint {
    pub x: f32,
    pub y: f32,
}

pub fn stamp_spacing(radius: f32) -> f32 {
    (radius * STAMP_SPACING_RATIO).max(MIN_STAMP_SPACING)
}

pub fn stroke_stamps(points: &[StrokePoint], radius: f32) -> Vec<StrokePoint> {
    let mut out = Vec::with_capacity(points.len());
    let Some(first) = points.first() else {
        return out;
    };
    out.push(*first);
    let spacing = stamp_spacing(radius);
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let distance = (dx * dx + dy * dy).sqrt();
        if !distance.is_finite() || distance <= spacing {
            out.push(b);
            continue;
        }
        let steps = (distance / spacing).ceil() as usize;
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            out.push(StrokePoint {
                x: a.x + dx * t,
                y: a.y + dy * t,
            });
        }
    }
    out
}

fn tiles_covering(rect: DocRect, out: &mut HashSet<TileCoord>) {
    let (tx0, ty0, tx1, ty1) = rect.tile_span();
    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            out.insert(TileCoord { x: tx, y: ty });
        }
    }
}

fn stamps_bounds(stamps: &[StrokePoint], radius: f32) -> Option<DocRect> {
    let pad = radius + STAMP_COVERAGE_PADDING;
    let first = stamps.first()?;
    let mut min_x = first.x;
    let mut min_y = first.y;
    let mut max_x = first.x;
    let mut max_y = first.y;
    for p in stamps {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }
    Some(DocRect::from_floats(
        min_x - pad,
        min_y - pad,
        max_x + pad,
        max_y + pad,
    ))
}

#[derive(Clone, Debug)]
pub struct Document {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub layers: Vec<Layer>,
    pub active_layer: usize,
    pub camera: Camera,
    pub history: History,
    pub tool: Tool,
    pub color: [u8; 4],
    pub brush_size: f32,
    pub fill: bool,
    pub dark_theme: bool,
    pub hover_layer: Option<usize>,
    pub stroke_active: bool,
    pub stroke_points: Vec<StrokePoint>,
    pub preview_shape: Option<Shape>,
    stroke_before: TileSnapshot,
}

impl Document {
    pub fn new(id: String, name: impl Into<String>, width: u32, height: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let paper = Layer::paper(width, height);
        let paint = Layer::new(crate::names::LAYER_ONE, width, height);
        Self {
            id,
            name: name.into(),
            width,
            height,
            layers: vec![paper, paint],
            active_layer: 1,
            camera: Camera::default(),
            history: History::default(),
            tool: Tool::Pen,
            color: DEFAULT_INK,
            brush_size: BRUSH_SIZE_DEFAULT,
            fill: false,
            dark_theme: true,
            hover_layer: None,
            stroke_active: false,
            stroke_points: Vec::with_capacity(STROKE_POINT_CAPACITY),
            preview_shape: None,
            stroke_before: TileSnapshot::new(),
        }
    }

    pub fn ensure_paper_layer(&mut self) {
        if self.layers.iter().any(Layer::is_paper) {
            return;
        }
        self.layers.insert(0, Layer::paper(self.width, self.height));
        self.active_layer += 1;
    }

    pub fn bounds(&self) -> DocRect {
        DocRect::from_size(self.width, self.height)
    }

    pub fn visible_rect(&self) -> Option<DocRect> {
        self.camera
            .visible_doc_rect(self.width as f32, self.height as f32)
    }

    pub fn active(&self) -> Option<&Layer> {
        self.layers.get(self.active_layer)
    }

    pub fn active_mut(&mut self) -> Option<&mut Layer> {
        self.layers.get_mut(self.active_layer)
    }

    pub fn add_layer(&mut self, name: impl Into<String>) {
        self.layers.push(Layer::new(name, self.width, self.height));
        self.active_layer = self.layers.len() - 1;
    }

    pub fn remove_layer(&mut self, index: usize) -> bool {
        if index >= self.layers.len() {
            return false;
        }
        self.layers.remove(index);
        if self.layers.is_empty() {
            self.active_layer = 0;
            self.hover_layer = None;
            return true;
        }
        if self.active_layer > index {
            self.active_layer -= 1;
        } else if self.active_layer >= self.layers.len() {
            self.active_layer = self.layers.len() - 1;
        }
        if let Some(hover) = self.hover_layer {
            if hover == index {
                self.hover_layer = None;
            } else if hover > index {
                self.hover_layer = Some(hover - 1);
            }
        }
        true
    }

    pub fn set_layer_visible(&mut self, index: usize, visible: bool) {
        if let Some(layer) = self.layers.get_mut(index) {
            layer.visible = visible;
        }
    }

    pub fn set_active_layer(&mut self, index: usize) {
        if index < self.layers.len() {
            self.active_layer = index;
        }
    }

    pub fn resize_viewport(&mut self, width: f32, height: f32, dpr: f32) {
        self.camera.viewport_width = width.max(1.0);
        self.camera.viewport_height = height.max(1.0);
        self.camera.dpr = dpr.max(1.0);
        self.camera
            .clamp_to_board(self.width as f32, self.height as f32);
    }

    pub fn fit_to_view(&mut self) {
        self.camera.fit(self.width as f32, self.height as f32);
    }

    pub fn pointer_down(&mut self, screen_x: f32, screen_y: f32) {
        let (dx, dy) = self.camera.to_doc(screen_x, screen_y);
        if self.tool == Tool::Pen {
            self.begin_stroke();
            self.push_stroke_point(dx, dy);
        } else {
            self.preview_shape = Some(Shape {
                tool: self.tool,
                start: (dx, dy),
                end: (dx, dy),
                half_width: self.brush_size * 0.5,
                fill: self.fill,
            });
        }
    }

    pub fn pointer_move(&mut self, screen_x: f32, screen_y: f32) {
        let (dx, dy) = self.camera.to_doc(screen_x, screen_y);
        if self.tool == Tool::Pen && self.stroke_active {
            self.push_stroke_point(dx, dy);
        } else if let Some(shape) = &mut self.preview_shape {
            shape.end = (dx, dy);
            shape.half_width = self.brush_size * 0.5;
            shape.fill = self.fill;
        }
    }

    pub fn pointer_up(&mut self, screen_x: f32, screen_y: f32) {
        let (dx, dy) = self.camera.to_doc(screen_x, screen_y);
        if self.tool == Tool::Pen {
            self.push_stroke_point(dx, dy);
            self.commit_stroke();
        } else if let Some(mut shape) = self.preview_shape.take() {
            shape.end = (dx, dy);
            shape.half_width = self.brush_size * 0.5;
            shape.fill = self.fill;
            self.commit_shape(shape);
        }
    }

    fn begin_stroke(&mut self) {
        self.stroke_active = true;
        self.stroke_points.clear();
        self.stroke_before.clear();
    }

    fn push_stroke_point(&mut self, x: f32, y: f32) {
        if let Some(last) = self.stroke_points.last() {
            let dx = x - last.x;
            let dy = y - last.y;
            if dx * dx + dy * dy < MIN_STROKE_POINT_DISTANCE * MIN_STROKE_POINT_DISTANCE {
                return;
            }
        }
        self.stroke_points.push(StrokePoint { x, y });
    }

    fn commit_stroke(&mut self) {
        if !self.stroke_active {
            return;
        }
        self.stroke_active = false;
        let points = std::mem::take(&mut self.stroke_points);
        if points.is_empty() {
            return;
        }
        let radius = self.brush_size * 0.5;
        let color = self.color;
        let active = self.active_layer;
        let stamps = stroke_stamps(&points, radius);

        let Some(span) = stamps_bounds(&stamps, radius) else {
            return;
        };
        let Some(span) = span.intersect(self.bounds()) else {
            return;
        };

        let mut touched = HashSet::new();
        tiles_covering(span, &mut touched);

        let Some(layer) = self.layers.get(active) else {
            return;
        };
        let layer_id = layer.id.clone();
        let Some(grid) = layer.tiles() else {
            return;
        };
        let touched: Vec<TileCoord> = touched
            .into_iter()
            .filter(|c| grid.tile_in_bounds(*c))
            .collect();
        if touched.is_empty() {
            return;
        }
        self.stroke_before = grid.snapshot_tiles(&touched);

        let mut painted = false;
        if let Some(tiles) = self.layers.get_mut(active).and_then(|l| l.tiles_mut()) {
            for p in &stamps {
                if tiles.stamp_disc(p.x, p.y, radius, color) > 0 {
                    painted = true;
                }
            }
        }

        if !painted {
            self.stroke_before.clear();
            return;
        }
        let before = std::mem::take(&mut self.stroke_before);
        self.history
            .push_layer_tiles(layer_id, before, Some(active));
    }

    fn commit_shape(&mut self, shape: Shape) {
        let (x0, y0, x1, y1) = shape.bounds();
        let Some(rect) = DocRect::from_floats(x0, y0, x1, y1).intersect(self.bounds()) else {
            return;
        };

        let mut coords = HashSet::new();
        tiles_covering(rect, &mut coords);
        let coords: Vec<TileCoord> = coords.into_iter().collect();

        let active = self.active_layer;
        let Some(layer) = self.layers.get(active) else {
            return;
        };
        let layer_id = layer.id.clone();
        let Some(grid) = layer.tiles() else {
            return;
        };
        let before = grid.snapshot_tiles(&coords);
        let color = self.color;

        let mut painted = false;
        if let Some(tiles) = self.layers.get_mut(active).and_then(|l| l.tiles_mut()) {
            let touched = tiles.paint_rect(rect, |px, py, dst| {
                let coverage = shape.coverage(px as f32 + 0.5, py as f32 + 0.5);
                if coverage <= 0.0 {
                    return None;
                }
                let mut rgba = color;
                rgba[3] = ((color[3] as f32) * coverage) as u8;
                if rgba[3] == 0 {
                    return None;
                }
                Some(blend_over(dst, rgba))
            });
            painted = touched > 0;
        }
        if !painted {
            return;
        }
        self.history
            .push_layer_tiles(layer_id, before, Some(active));
    }

    pub fn undo(&mut self) -> bool {
        let mut active = self.active_layer;
        let changed = self.history.undo(&mut self.layers, &mut active);
        self.active_layer = active.min(self.layers.len().saturating_sub(1));
        changed
    }

    pub fn redo(&mut self) -> bool {
        let mut active = self.active_layer;
        let changed = self.history.redo(&mut self.layers, &mut active);
        self.active_layer = active.min(self.layers.len().saturating_sub(1));
        changed
    }

    pub fn clear_active_layer(&mut self) {
        let active = self.active_layer;
        let Some(layer) = self.layers.get_mut(active) else {
            return;
        };
        let layer_id = layer.id.clone();
        let snap = layer.clear();
        if snap.is_empty() {
            return;
        }
        self.history.push_layer_tiles(layer_id, snap, Some(active));
    }

    pub fn mark_all_layers_dirty(&mut self) {
        for layer in &mut self.layers {
            layer.mark_all_dirty();
        }
    }

    pub fn clear_layer_dirty(&mut self, channel: DirtyChannel) {
        for layer in &mut self.layers {
            layer.clear_dirty(channel);
        }
    }

    pub fn has_live_preview(&self) -> bool {
        self.stroke_active || self.preview_shape.is_some() || self.hover_layer.is_some()
    }

    pub fn tile_size(&self) -> u32 {
        TILE_SIZE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(doc: &Document, index: usize, x: i32, y: i32) -> [u8; 4] {
        doc.layers[index].tiles().unwrap().get_pixel(x, y)
    }

    #[test]
    fn pen_previews_then_commits() {
        let mut doc = Document::new("p".into(), "t", 256, 256);
        doc.resize_viewport(256.0, 256.0, 1.0);
        doc.fit_to_view();
        let (sx, sy) = doc.camera.to_screen(40.0, 40.0);
        doc.pointer_down(sx, sy);
        assert!(doc.stroke_active);
        assert!(!doc.stroke_points.is_empty());
        assert_eq!(pixel(&doc, doc.active_layer, 40, 40), [0, 0, 0, 0]);
        let (sx2, sy2) = doc.camera.to_screen(48.0, 40.0);
        doc.pointer_move(sx2, sy2);
        doc.pointer_up(sx2, sy2);
        assert!(!doc.stroke_active);
        assert!(doc.stroke_points.is_empty());
        assert_ne!(pixel(&doc, doc.active_layer, 40, 40), [0, 0, 0, 0]);
        assert!(doc.history.can_undo());
    }

    #[test]
    fn shape_preview_then_commit() {
        let mut doc = Document::new("p".into(), "t", 256, 256);
        doc.tool = Tool::Rect;
        doc.fill = true;
        doc.resize_viewport(256.0, 256.0, 1.0);
        doc.fit_to_view();
        let (s0x, s0y) = doc.camera.to_screen(20.0, 20.0);
        let (s1x, s1y) = doc.camera.to_screen(60.0, 60.0);
        doc.pointer_down(s0x, s0y);
        assert!(doc.preview_shape.is_some());
        doc.pointer_move(s1x, s1y);
        doc.pointer_up(s1x, s1y);
        assert!(doc.preview_shape.is_none());
        assert!(!doc.layers[doc.active_layer].tiles().unwrap().is_empty());
    }

    #[test]
    fn stamps_fill_gaps_between_points() {
        let points = [
            StrokePoint { x: 0.0, y: 0.0 },
            StrokePoint { x: 40.0, y: 0.0 },
        ];
        let stamps = stroke_stamps(&points, 2.0);
        assert!(stamps.len() > 40);
        for pair in stamps.windows(2) {
            let dx = pair[1].x - pair[0].x;
            let dy = pair[1].y - pair[0].y;
            assert!((dx * dx + dy * dy).sqrt() <= stamp_spacing(2.0) + 1e-3);
        }
        assert_eq!(stamps.last().map(|p| p.x), Some(40.0));
    }

    #[test]
    fn fast_stroke_paints_across_tiles_and_undoes() {
        let mut doc = Document::new("p".into(), "t", 512, 512);
        doc.resize_viewport(512.0, 512.0, 1.0);
        doc.fit_to_view();
        let (sx0, sy0) = doc.camera.to_screen(20.0, 20.0);
        let (sx1, sy1) = doc.camera.to_screen(400.0, 400.0);
        doc.pointer_down(sx0, sy0);
        doc.pointer_up(sx1, sy1);
        assert_eq!(doc.stroke_points.len(), 0);
        assert_ne!(pixel(&doc, doc.active_layer, 200, 200), [0, 0, 0, 0]);
        assert_ne!(pixel(&doc, doc.active_layer, 300, 300), [0, 0, 0, 0]);
        assert!(doc.undo());
        assert_eq!(pixel(&doc, doc.active_layer, 200, 200), [0, 0, 0, 0]);
        assert_eq!(pixel(&doc, doc.active_layer, 300, 300), [0, 0, 0, 0]);
        assert!(doc.layers[doc.active_layer].tiles().unwrap().is_empty());
    }

    #[test]
    fn shape_outside_board_paints_nothing() {
        let mut doc = Document::new("p".into(), "t", 128, 128);
        doc.tool = Tool::Rect;
        doc.resize_viewport(128.0, 128.0, 1.0);
        doc.fit_to_view();
        doc.commit_shape(Shape {
            tool: Tool::Rect,
            start: (-900.0, -900.0),
            end: (-500.0, -500.0),
            half_width: 1.5,
            fill: true,
        });
        assert!(doc.layers[doc.active_layer].tiles().unwrap().is_empty());
        assert!(!doc.history.can_undo());
    }

    #[test]
    fn undo_stroke() {
        let mut doc = Document::new("p".into(), "t", 128, 128);
        doc.resize_viewport(128.0, 128.0, 1.0);
        doc.fit_to_view();
        let (sx, sy) = doc.camera.to_screen(16.0, 16.0);
        doc.pointer_down(sx, sy);
        doc.pointer_up(sx, sy);
        assert!(doc.undo());
        assert_eq!(pixel(&doc, doc.active_layer, 16, 16), [0, 0, 0, 0]);
    }

    #[test]
    fn stroke_only_dirties_tiles_it_touched() {
        let mut doc = Document::new("p".into(), "t", 2048, 2048);
        doc.resize_viewport(2048.0, 2048.0, 1.0);
        doc.fit_to_view();
        doc.clear_layer_dirty(DirtyChannel::Render);
        let (sx, sy) = doc.camera.to_screen(100.0, 100.0);
        doc.pointer_down(sx, sy);
        doc.pointer_up(sx, sy);
        let dirty = doc.layers[doc.active_layer].dirty_tiles(DirtyChannel::Render).unwrap();
        assert_eq!(dirty.len(), 1);
        assert!(dirty.contains(&TileCoord { x: 0, y: 0 }));
    }

    #[test]
    fn undo_after_clear_restores_pixels() {
        let mut doc = Document::new("p".into(), "t", 128, 128);
        let i = doc.active_layer;
        doc.layers[i]
            .tiles_mut()
            .unwrap()
            .set_pixel(5, 5, [1, 2, 3, 255]);
        doc.clear_active_layer();
        assert_eq!(pixel(&doc, doc.active_layer, 5, 5), [0, 0, 0, 0]);
        assert!(doc.undo());
        assert_eq!(pixel(&doc, doc.active_layer, 5, 5), [1, 2, 3, 255]);
    }

    #[test]
    fn clearing_empty_layer_pushes_no_history() {
        let mut doc = Document::new("p".into(), "t", 128, 128);
        doc.clear_active_layer();
        assert!(!doc.history.can_undo());
    }
}
