//! Changing the canvas itself: resizing it and shifting its origin, which Crop commits through.

use super::*;

/// The up-to-4 rectangles covering `outer \ inner` — top, bottom, left and right bands, in that
/// order, `None` for a band that is empty. `inner` must already be a sub-rect of `outer` (or
/// absent, in which case the whole of `outer` is exposed). Paper's fill is painted onto only
/// the region a canvas shift newly exposed, leaving whatever survived from the old canvas alone.
pub(super) fn exposed_bands(outer: DocRect, inner: Option<DocRect>) -> [Option<DocRect>; 4] {
    let Some(inner) = inner else {
        return [Some(outer), None, None, None];
    };
    [
        (inner.min_y > outer.min_y)
            .then(|| DocRect::new(outer.min_x, outer.min_y, outer.max_x, inner.min_y - 1)),
        (inner.max_y < outer.max_y)
            .then(|| DocRect::new(outer.min_x, inner.max_y + 1, outer.max_x, outer.max_y)),
        (inner.min_x > outer.min_x)
            .then(|| DocRect::new(outer.min_x, inner.min_y, inner.min_x - 1, inner.max_y)),
        (inner.max_x < outer.max_x)
            .then(|| DocRect::new(inner.max_x + 1, inner.min_y, outer.max_x, inner.max_y)),
    ]
}

/// `outer \ inner` as up to four non-overlapping bands. Cropping clears these rather than
/// rewriting the whole layer, so the cost is the discarded margin and not the picture.
pub(crate) fn outside_bands(outer: DocRect, inner: DocRect) -> Vec<DocRect> {
    let Some(inner) = outer.intersect(inner) else {
        return vec![outer];
    };
    let mut out = Vec::with_capacity(4);
    if inner.min_y > outer.min_y {
        out.push(DocRect::new(
            outer.min_x,
            outer.min_y,
            outer.max_x,
            inner.min_y - 1,
        ));
    }
    if inner.max_y < outer.max_y {
        out.push(DocRect::new(
            outer.min_x,
            inner.max_y + 1,
            outer.max_x,
            outer.max_y,
        ));
    }
    if inner.min_x > outer.min_x {
        out.push(DocRect::new(
            outer.min_x,
            inner.min_y,
            inner.min_x - 1,
            inner.max_y,
        ));
    }
    if inner.max_x < outer.max_x {
        out.push(DocRect::new(
            inner.max_x + 1,
            inner.min_y,
            outer.max_x,
            inner.max_y,
        ));
    }
    out
}

impl Document {
    pub fn resize(&mut self, new_width: u32, new_height: u32) {
        let new_width = new_width.clamp(MIN_CANVAS_SIDE, MAX_CANVAS_SIDE);
        let new_height = new_height.clamp(MIN_CANVAS_SIDE, MAX_CANVAS_SIDE);
        let (old_width, old_height) = (self.width, self.height);
        if new_width == old_width && new_height == old_height {
            return;
        }
        self.commit_text();
        self.record_stack_history();
        for layer in &mut self.layers {
            let is_paper = layer.is_paper();
            let Some(tiles) = layer.tiles_mut() else {
                continue;
            };
            tiles.set_size(new_width, new_height);
            if !is_paper {
                continue;
            }
            if new_width > old_width {
                tiles.fill_uniform(
                    DocRect::new(
                        old_width as i32,
                        0,
                        new_width as i32 - 1,
                        new_height as i32 - 1,
                    ),
                    PAPER_WHITE,
                );
            }
            if new_height > old_height {
                tiles.fill_uniform(
                    DocRect::new(
                        0,
                        old_height as i32,
                        old_width as i32 - 1,
                        new_height as i32 - 1,
                    ),
                    PAPER_WHITE,
                );
            }
        }
        self.width = new_width;
        self.height = new_height;
        self.fit_to_view();
    }

    /// `resize`'s general form: the new canvas window need not share the old one's top-left
    /// corner. `apply_canvas_shift(0, 0, w, h)` is exactly `resize(w, h)`.
    ///
    /// A nonzero origin never resamples a pixel: every layer's content moves by shifting
    /// `layer.transform.offset`, which composes with any existing transform exactly (a pure
    /// post-hoc translation cancels the pivot term in `LayerTransform::forward` regardless of
    /// rotation/scale), so tile data is never touched and nothing painted is ever lost — the
    /// same non-destructive guarantee `resize` already has, generalized to any edge or corner.
    pub fn apply_canvas_shift(
        &mut self,
        origin_x: i32,
        origin_y: i32,
        new_width: u32,
        new_height: u32,
    ) {
        if origin_x == 0 && origin_y == 0 {
            self.resize(new_width, new_height);
            return;
        }
        let new_width = new_width.clamp(MIN_CANVAS_SIDE, MAX_CANVAS_SIDE);
        let new_height = new_height.clamp(MIN_CANVAS_SIDE, MAX_CANVAS_SIDE);
        let (old_width, old_height) = (self.width, self.height);
        self.commit_text();
        self.record_stack_history();
        // The old canvas window, and the new one, both expressed in the *current* (pre-shift)
        // local coordinate space every layer's tiles already live in — nothing has moved yet at
        // this point, so this is also every layer's own local space, paper included.
        let old_local = DocRect::from_size(old_width, old_height);
        let new_window = DocRect::new(
            origin_x,
            origin_y,
            origin_x + new_width as i32 - 1,
            origin_y + new_height as i32 - 1,
        );
        for layer in &mut self.layers {
            let t = layer.transform.get_or_insert_with(LayerTransform::default);
            t.offset_x -= origin_x as f32;
            t.offset_y -= origin_y as f32;
            let is_paper = layer.is_paper();
            let Some(tiles) = layer.tiles_mut() else {
                continue;
            };
            tiles.set_size(new_width, new_height);
            // `set_size` only ever unions the extent with `[0, new_width) x [0, new_height)`,
            // which is the wrong place once this layer carries an offset — the local window a
            // future paint or fill needs room for is `new_window`, not the document's own frame.
            tiles.grow_extent(new_window);
            if !is_paper {
                continue;
            }
            let surviving = new_window.intersect(old_local);
            for band in exposed_bands(new_window, surviving).into_iter().flatten() {
                tiles.fill_uniform(band, PAPER_WHITE);
            }
        }
        self.width = new_width;
        self.height = new_height;
        self.fit_to_view();
    }
}
