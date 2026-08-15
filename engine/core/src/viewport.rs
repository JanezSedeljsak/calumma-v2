use crate::camera::Camera;
use crate::limits::VIEWPORT_CULL_PADDING_PX;
use crate::tile::DocRect;

impl Camera {
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

    pub fn device_size(&self) -> (u32, u32) {
        (
            ((self.viewport_width * self.dpr).round() as u32).max(1),
            ((self.viewport_height * self.dpr).round() as u32).max(1),
        )
    }

    pub fn device_zoom(&self) -> f32 {
        self.zoom * self.dpr
    }

    pub fn paper_scissor(
        &self,
        doc_width: f32,
        doc_height: f32,
        fb_w: u32,
        fb_h: u32,
    ) -> Option<(u32, u32, u32, u32)> {
        if fb_w == 0 || fb_h == 0 || doc_width <= 0.0 || doc_height <= 0.0 {
            return None;
        }
        let dpr = self.dpr.max(1e-6);
        let x0 = self.pan_x * dpr;
        let y0 = self.pan_y * dpr;
        let x1 = (doc_width * self.zoom + self.pan_x) * dpr;
        let y1 = (doc_height * self.zoom + self.pan_y) * dpr;
        let left = x0.max(0.0).floor();
        let top = y0.max(0.0).floor();
        let right = x1.min(fb_w as f32).ceil();
        let bottom = y1.min(fb_h as f32).ceil();
        if right <= left || bottom <= top {
            return None;
        }
        let x = left as u32;
        let y = top as u32;
        let x2 = (right as u32).min(fb_w);
        let y2 = (bottom as u32).min(fb_h);
        if x2 <= x || y2 <= y {
            return None;
        }
        Some((x, y, x2 - x, y2 - y))
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
