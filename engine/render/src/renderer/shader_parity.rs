use super::*;
use calumma_core::limits::PAPER_WHITE;
use calumma_core::{Document, Tool};

const SIDE: u32 = 256;
const FRINGE_DELTA: u8 = 128;

impl Renderer {
    fn read_headless_rgba(&self) -> Option<Vec<u8>> {
        let FrameOutput::Headless(texture) = &self.output else {
            return None;
        };
        let width = self.config.width;
        let height = self.config.height;
        let mut bgra =
            crate::test_gpu::read_texture_2d(&self.device, &self.queue, texture, width, height);
        for px in bgra.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        Some(bgra)
    }
}

fn identity_doc() -> Document {
    let mut doc = Document::new("p".into(), "t", SIDE, SIDE);
    doc.resize_viewport(SIDE as f32, SIDE as f32, 1.0);
    doc.camera.zoom = 1.0;
    doc.camera.pan_x = 0.0;
    doc.camera.pan_y = 0.0;
    doc.ink_opacity = 1.0;
    doc.fill = true;
    doc.stroke = false;
    doc.layers[0].visible = true;
    doc
}

fn draw_shape(doc: &mut Document, tool: Tool, start: (f32, f32), end: (f32, f32), fill: [u8; 4]) {
    doc.tool = tool;
    if tool.takes_fill() {
        doc.shape_fill_color = fill;
        doc.stroke_color = fill;
    } else {
        doc.color = fill;
    }
    doc.pointer_down(start.0, start.1);
    doc.pointer_move(end.0, end.1);
    doc.pointer_up(end.0, end.1);
}

fn pixel(rgba: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * w + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

fn is_aa_fringe(cpu: &[u8], w: u32, h: u32, x: u32, y: u32) -> bool {
    let here = pixel(cpu, w, x, y);
    let x0 = x.saturating_sub(1);
    let y0 = y.saturating_sub(1);
    let x1 = (x + 1).min(w - 1);
    let y1 = (y + 1).min(h - 1);
    for ny in y0..=y1 {
        for nx in x0..=x1 {
            if pixel(cpu, w, nx, ny) != here {
                return true;
            }
        }
    }
    false
}

fn channel_delta(a: u8, b: u8) -> u8 {
    a.abs_diff(b)
}

fn assert_gpu_matches_composite(gpu: &[u8], cpu: &[u8], w: u32, h: u32) {
    assert_eq!(gpu.len(), cpu.len());
    assert_eq!(gpu.len(), (w * h * 4) as usize);
    let mut worst = (0u8, 0u32, 0u32, [0u8; 4], [0u8; 4]);
    let mut solid_mismatch = None;
    for y in 0..h {
        for x in 0..w {
            let g = pixel(gpu, w, x, y);
            let c = pixel(cpu, w, x, y);
            let delta = channel_delta(g[0], c[0])
                .max(channel_delta(g[1], c[1]))
                .max(channel_delta(g[2], c[2]))
                .max(channel_delta(g[3], c[3]));
            if delta > worst.0 {
                worst = (delta, x, y, g, c);
            }
            if delta == 0 {
                continue;
            }
            if is_aa_fringe(cpu, w, h, x, y) {
                assert!(
                    delta <= FRINGE_DELTA,
                    "AA fringe at ({x},{y}) drifted {delta}: gpu={g:?} cpu={c:?}"
                );
                continue;
            }
            solid_mismatch = Some((x, y, g, c, delta));
        }
    }
    if let Some((x, y, g, c, delta)) = solid_mismatch {
        panic!("solid pixel ({x},{y}) gpu={g:?} != cpu={c:?} delta={delta}");
    }
    assert!(
        worst.0 <= FRINGE_DELTA,
        "worst delta {} at ({},{}) gpu={:?} cpu={:?}",
        worst.0,
        worst.1,
        worst.2,
        worst.3,
        worst.4
    );
}

fn render_and_compare(doc: &mut Document) {
    let Some(mut renderer) = Renderer::new_headless(SIDE, SIDE) else {
        return;
    };
    renderer.invalidate();
    renderer.render(doc);
    let gpu = renderer
        .read_headless_rgba()
        .expect("headless frame readback");
    let (cw, ch, cpu) = doc.composite_rgba();
    assert_eq!((cw, ch), (SIDE, SIDE));
    assert_gpu_matches_composite(&gpu, &cpu, cw, ch);
}

#[test]
fn vector_shapes_match_composite_rgba() {
    let mut doc = identity_doc();
    doc.set_vector_mode(true);

    draw_shape(
        &mut doc,
        Tool::Rect,
        (16.0, 16.0),
        (80.0, 80.0),
        [255, 0, 0, 255],
    );
    draw_shape(
        &mut doc,
        Tool::Ellipse,
        (96.0, 16.0),
        (160.0, 80.0),
        [0, 255, 0, 255],
    );
    draw_shape(
        &mut doc,
        Tool::Triangle,
        (176.0, 16.0),
        (240.0, 80.0),
        [0, 0, 255, 255],
    );
    draw_shape(
        &mut doc,
        Tool::Pentagon,
        (16.0, 96.0),
        (80.0, 160.0),
        [255, 0, 255, 255],
    );
    draw_shape(
        &mut doc,
        Tool::Line,
        (96.0, 112.0),
        (160.0, 160.0),
        [0, 255, 255, 255],
    );
    draw_shape(
        &mut doc,
        Tool::Arrow,
        (176.0, 112.0),
        (240.0, 160.0),
        [255, 255, 0, 255],
    );

    assert!(
        doc.layers.iter().filter(|l| l.content.is_vector()).count() >= 6,
        "each shape is its own vector layer"
    );
    render_and_compare(&mut doc);
}

#[test]
fn raster_shapes_match_composite_rgba() {
    let mut doc = identity_doc();
    doc.set_vector_mode(false);
    draw_shape(
        &mut doc,
        Tool::Rect,
        (40.0, 40.0),
        (200.0, 200.0),
        [255, 0, 0, 255],
    );
    draw_shape(
        &mut doc,
        Tool::Ellipse,
        (72.0, 72.0),
        (168.0, 168.0),
        [0, 0, 255, 255],
    );
    assert_eq!(
        doc.layers[doc.active_layer]
            .tiles()
            .expect("paint layer")
            .get_pixel(120, 120)[3],
        255,
        "the raster commit landed"
    );
    render_and_compare(&mut doc);
}

#[test]
fn empty_paper_matches_composite_rgba() {
    let mut doc = identity_doc();
    let (_, _, cpu) = doc.composite_rgba();
    assert_eq!(pixel(&cpu, SIDE, 8, 8), PAPER_WHITE);
    render_and_compare(&mut doc);
}
