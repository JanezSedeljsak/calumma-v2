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
    fn device_size_scales_by_dpr() {
        let c = cam(800.0, 600.0);
        assert_eq!(c.device_size(), (1600, 1200));
    }

    #[test]
    fn view_proj_maps_pan_origin_to_top_left() {
        let mut c = cam(800.0, 600.0);
        c.fit(400.0, 300.0);
        let m = c.view_proj();
        let x = m[3][0];
        let y = m[3][1];
        assert!((-1.0..=1.0).contains(&x));
        assert!((-1.0..=1.0).contains(&y));
    }
}
