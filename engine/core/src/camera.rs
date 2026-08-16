use crate::limits::{
    FIT_PADDING, MAX_ZOOM_HARD, MAX_ZOOM_IN_FACTOR, MIN_VISIBLE_DOC_SIDE, MIN_ZOOM_FILL,
    PAN_KEEP_VISIBLE, SCROLL_LINE_PIXELS, SCROLL_PAN_MAX_GAIN, ZOOM_PER_SCROLL_LINE,
    ZOOM_PER_SCROLL_PIXEL, ZOOM_STEP,
};

/// Scroll deltas reach the camera in whatever unit the input device speaks: pixels from a
/// trackpad, lines from a wheel. Everything below works in pixels.
fn scroll_pixels(delta: f32, precise: bool) -> f32 {
    if precise {
        delta
    } else {
        delta * SCROLL_LINE_PIXELS
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub dpr: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            viewport_width: 0.0,
            viewport_height: 0.0,
            dpr: 1.0,
        }
    }
}

impl Camera {
    fn fill_zoom(&self, doc_width: f32, doc_height: f32, fill: f32) -> f32 {
        if doc_width <= 0.0
            || doc_height <= 0.0
            || self.viewport_width <= 0.0
            || self.viewport_height <= 0.0
        {
            return 1.0;
        }
        let by_width = self.viewport_width * fill / doc_width;
        let by_height = self.viewport_height * fill / doc_height;
        by_width
            .min(by_height)
            .clamp(f32::MIN_POSITIVE, MAX_ZOOM_HARD)
    }

    pub fn fit_zoom(&self, doc_width: f32, doc_height: f32) -> f32 {
        self.fill_zoom(doc_width, doc_height, FIT_PADDING)
    }

    pub fn min_zoom(&self, doc_width: f32, doc_height: f32) -> f32 {
        self.fill_zoom(doc_width, doc_height, MIN_ZOOM_FILL)
    }

    pub fn max_zoom(&self, doc_width: f32, doc_height: f32) -> f32 {
        let min = self.min_zoom(doc_width, doc_height);
        let by_factor = min * MAX_ZOOM_IN_FACTOR;
        let shorter_view = self.viewport_width.min(self.viewport_height).max(1.0);
        let shorter_doc = doc_width.min(doc_height).max(1.0);
        let visible_side = MIN_VISIBLE_DOC_SIDE.min(shorter_doc);
        let by_detail = shorter_view / visible_side;
        by_factor.min(by_detail).min(MAX_ZOOM_HARD).max(min)
    }

    pub fn to_doc(&self, screen_x: f32, screen_y: f32) -> (f32, f32) {
        let zoom = self.zoom.max(f32::MIN_POSITIVE);
        (
            (screen_x - self.pan_x) / zoom,
            (screen_y - self.pan_y) / zoom,
        )
    }

    pub fn to_screen(&self, doc_x: f32, doc_y: f32) -> (f32, f32) {
        (
            doc_x * self.zoom + self.pan_x,
            doc_y * self.zoom + self.pan_y,
        )
    }

    /// Pan range that keeps `PAN_KEEP_VISIBLE` of the paper on screen, as
    /// `(min_x, max_x, min_y, max_y)`. The window is always non-empty, so a fitted
    /// paper is still draggable instead of being pinned to the centre.
    pub fn pan_bounds(&self, doc_width: f32, doc_height: f32) -> (f32, f32, f32, f32) {
        let paper_width = doc_width * self.zoom;
        let paper_height = doc_height * self.zoom;
        let keep_x = paper_width.min(self.viewport_width) * PAN_KEEP_VISIBLE;
        let keep_y = paper_height.min(self.viewport_height) * PAN_KEEP_VISIBLE;
        (
            keep_x - paper_width,
            self.viewport_width - keep_x,
            keep_y - paper_height,
            self.viewport_height - keep_y,
        )
    }

    pub fn center(&mut self, doc_width: f32, doc_height: f32) {
        self.pan_x = (self.viewport_width - doc_width * self.zoom) * 0.5;
        self.pan_y = (self.viewport_height - doc_height * self.zoom) * 0.5;
    }

    pub fn clamp_to_board(&mut self, doc_width: f32, doc_height: f32) {
        let min = self.min_zoom(doc_width, doc_height);
        let max = self.max_zoom(doc_width, doc_height);
        self.zoom = self.zoom.clamp(min, max);

        let (min_x, max_x, min_y, max_y) = self.pan_bounds(doc_width, doc_height);
        self.pan_x = self.pan_x.clamp(min_x, max_x);
        self.pan_y = self.pan_y.clamp(min_y, max_y);
    }

