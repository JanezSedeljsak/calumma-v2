//! What the non-brush tools commit on release: a shape, a selection of any kind, and a fill.

use super::*;

impl Document {
    pub(super) fn commit_shape(&mut self, shape: Shape) {
        if self.tool_blocked(self.tool) {
            return;
        }
        let (fill_color, stroke_color) = self.shape_paint(shape.tool);
        if self.effective_vector_mode() {
            if let Some(item) = vector::item_from_shape(shape, fill_color, stroke_color) {
                self.push_vector_item(item);
                return;
            }
        }
        let (x0, y0, x1, y1) = shape.bounds();
        let Some(rect) = DocRect::from_floats(x0, y0, x1, y1).intersect(self.bounds()) else {
            return;
        };

        let mut coords = TileSet::default();
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

        let selection = self.selection.clone();
        let mut painted = false;
        if let Some(tiles) = self.layers.get_mut(active).and_then(|l| l.tiles_mut()) {
            // Fill first, stroke over it — the same order the shader composites in, so a
            // translucent border reads the same on the board as it does once committed.
            let touched = tiles.paint_rect(rect, |px, py, dst| {
                let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
                if let Some(sel) = &selection {
                    if !sel.contains(x, y) {
                        return None;
                    }
                }
                let parts = [
                    ink_sample(shape.fill_distance(x, y), fill_color),
                    ink_sample(shape.stroke_distance(x, y), stroke_color),
                ];
                let mut out = dst;
                let mut inked = false;
                for src in parts.into_iter().flatten() {
                    out = blend_over(out, src);
                    inked = true;
                }
                inked.then_some(out)
            });
            painted = touched > 0;
        }
        if !painted {
            return;
        }
        self.history
            .push_layer_tiles(layer_id, before, Some(active));
    }

    pub(super) fn commit_selection_shape(&mut self, shape: Shape) {
        if self.tool_blocked(self.tool) {
            return;
        }
        let geom = match shape.tool {
            Tool::Rect => SelectionShape::Rect {
                start: shape.start,
                end: shape.end,
            },
            Tool::Ellipse => SelectionShape::Ellipse {
                start: shape.start,
                end: shape.end,
            },
            _ => return,
        };
        self.commit_geometry_selection(geom);
    }

    /// A marquee or a lasso, kept only where the active layer has ink.
    ///
    /// Two answers, not one. A layer with **nothing painted** has nothing for the region to hug,
    /// so the geometry stands as drawn — that is the only thing a marquee on a fresh layer could
    /// sensibly mean, and it is what Photoshop does everywhere. A layer that *does* have ink and
    /// simply none inside the region leaves the selection alone, the same as a wand that hit
    /// nothing: the gesture asked for artwork and found none.
    pub(super) fn commit_geometry_selection(&mut self, geom: SelectionShape) {
        let doc_bounds = self.bounds();
        let Some(layer) = self.layers.get(self.active_layer) else {
            return;
        };
        if crate::select_sample::painted_scope(layer, doc_bounds).is_none() {
            self.selection = Some(Selection { shape: geom });
            return;
        }
        let Some(mask) = crate::select_sample::selection_from_geometry(layer, doc_bounds, &geom)
        else {
            return;
        };
        self.selection = Some(Selection {
            shape: SelectionShape::Mask(mask),
        });
    }

    /// Select by color: flood from the clicked pixel of the active layer and keep what the
    /// walk reached.
    ///
    /// Scope is the **document**, not the layer's painted box. Alpha counts toward the
    /// tolerance, so the empty space around a drawing is a colour like any other and has to be
    /// floodable — clicking beside a sketch selects the space beside it, which is how you get
    /// at a background to fill or delete it. Colour range is the one that stays scoped to the
    /// ink, because it walks every pixel rather than following one blob.
    ///
    /// The previous selection does not clip the new one either: the wand *replaces* the selection,
    /// so letting it bound the walk would make a second click inside a previous wand
    /// result unable to grow past it. That is the one place the wand deliberately diverges from
    /// the bucket, which paints *into* the selection and so has to respect it.
    ///
    /// Reading the active layer (not the composite) is what makes the wand answer about the
    /// thing being edited: clicking a sketch's white background selects the background of that
    /// layer, not of the Paper showing through it — and outside that layer's ink the sample
    /// answers transparent, which is exactly what is there.
    pub(super) fn commit_magic_wand(&mut self, doc_x: f32, doc_y: f32) {
        if self.tool_blocked(Tool::MagicWand) {
            return;
        }
        let x = doc_x.floor() as i32;
        let y = doc_y.floor() as i32;
        let scope = self.bounds();
        if !scope.contains(x, y) {
            return;
        }
        let Some(layer) = self.layers.get(self.active_layer) else {
            return;
        };
        let Some(sample) = crate::select_sample::LayerSelectSample::new(layer, scope) else {
            return;
        };
        let tolerance = self.tolerance;
        self.selection =
            crate::fill::flood_region_pixels(scope, x, y, tolerance, |px, py| sample.pixel(px, py))
                .map(|mask| Selection {
                    shape: SelectionShape::Mask(mask),
                });
    }

