use super::{
    brush_params, overlay_rect_params, push_outlined_segment, rgba_unit, StrokeInstance,
    OVERLAY_BORDER_COLOR, OVERLAY_BORDER_PX,
};
use calumma_core::{BrushProfile, Document};

const CROP_OUTLINE_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const CROP_OUTLINE_WIDTH_PX: f32 = 1.0;
const CROP_HANDLE_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const CROP_HANDLE_RADIUS_PX: f32 = 4.0;
const CROP_HANDLE_BORDER_COLOR: [f32; 4] = OVERLAY_BORDER_COLOR;
const CROP_HANDLE_BORDER_PX: f32 = OVERLAY_BORDER_PX;
const CROP_GUIDE_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const CROP_GUIDE_WIDTH_PX: f32 = 1.0;

fn crop_shade_box(
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    zoom: f32,
    color: [f32; 4],
) -> Option<StrokeInstance> {
    let w = (x1 - x0).abs();
    let h = (y1 - y0).abs();
    if w < 0.5 || h < 0.5 {
        return None;
    }
    let (x0, x1) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
    let (y0, y1) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
    let mid_y = (y0 + y1) * 0.5;
    Some(StrokeInstance {
        segment: [x0, mid_y, x1, mid_y],
        color,
        brush: overlay_rect_params((h * 0.5 * zoom.max(1e-6)).max(0.5)),
    })
}

fn crop_shade_instances(doc: &Document, crop: (f32, f32, f32, f32)) -> Vec<StrokeInstance> {
    let (cx0, cy0, cx1, cy1) = crop;
    let (cx0, cx1) = if cx0 <= cx1 { (cx0, cx1) } else { (cx1, cx0) };
    let (cy0, cy1) = if cy0 <= cy1 { (cy0, cy1) } else { (cy1, cy0) };
    let pw = doc.width as f32;
    let ph = doc.height as f32;
    let left = cx0.clamp(0.0, pw);
    let right = cx1.clamp(0.0, pw);
    let top = cy0.clamp(0.0, ph);
    let bottom = cy1.clamp(0.0, ph);
    let zoom = doc.camera.zoom;
    let mut color = rgba_unit(doc.board_colors.desk);
    color[3] = 1.0;
    [
        crop_shade_box(0.0, 0.0, pw, top, zoom, color),
        crop_shade_box(0.0, bottom, pw, ph, zoom, color),
        crop_shade_box(0.0, top, left, bottom, zoom, color),
        crop_shade_box(right, top, pw, bottom, zoom, color),
    ]
    .into_iter()
    .flatten()
    .collect()
}

pub fn crop_overlay_instances(doc: &Document) -> Vec<StrokeInstance> {
    let Some((x0, y0, x1, y1)) = doc.crop_overlay_rect() else {
        return Vec::new();
    };
    let guides = doc.crop_overlay_lines();
    let shade = crop_shade_instances(doc, (x0, y0, x1, y1));
    let mut out = Vec::with_capacity(shade.len() + guides.len() * 2 + 4 * 2 + 8 * 2);
    out.extend(shade);
    for (a, b) in guides {
        push_outlined_segment(&mut out, a, b, CROP_GUIDE_COLOR, CROP_GUIDE_WIDTH_PX);
    }
    let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    for i in 0..4 {
        push_outlined_segment(
            &mut out,
            corners[i],
            corners[(i + 1) % 4],
            CROP_OUTLINE_COLOR,
            CROP_OUTLINE_WIDTH_PX,
        );
    }
    let (mx, my) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
    let handles = [
        corners[0],
        (mx, y0),
        corners[1],
        (x1, my),
        corners[2],
        (mx, y1),
        corners[3],
        (x0, my),
    ];
    for p in handles {
        out.push(StrokeInstance {
            segment: [p.0, p.1, p.0, p.1],
            color: CROP_HANDLE_BORDER_COLOR,
            brush: brush_params(
                CROP_HANDLE_RADIUS_PX + CROP_HANDLE_BORDER_PX,
                &BrushProfile::HARD,
            ),
        });
        out.push(StrokeInstance {
            segment: [p.0, p.1, p.0, p.1],
            color: CROP_HANDLE_COLOR,
            brush: brush_params(CROP_HANDLE_RADIUS_PX, &BrushProfile::HARD),
        });
    }
    out
}