    pub fn fit(&mut self, doc_width: f32, doc_height: f32) {
        self.zoom = self.fit_zoom(doc_width, doc_height);
        self.center(doc_width, doc_height);
        self.clamp_to_board(doc_width, doc_height);
    }

    pub fn zoom_at(&mut self, screen_x: f32, screen_y: f32, zoom: f32, doc_w: f32, doc_h: f32) {
        let next = zoom.clamp(self.min_zoom(doc_w, doc_h), self.max_zoom(doc_w, doc_h));
        let (doc_x, doc_y) = self.to_doc(screen_x, screen_y);
        self.zoom = next;
        self.pan_x = screen_x - doc_x * next;
        self.pan_y = screen_y - doc_y * next;
        self.clamp_to_board(doc_w, doc_h);
    }

    pub fn zoom_to_center(&mut self, zoom: f32, doc_width: f32, doc_height: f32) {
        self.zoom_at(
            self.viewport_width * 0.5,
            self.viewport_height * 0.5,
            zoom,
            doc_width,
            doc_height,
        );
    }

    pub fn pan_by(&mut self, dx: f32, dy: f32, doc_width: f32, doc_height: f32) {
        self.pan_x += dx;
        self.pan_y += dy;
        self.clamp_to_board(doc_width, doc_height);
    }

    /// Speed-up applied to scroll-wheel panning as the board zooms out. A pointer drag
    /// tracks the cursor one-for-one and needs no gain; a scroll notch is a fixed pixel
    /// amount, so the same notch covers proportionally less board the further out you go.
    /// Anchored at `fit_zoom` (gain 1 at Fit) and never less than 1.
    pub fn scroll_pan_gain(&self, doc_width: f32, doc_height: f32) -> f32 {
        let zoom = self.zoom.max(f32::MIN_POSITIVE);
        (self.fit_zoom(doc_width, doc_height) / zoom).clamp(1.0, SCROLL_PAN_MAX_GAIN)
    }

    pub fn pan_by_scroll(
        &mut self,
        dx: f32,
        dy: f32,
        precise: bool,
        doc_width: f32,
        doc_height: f32,
    ) {
        let gain = self.scroll_pan_gain(doc_width, doc_height);
        self.pan_by(
            scroll_pixels(dx, precise) * gain,
            scroll_pixels(dy, precise) * gain,
            doc_width,
            doc_height,
        );
    }

    /// Scroll-wheel zoom, anchored under the pointer. Exponential in the delta, so a notch
    /// multiplies the zoom by a constant factor wherever you already are on the curve.
    /// A positive delta (content pulled down) zooms out, matching the pan direction.
    pub fn zoom_by_scroll(
        &mut self,
        screen_x: f32,
        screen_y: f32,
        delta: f32,
        precise: bool,
        doc_width: f32,
        doc_height: f32,
    ) {
        let weight = if precise {
            ZOOM_PER_SCROLL_PIXEL
        } else {
            ZOOM_PER_SCROLL_LINE
        };
        let step = delta * weight;
        if step == 0.0 {
            return;
        }
        self.zoom_at(
            screen_x,
            screen_y,
            self.zoom * (-step).exp(),
            doc_width,
            doc_height,
        );
    }

    pub fn step_zoom(&mut self, zoom_in: bool, doc_width: f32, doc_height: f32) {
        let factor = if zoom_in { ZOOM_STEP } else { 1.0 / ZOOM_STEP };
        self.zoom_to_center(self.zoom * factor, doc_width, doc_height);
    }

    pub fn zoom_unit(&self, doc_width: f32, doc_height: f32) -> f32 {
        let min = self.min_zoom(doc_width, doc_height);
        let max = self.max_zoom(doc_width, doc_height);
        if max <= min || min <= 0.0 || self.zoom <= 0.0 {
            return 0.0;
        }
        ((self.zoom / min).ln() / (max / min).ln()).clamp(0.0, 1.0)
    }

    pub fn zoom_from_unit(&self, unit: f32, doc_width: f32, doc_height: f32) -> f32 {
        let min = self.min_zoom(doc_width, doc_height);
        let max = self.max_zoom(doc_width, doc_height);
        if max <= min || min <= 0.0 {
            return min;
        }
        min * (max / min).powf(unit.clamp(0.0, 1.0))
    }

    pub fn ruler_ticks_x(&self) -> Vec<crate::ruler::RulerTick> {
        crate::ruler::ruler_ticks(self.zoom, self.pan_x, self.viewport_width)
    }

    pub fn ruler_ticks_y(&self) -> Vec<crate::ruler::RulerTick> {
        crate::ruler::ruler_ticks(self.zoom, self.pan_y, self.viewport_height)
    }
}