    /// Clicking with Select Color is the eyedropper half of Photoshop's Color Range: it samples
    /// the pixel into the match swatch and then selects everything that matches it. Changing the
    /// swatch or the tolerance afterwards re-runs against the same layer — see
    /// `Document::reselect_color`.
    pub(super) fn commit_select_color(&mut self, doc_x: f32, doc_y: f32) {
        if self.tool_blocked(Tool::SelectColor) {
            return;
        }
        let x = doc_x.floor() as i32;
        let y = doc_y.floor() as i32;
        let doc_bounds = self.bounds();
        let Some(layer) = self.layers.get(self.active_layer) else {
            return;
        };
        let Some(sample) = crate::select_sample::LayerSelectSample::new(layer, doc_bounds) else {
            return;
        };
        if !sample.scope.contains(x, y) {
            return;
        }
        self.select_color = sample.pixel(x, y);
        self.apply_color_range();
    }

    pub(super) fn commit_lasso_selection(&mut self) {
        self.stroke_active = false;
        if self.tool_blocked(Tool::SelectLasso) {
            self.stroke_points.clear();
            return;
        }
        let points: Vec<(f32, f32)> = std::mem::take(&mut self.stroke_points)
            .into_iter()
            .map(|p| (p.x, p.y))
            .collect();
        let points = crate::select_sample::simplify_lasso_points(points);
        if points.len() < 3 {
            return;
        }
        self.commit_geometry_selection(SelectionShape::Lasso { points });
    }

    /// Tolerance is one knob for the bucket, the wand and Select Color. Only the last of the
    /// three re-runs on it: it is Color Range's Fuzziness, where the point is watching the
    /// selection open up as you drag. The other two apply it to their next click, which is what
    /// a flood from a pixel you are no longer pointing at would have to guess.
    pub fn set_tolerance(&mut self, tolerance: u8) {
        let next = tolerance.clamp(TOLERANCE_MIN, TOLERANCE_MAX);
        if self.tolerance == next {
            return;
        }
        self.tolerance = next;
        self.reselect_color();
    }

    /// The match colour, pushed from the shell's tertiary swatch. Ringing that swatch while
    /// Select Color is in hand re-runs the selection, which is what makes it *the* match colour
    /// rather than a note about one — Photoshop's Color Range updates as you re-sample too.
    pub fn set_select_color(&mut self, color: [u8; 4]) {
        if self.select_color == color {
            return;
        }
        self.select_color = color;
        self.reselect_color();
    }

    pub fn select_color(&self) -> [u8; 4] {
        self.select_color
    }

    pub(super) fn commit_fill(&mut self, doc_x: f32, doc_y: f32) {
        if self.tool_blocked(Tool::Fill) {
            return;
        }
        let x = doc_x.floor() as i32;
        let y = doc_y.floor() as i32;
        let scope = match &self.selection {
            Some(sel) => sel.bounds(),
            None => self.bounds(),
        };
        let Some(scope) = scope.intersect(self.bounds()) else {
            return;
        };
        if !scope.contains(x, y) {
            return;
        }
        if let Some(sel) = &self.selection {
            if !sel.contains(x as f32 + 0.5, y as f32 + 0.5) {
                return;
            }
        }
        let active = self.active_layer;
        let Some(layer) = self.layers.get(active) else {
            return;
        };
        let layer_id = layer.id.clone();
        let Some(grid) = layer.tiles() else {
            return;
        };
        let mut coords = TileSet::default();
        tiles_covering(scope, &mut coords);
        let coords: Vec<TileCoord> = coords
            .into_iter()
            .filter(|c| grid.tile_in_bounds(*c))
            .collect();
        if coords.is_empty() {
            return;
        }
        let before = grid.snapshot_tiles(&coords);
        let color = self.ink_rgba();
        let selection = self.selection.clone();
        let tolerance = self.tolerance;
        let (comp_w, comp_h, composite) = self.composite_rgba();
        let mut touched = 0;
        if let Some(tiles) = self.layers.get_mut(active).and_then(|l| l.tiles_mut()) {
            touched = crate::fill::flood_fill_sampled(
                tiles,
                scope,
                x,
                y,
                color,
                selection.as_ref(),
                tolerance,
                |px, py| crate::fill::pixel_in_rgba(&composite, comp_w, comp_h, px, py),
            );
        }
        if touched == 0 {
            return;
        }
        self.history
            .push_layer_tiles(layer_id, before, Some(active));
    }
}
