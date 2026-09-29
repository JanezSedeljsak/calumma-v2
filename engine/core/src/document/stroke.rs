//! Brush strokes: stamping along the pointer, the live blur / clone / heal passes that paint as
//! the pointer moves, and the commit that lands a stroke in the layer's tiles on pointer-up.

use super::*;

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

pub(super) fn tiles_covering(rect: DocRect, out: &mut TileSet) {
    let (tx0, ty0, tx1, ty1) = rect.tile_span();
    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            out.insert(TileCoord { x: tx, y: ty });
        }
    }
}

pub(super) fn stamps_bounds(stamps: &[StrokePoint], radius: f32) -> Option<DocRect> {
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

/// Same padded bounding box as `stamps_bounds`, over grid-space points already mapped through
/// `doc_point_to_grid` — what a stroke needs `grow_extent` to cover on a moved or scaled layer,
/// the way a paste already covers the image it drops off the paper.
pub(super) fn points_bounds(points: &[(f32, f32)], radius: f32) -> Option<DocRect> {
    let pad = radius + STAMP_COVERAGE_PADDING;
    let first = points.first()?;
    let mut min_x = first.0;
    let mut min_y = first.1;
    let mut max_x = first.0;
    let mut max_y = first.1;
    for p in points {
        min_x = min_x.min(p.0);
        min_y = min_y.min(p.1);
        max_x = max_x.max(p.0);
        max_y = max_y.max(p.1);
    }
    Some(DocRect::from_floats(
        min_x - pad,
        min_y - pad,
        max_x + pad,
        max_y + pad,
    ))
}

pub(super) type SourceStampFn = fn(
    &mut crate::tile::TileGrid,
    &[(f32, f32)],
    f32,
    (i32, i32),
    Option<&crate::selection::Selection>,
) -> usize;

impl Document {
    pub(super) fn begin_stroke(&mut self) {
        self.stroke_active = true;
        self.stroke_points.clear();
        self.stroke_generation = self.stroke_generation.wrapping_add(1);
        self.stroke_straight_anchor = None;
        self.stroke_before.clear();
        self.stroke_selection = self.selection.clone();
        self.live_stamp_progress = 0;
        self.live_stamp_painted = false;
        // Unaligned means every stroke starts fresh off the anchor: dropping the offset here
        // is what makes the next `clone_pending_stamps` / `heal_pending_stamps` recompute it
        // from `anchor` instead of carrying the last stroke's.
        if !self.clone_aligned {
            if let Some(source) = self.clone_source.as_mut() {
                source.offset = None;
            }
        }
    }

    /// Identifies the stroke `stroke_points` currently belongs to. See the field's own note —
    /// this exists so the renderer can append to GPU coverage it has already accumulated
    /// instead of rasterizing the whole stroke again every frame.
    pub fn stroke_generation(&self) -> u64 {
        self.stroke_generation
    }

    pub(super) fn push_stroke_point(&mut self, x: f32, y: f32) {
        if self.shift_held && matches!(self.tool, Tool::Pen | Tool::Eraser) {
            let anchor = *self
                .stroke_straight_anchor
                .get_or_insert(self.stroke_points.len().saturating_sub(1));
            if self.stroke_points.len() > anchor + 1 {
                self.stroke_generation = self.stroke_generation.wrapping_add(1);
            }
            self.stroke_points.truncate(anchor + 1);
            if let Some(anchor_pt) = self.stroke_points.get(anchor) {
                let dx = x - anchor_pt.x;
                let dy = y - anchor_pt.y;
                if dx * dx + dy * dy < MIN_STROKE_POINT_DISTANCE * MIN_STROKE_POINT_DISTANCE {
                    return;
                }
            }
            self.stroke_points.push(StrokePoint { x, y });
            return;
        }
        self.stroke_straight_anchor = None;
        if let Some(last) = self.stroke_points.last() {
            let dx = x - last.x;
            let dy = y - last.y;
            if dx * dx + dy * dy < MIN_STROKE_POINT_DISTANCE * MIN_STROKE_POINT_DISTANCE {
                return;
            }
        }
        self.stroke_points.push(StrokePoint { x, y });
    }

    pub fn set_blur_strength(&mut self, strength: f32) {
        self.blur_strength = strength.clamp(BLUR_STRENGTH_MIN, BLUR_STRENGTH_MAX);
    }

    pub fn set_clone_aligned(&mut self, aligned: bool) {
        self.clone_aligned = aligned;
    }

    /// `⌥`-click: where the clone stamp / healing brush reads from next. Always replaces
    /// whatever anchor was there and drops any carried-over offset — a fresh click means
    /// "start relative to here," aligned or not.
    pub fn set_clone_anchor(&mut self, x: f32, y: f32) {
        self.clone_source = Some(CloneSource {
            anchor: (x, y),
            offset: None,
        });
    }

    /// Where the board should draw the source crosshair: the anchor itself before a stroke has
    /// fixed an offset, then the point that offset tracks the pointer to. `None` off the clone
    /// stamp and the healing brush, or before either has ever had an anchor set.
    pub fn clone_source_cursor(&self) -> Option<(f32, f32)> {
        if !matches!(self.tool, Tool::Clone | Tool::Heal) {
            return None;
        }
        // The same guards `brush_ring` runs before promising a stamp: a source crosshair the
        // engine would then refuse to paint from is worse than no crosshair at all.
        if self.transform_active || self.tool_blocked(self.tool) {
            return None;
        }
        let source = self.clone_source?;
        match source.offset {
            Some(offset) => {
                let hover = self.pointer_hover?;
                if !self.brush_reaches(hover) {
                    return None;
                }
                Some((hover.0 + offset.0, hover.1 + offset.1))
            }
            None => Some(source.anchor),
        }
    }

    pub fn set_brush(&mut self, brush: Brush) {
        self.brush = brush;
    }

    /// The profile the *current* stroke lays ink down with. The pen carries a whole brush; the
    /// eraser carries only an edge, since grain and flow describe ink being put down and it is
    /// taking ink away. Everything else draws hard-edged.
    pub fn active_brush_profile(&self) -> BrushProfile {
        match self.tool {
            Tool::Pen => self.brush.profile(),
            Tool::Eraser => BrushProfile {
                hardness: self.eraser_hardness,
                ..BrushProfile::HARD
            },
            _ => BrushProfile::HARD,
        }
    }

    pub fn set_eraser_hardness(&mut self, hardness: f32) {
        self.eraser_hardness = hardness.clamp(ERASER_HARDNESS_MIN, ERASER_HARDNESS_MAX);
    }

    /// The ink a stroke actually lands, with the brush's flow folded into the alpha. The one
    /// place that happens, so the GPU preview and the committed pixels cannot disagree about
    /// how translucent a marker is.
    pub fn stroke_ink(&self) -> [u8; 4] {
        let mut ink = self.ink_rgba();
        let flow = self.active_brush_profile().flow;
        ink[3] = ((ink[3] as f32) * flow).round().clamp(0.0, 255.0) as u8;
        ink
    }

    /// Whether the in-progress stroke is ink going onto a raster layer — the case that has to
    /// preview through the coverage pass so overlapping segments do not compound. A vector pen
    /// stroke and a lasso are outlines, not ink, and draw straight.
    pub fn previews_brush_stroke(&self) -> bool {
        if self.stroke_points.is_empty() {
            return false;
        }
        match self.tool {
            Tool::Pen => !self.effective_vector_mode(),
            Tool::Eraser => true,
            _ => false,
        }
    }

    /// Shared bookkeeping for every live-committing brush (blur, clone, heal): batch whatever
    /// part of the stroke has not been applied yet, snapshot whichever of the tiles it touches
    /// the stroke has not already snapshotted, and hand the caller the fresh stamps to paint.
    ///
    /// Unlike every other stamp tool these run *during* the drag rather than at pointer-up:
    /// none of the three has an ink preview, so committing as it goes is what makes the brush
    /// visible while you use it. `live_stamp_progress` is the boundary — `stroke_stamps` only
    /// ever appends as points arrive, so a stamp already applied is never re-applied and the
    /// spacing phase along the polyline stays the same as the pen's.
    ///
    /// The tiles the whole stroke touches accumulate into one `stroke_before` snapshot, so the
    /// stroke is still a single undo step no matter how many pointer events it spanned. Only
    /// tiles not already in the snapshot are captured — re-snapshotting a tile the stroke has
    /// already touched would record the touched state as the "before".
    ///
    /// Returns the fresh stamps and whether this is the first batch of the stroke — the clone
    /// stamp and the healing brush need the latter to know whether their offset still needs
    /// computing.
    pub(super) fn live_stamp_batch(&mut self, radius: f32) -> Option<(Vec<(f32, f32)>, bool)> {
        if !self.stroke_active || self.tool_blocked(self.tool) {
            return None;
        }
        let all = stroke_stamps(&self.stroke_points, radius);
        let fresh = all
            .get(self.live_stamp_progress..)
            .filter(|s| !s.is_empty())?;
        let stamps: Vec<(f32, f32)> = fresh.iter().map(|p| (p.x, p.y)).collect();
        let is_first = self.live_stamp_progress == 0;
        self.live_stamp_progress = all.len();

        let span = stamps_bounds(fresh, radius).and_then(|r| r.intersect(self.bounds()))?;
        let mut touched = TileSet::default();
        tiles_covering(span, &mut touched);

        let active = self.active_layer;
        let grid = self.layers.get(active).and_then(Layer::tiles)?;
        let unseen: Vec<TileCoord> = touched
            .into_iter()
            .filter(|c| grid.tile_in_bounds(*c) && !self.stroke_before.contains_key(c))
            .collect();
        self.stroke_before.extend(grid.snapshot_tiles(&unseen));
        Some((stamps, is_first))
    }

    /// Blur whatever part of the stroke has not been blurred yet, straight into the layer. See
    /// `live_stamp_batch` for the shared half of this.
    pub(super) fn blur_pending_stamps(&mut self) -> bool {
        if self.tool != Tool::Blur {
            return false;
        }
        let strength = self.blur_strength;
        if strength <= 0.0 {
            return false;
        }
        let radius = self.effective_brush_size() * 0.5;
        let Some((stamps, _)) = self.live_stamp_batch(radius) else {
            return false;
        };
        let mut painted_now = false;
        if let Some(tiles) = self
            .layers
            .get_mut(self.active_layer)
            .and_then(|l| l.tiles_mut())
        {
            let touched = crate::blur::blur_stamps(
                tiles,
                &stamps,
                radius,
                strength,
                self.stroke_selection.as_ref(),
            );
            painted_now = touched > 0;
            self.live_stamp_painted |= painted_now;
        }
        painted_now
    }

    /// The source offset a clone/heal batch reads through, computing it from `anchor` on the
    /// first batch of a stroke that does not already carry one — see `CloneSource` and
    /// `begin_stroke`'s `clone_aligned` handling for when that is.
    pub(super) fn clone_offset(
        &mut self,
        is_first: bool,
        first_stamp: (f32, f32),
    ) -> Option<(i32, i32)> {
        let source = self.clone_source.as_mut()?;
        if is_first && source.offset.is_none() {
            source.offset = Some((
                (source.anchor.0 - first_stamp.0).round(),
                (source.anchor.1 - first_stamp.1).round(),
            ));
        }
        source.offset.map(|(ox, oy)| (ox as i32, oy as i32))
    }

    /// Clone- or heal-stamp whatever part of the stroke has not been stamped yet, reading
    /// through the stroke's source offset. See `live_stamp_batch` for the shared half of this.
    pub(super) fn source_pending_stamps(&mut self, stamp: SourceStampFn) -> bool {
        let radius = self.effective_brush_size() * 0.5;
        let Some((stamps, is_first)) = self.live_stamp_batch(radius) else {
            return false;
        };
        let Some(offset) = self.clone_offset(is_first, stamps[0]) else {
            return false;
        };
        let Some(tiles) = self
            .layers
            .get_mut(self.active_layer)
            .and_then(|l| l.tiles_mut())
        else {
            return false;
        };
        let painted_now = stamp(
            tiles,
            &stamps,
            radius,
            offset,
            self.stroke_selection.as_ref(),
        ) > 0;
        self.live_stamp_painted |= painted_now;
        painted_now
    }

    /// The one place that dispatches a stroke event to whichever live-committing brush is
    /// active. A no-op for every other stroke tool (Pen, Eraser, the lasso), which commit at
    /// pointer-up instead.
    pub(super) fn live_stamp_pending(&mut self) -> bool {
        match self.tool {
            Tool::Blur => self.blur_pending_stamps(),
            Tool::Clone => self.source_pending_stamps(crate::clone::clone_stamps),
            Tool::Heal => self.source_pending_stamps(crate::heal::heal_stamps),
            _ => false,
        }
    }

    /// Close out a live-committing stroke: the pixels are already committed, so all that is
    /// left is turning the accumulated snapshot into one history entry.
    pub(super) fn commit_live_stamp_stroke(&mut self) {
        self.stroke_active = false;
        self.stroke_points.clear();
        self.live_stamp_progress = 0;
        let painted = std::mem::take(&mut self.live_stamp_painted);
        let before = std::mem::take(&mut self.stroke_before);
        if !painted || before.is_empty() {
            return;
        }
        let Some(layer_id) = self.layers.get(self.active_layer).map(|l| l.id.clone()) else {
            return;
        };
        self.history
            .push_layer_tiles(layer_id, before, Some(self.active_layer));
    }

    pub(super) fn commit_stroke(&mut self) {
        if !self.stroke_active {
            return;
        }
        if self.tool_blocked(self.tool) {
            self.stroke_active = false;
            self.stroke_points.clear();
            return;
        }
        if matches!(self.tool, Tool::Blur | Tool::Clone | Tool::Heal) {
            self.live_stamp_pending();
            self.commit_live_stamp_stroke();
            return;
        }
        self.stroke_active = false;
        let points = std::mem::take(&mut self.stroke_points);
        if points.is_empty() {
            return;
        }
        if self.effective_vector_mode() && self.tool == Tool::Pen {
            let pts: Vec<(f32, f32)> = points.iter().map(|p| (p.x, p.y)).collect();
            if let Some(item) = vector::item_from_points(&pts, self.ink_rgba(), self.brush_size) {
                self.push_vector_item(item);
            }
            return;
        }
        let erasing = self.tool == Tool::Eraser;
        let profile = self.active_brush_profile();
        let ink = if erasing {
            [0, 0, 0, ALPHA_OPAQUE]
        } else {
            self.stroke_ink()
        };
        let active = self.active_layer;

        // The stroke was aimed at document coordinates; a layer holds its pixels in its own
        // grid, and the renderer maps that grid into the document through the layer's transform.
        // Everything from here down is in *grid* space — the points, the radius, and the area
        // the coverage may cover. For an untransformed layer the two are the same thing; for a
        // moved or scaled one this is the difference between the stroke staying where it was
        // drawn and jumping the moment the GPU preview hands over to the commit.
        let Some(layer) = self.layers.get(active) else {
            return;
        };
        let layer_id = layer.id.clone();
        // Captured before this stroke touches a pixel: if painting or erasing retightens
        // `content_bounds()` enough to move a transformed layer's pivot, the pixels have to be
        // repivoted about it below or the layer visibly jumps despite nothing having dragged,
        // scaled or rotated it — see `LayerTransform::repivoted`.
        let old_transform = layer.transform.filter(|t| !t.is_identity());
        let old_bounds = layer.content_bounds();
        let radius = layer.doc_length_to_grid(self.effective_brush_size() * 0.5);
        let points: Vec<(f32, f32)> = points
            .iter()
            .map(|p| layer.doc_point_to_grid((p.x, p.y)))
            .collect();
        if layer.tiles().is_none() {
            return;
        }

        // A moved or scaled layer can reach grid-space coordinates its storage has never held —
        // the same situation an oversized paste puts a layer in on purpose. Grow the extent to
        // meet the stroke first, or `CoverageGrid::add_segment` and `TileGrid::tile_in_bounds`
        // clip it away below exactly as if it had been painted past the paper with nothing to
        // catch it: the part of the layer the transform moved into new territory just never
        // takes paint.
        if let Some(reach) = points_bounds(&points, radius) {
            if let Some(tiles) = self.layers.get_mut(active).and_then(|l| l.tiles_mut()) {
                tiles.grow_extent(reach);
            }
        }
        let Some(layer) = self.layers.get(active) else {
            return;
        };
        let Some(grid) = layer.tiles() else {
            return;
        };
        // Bounded by what the grid may hold, not by the paper: a pasted image reaches past the
        // document, and a stroke on the part hanging off it is still a stroke on the layer.
        let area = grid.extent();

        let mut coverage = CoverageGrid::new(area);
        match points.len() {
            0 => return,
            1 => {
                coverage.add_segment(points[0], points[0], radius, &profile);
            }
            _ => {
                for pair in points.windows(2) {
                    coverage.add_segment(pair[0], pair[1], radius, &profile);
                }
            }
        }
        if coverage.is_empty() {
            return;
        }

        let touched: Vec<TileCoord> = coverage
            .tile_coords()
            .filter(|c| grid.tile_in_bounds(*c))
            .collect();
        if touched.is_empty() {
            return;
        }
        self.stroke_before = grid.snapshot_tiles(&touched);

        let mut painted = false;
        if let Some(tiles) = self.layers.get_mut(active).and_then(|l| l.tiles_mut()) {
            painted = coverage.paint_into(tiles, ink, erasing, self.selection.as_ref()) > 0;
        }

        if !painted {
            self.stroke_before.clear();
            return;
        }

        let mut repivoted_from = None;
        if let (Some(old_t), Some(old_bounds)) = (old_transform, old_bounds) {
            if let Some(layer) = self.layers.get_mut(active) {
                if let Some(new_bounds) = layer.content_bounds() {
                    let old_pivot = bounds_center(old_bounds);
                    let new_pivot = bounds_center(new_bounds);
                    if old_pivot != new_pivot {
                        layer.transform = Some(old_t.repivoted(old_pivot, new_pivot));
                        repivoted_from = Some(old_t);
                    }
                }
            }
        }

        let before = std::mem::take(&mut self.stroke_before);
        match repivoted_from {
            Some(old_t) => {
                self.history
                    .push_layer_tiles_and_transform(layer_id, before, old_t, Some(active))
            }
            None => self
                .history
                .push_layer_tiles(layer_id, before, Some(active)),
        }
    }
}
