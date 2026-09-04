//! Photoshop clips every paint tool to the active selection. The bucket and the blur brush
//! already did; the pen, the eraser and the shape tools did not — this is the gap that closed
//! (`docs/todo.md`'s "Unplanned gap, found while building the blur brush").

use calumma_core::document::*;
use calumma_core::*;

fn board() -> Document {
    let mut doc = Document::new("p".into(), "t", 256, 256);
    doc.resize_viewport(256.0, 256.0, 1.0);
    doc.fit_to_view();
    doc
}

fn pixel(doc: &Document, x: i32, y: i32) -> [u8; 4] {
    doc.layers[doc.active_layer]
        .tiles()
        .unwrap()
        .get_pixel(x, y)
}

fn top_half_selection() -> Selection {
    Selection {
        shape: SelectionShape::Rect {
            start: (0.0, 0.0),
            end: (256.0, 100.0),
        },
    }
}

fn drag(doc: &mut Document, from: (f32, f32), to: (f32, f32)) {
    let (sx, sy) = doc.camera.to_screen(from.0, from.1);
    let (ex, ey) = doc.camera.to_screen(to.0, to.1);
    doc.pointer_down(sx, sy);
    doc.pointer_move(ex, ey);
    doc.pointer_up(ex, ey);
}

#[test]
fn the_pen_clips_to_the_active_selection() {
    let mut doc = board();
    doc.selection = Some(top_half_selection());
    doc.tool = Tool::Pen;
    doc.set_color([0, 0, 0, 255]);
    doc.brush_size = 40.0;

    drag(&mut doc, (128.0, 50.0), (128.0, 50.0));
    drag(&mut doc, (128.0, 200.0), (128.0, 200.0));

    assert_eq!(
        pixel(&doc, 128, 50)[3],
        255,
        "inside the selection, painted"
    );
    assert_eq!(
        pixel(&doc, 128, 200)[3],
        0,
        "outside it, the selection refused the ink"
    );
}

#[test]
fn the_eraser_clips_to_the_active_selection() {
    let mut doc = board();
    {
        let tiles = doc.layers[doc.active_layer].tiles_mut().unwrap();
        tiles.fill_uniform(DocRect::new(0, 0, 255, 255), [10, 20, 30, 255]);
    }
    doc.selection = Some(top_half_selection());
    doc.tool = Tool::Eraser;
    doc.brush_size = 40.0;

    drag(&mut doc, (128.0, 50.0), (128.0, 50.0));
    drag(&mut doc, (128.0, 200.0), (128.0, 200.0));

    assert_eq!(pixel(&doc, 128, 50), [0, 0, 0, 0], "inside it, erased");
    assert_eq!(
        pixel(&doc, 128, 200),
        [10, 20, 30, 255],
        "outside it, the selection kept the eraser off"
    );
}

/// The bucket and blur clip per pixel, not per stroke — the part of a stroke that crosses out
/// of the selection stops there rather than the whole stroke being refused or accepted whole.
#[test]
fn the_pen_clips_per_pixel_not_per_stroke() {
    let mut doc = board();
    doc.selection = Some(top_half_selection());
    doc.tool = Tool::Pen;
    doc.set_color([0, 0, 0, 255]);
    doc.brush_size = 10.0;

    drag(&mut doc, (128.0, 60.0), (128.0, 140.0));

    assert_eq!(pixel(&doc, 128, 60)[3], 255, "the part inside painted");
    assert_eq!(pixel(&doc, 128, 130)[3], 0, "the part outside did not");
}

#[test]
fn a_rect_shape_clips_to_the_active_selection() {
    let mut doc = board();
    doc.selection = Some(top_half_selection());
    doc.tool = Tool::Rect;
    doc.fill = true;
    doc.set_color([0, 0, 0, 255]);

    // One rectangle spanning both sides of the selection boundary at y = 100.
    drag(&mut doc, (50.0, 50.0), (200.0, 200.0));

    assert_eq!(
        pixel(&doc, 100, 70)[3],
        255,
        "the part of the shape inside the selection painted"
    );
    assert_eq!(
        pixel(&doc, 100, 150)[3],
        0,
        "the part outside it did not, though the shape itself covers that point"
    );
}

/// Painting with no selection at all is unclipped, exactly as before this fix.
#[test]
fn no_selection_means_no_clipping() {
    let mut doc = board();
    doc.tool = Tool::Pen;
    doc.set_color([0, 0, 0, 255]);
    doc.brush_size = 20.0;

    drag(&mut doc, (128.0, 200.0), (128.0, 200.0));

    assert_eq!(pixel(&doc, 128, 200)[3], 255);
}
