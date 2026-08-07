use crate::limits::{
    FIT_PADDING, MAX_ZOOM_HARD, MAX_ZOOM_IN_FACTOR, MIN_VISIBLE_DOC_SIDE, MIN_ZOOM_FILL,
    VIEWPORT_CULL_PADDING_PX,
};
use crate::tile::DocRect;

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
        by_factor
            .min(by_detail)
            .min(MAX_ZOOM_HARD)
            .max(min)
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

    pub fn visible_doc_rect(&self, doc_width: f32, doc_height: f32) -> Option<DocRect> {
        if self.viewport_width <= 0.0 || self.viewport_height <= 0.0 {
            return None;
        }
        let (min_x, min_y) = self.to_doc(0.0, 0.0);
        let (max_x, max_y) = self.to_doc(self.viewport_width, self.viewport_height);
        let pad = VIEWPORT_CULL_PADDING_PX;
        let visible = DocRect::from_floats(min_x - pad, min_y - pad, max_x + pad, max_y + pad);
        let board = DocRect::from_floats(0.0, 0.0, doc_width - 1.0, doc_height - 1.0);
        visible.intersect(board)
    }

    pub fn clamp_to_board(&mut self, doc_width: f32, doc_height: f32) {
        let min = self.min_zoom(doc_width, doc_height);
        let max = self.max_zoom(doc_width, doc_height);
        self.zoom = self.zoom.clamp(min, max);

        let paper_width = doc_width * self.zoom;
        let paper_height = doc_height * self.zoom;

        self.pan_x = if paper_width <= self.viewport_width {
            (self.viewport_width - paper_width) * 0.5
        } else {
            self.pan_x.clamp(self.viewport_width - paper_width, 0.0)
        };

        self.pan_y = if paper_height <= self.viewport_height {
            (self.viewport_height - paper_height) * 0.5
        } else {
            self.pan_y.clamp(self.viewport_height - paper_height, 0.0)
        };
    }

    pub fn fit(&mut self, doc_width: f32, doc_height: f32) {
        self.zoom = self.fit_zoom(doc_width, doc_height);
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

    pub fn device_size(&self) -> (u32, u32) {
        (
            ((self.viewport_width * self.dpr).round() as u32).max(1),
            ((self.viewport_height * self.dpr).round() as u32).max(1),
        )
    }

    pub fn device_zoom(&self) -> f32 {
        self.zoom * self.dpr
    }

    pub fn view_proj(&self) -> [[f32; 4]; 4] {
        let (device_width, device_height) = self.device_size();
        let width = device_width as f32;
        let height = device_height as f32;
        let zoom = self.device_zoom();
        let pan_x = self.pan_x * self.dpr;
        let pan_y = self.pan_y * self.dpr;
        [
            [2.0 * zoom / width, 0.0, 0.0, 0.0],
            [0.0, -2.0 * zoom / height, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [
                2.0 * pan_x / width - 1.0,
                1.0 - 2.0 * pan_y / height,
                0.0,
                1.0,
            ],
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam(vw: f32, vh: f32) -> Camera {
        Camera {
            viewport_width: vw,
            viewport_height: vh,
            dpr: 2.0,
            ..Default::default()
        }
    }

    #[test]
    fn fit_zoom_is_above_min_floor() {
        let mut c = cam(800.0, 600.0);
        c.fit(1920.0, 1080.0);
        let floor = c.min_zoom(1920.0, 1080.0);
        let fit = c.fit_zoom(1920.0, 1080.0);
        assert!(fit > floor);
        assert!((c.zoom - fit).abs() < 1e-5);
        c.zoom = floor * 0.5;
        c.clamp_to_board(1920.0, 1080.0);
        assert!((c.zoom - floor).abs() < 1e-5);
    }

    #[test]
    fn min_zoom_fills_half_viewport() {
        let c = cam(1000.0, 800.0);
        let z = c.min_zoom(2000.0, 1000.0);
        let paper_w = 2000.0 * z;
        let paper_h = 1000.0 * z;
        let fill_w = paper_w / 1000.0;
        let fill_h = paper_h / 800.0;
        assert!((fill_w.max(fill_h) - MIN_ZOOM_FILL).abs() < 1e-4);
    }

    #[test]
    fn max_zoom_is_ten_times_min_when_detail_allows() {
        let c = cam(2000.0, 2000.0);
        let min = c.min_zoom(3000.0, 3000.0);
        let max = c.max_zoom(3000.0, 3000.0);
        assert!((max - min * MAX_ZOOM_IN_FACTOR).abs() < 1e-3);
    }

    #[test]
    fn max_zoom_respects_visible_doc_side() {
        let c = cam(800.0, 600.0);
        let max = c.max_zoom(4000.0, 4000.0);
        let visible = 600.0 / max;
        assert!(visible + 1e-3 >= MIN_VISIBLE_DOC_SIDE.min(4000.0));
    }

    #[test]
    fn screen_doc_round_trip() {
        let mut c = cam(1000.0, 800.0);
        c.fit(2000.0, 1500.0);
        c.zoom_at(400.0, 300.0, c.zoom * 2.0, 2000.0, 1500.0);
        let (dx, dy) = c.to_doc(412.0, 288.0);
        let (sx, sy) = c.to_screen(dx, dy);
        assert!((sx - 412.0).abs() < 1e-3);
        assert!((sy - 288.0).abs() < 1e-3);
    }

    #[test]
    fn visible_rect_covers_whole_board_when_fitted() {
        let mut c = cam(1000.0, 800.0);
        c.fit(2000.0, 1500.0);
        let r = c.visible_doc_rect(2000.0, 1500.0).unwrap();
        assert_eq!(r, DocRect::new(0, 0, 1999, 1499));
    }

    #[test]
    fn visible_rect_shrinks_when_zoomed_in() {
        let mut c = cam(1000.0, 800.0);
        c.fit(4000.0, 4000.0);
        let max = c.max_zoom(4000.0, 4000.0);
        c.zoom_at(500.0, 400.0, max, 4000.0, 4000.0);
        let r = c.visible_doc_rect(4000.0, 4000.0).unwrap();
        let width = (r.max_x - r.min_x + 1) as f32;
        let height = (r.max_y - r.min_y + 1) as f32;
        assert!(width < 4000.0 / 2.0, "width was {width}");
        assert!(height < 4000.0 / 2.0, "height was {height}");
    }

    #[test]
    fn visible_rect_is_none_without_viewport() {
        let c = Camera::default();
        assert!(c.visible_doc_rect(100.0, 100.0).is_none());
    }

    #[test]
    fn clamp_centers_when_paper_fits() {
        let mut c = cam(1000.0, 800.0);
        c.fit(100.0, 100.0);
        let pw = 100.0 * c.zoom;
        let ph = 100.0 * c.zoom;
        assert!((c.pan_x - (1000.0 - pw) * 0.5).abs() < 1e-3);
        assert!((c.pan_y - (800.0 - ph) * 0.5).abs() < 1e-3);
    }
}
