use calumma_core::Camera;
use proptest::prelude::*;

proptest! {
    #[test]
    fn round_trip_screen_doc(
        vw in 200.0f32..4000.0,
        vh in 200.0f32..4000.0,
        dw in 100.0f32..8000.0,
        dh in 100.0f32..8000.0,
        sx in 0.0f32..2000.0,
        sy in 0.0f32..2000.0,
        zoom_factor in 1.0f32..8.0,
    ) {
        let mut cam = Camera {
            viewport_width: vw,
            viewport_height: vh,
            dpr: 2.0,
            ..Default::default()
        };
        cam.fit(dw, dh);
        let max = cam.max_zoom(dw, dh);
        let z = (cam.zoom * zoom_factor).min(max);
        cam.zoom_at(sx.min(vw), sy.min(vh), z, dw, dh);
        let sx = sx.min(vw);
        let sy = sy.min(vh);
        let (dx, dy) = cam.to_doc(sx, sy);
        let (rx, ry) = cam.to_screen(dx, dy);
        prop_assert!((rx - sx).abs() < 0.05);
        prop_assert!((ry - sy).abs() < 0.05);
    }
}
