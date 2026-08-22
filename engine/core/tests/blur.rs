use calumma_core::document::*;
use calumma_core::*;

/// A document with one paintable layer above Paper, and a hard black/white edge running down
/// the middle of it — the thing a blur brush exists to soften.
fn edged_board() -> Document {
    let mut doc = Document::new("p".into(), "t", 256, 256);
    doc.resize_viewport(256.0, 256.0, 1.0);
    doc.fit_to_view();
    let tiles = doc.layers[doc.active_layer].tiles_mut().unwrap();
    tiles.fill_uniform(DocRect::new(0, 0, 127, 255), [0, 0, 0, 255]);
    tiles.fill_uniform(DocRect::new(128, 0, 255, 255), [255, 255, 255, 255]);
    doc
}

fn pixel(doc: &Document, x: i32, y: i32) -> [u8; 4] {
    doc.layers[doc.active_layer]
        .tiles()
        .unwrap()
        .get_pixel(x, y)
}

fn drag(doc: &mut Document, from: (f32, f32), to: (f32, f32)) {
    let (sx, sy) = doc.camera.to_screen(from.0, from.1);
    let (ex, ey) = doc.camera.to_screen(to.0, to.1);
    doc.pointer_down(sx, sy);
    doc.pointer_move(ex, ey);
    doc.pointer_up(ex, ey);
}

#[test]
fn blur_softens_the_edge_it_is_dragged_along() {
    let mut doc = edged_board();
    doc.tool = Tool::Blur;
    doc.brush_size = 24.0;
    doc.set_blur_strength(1.0);

    assert_eq!(pixel(&doc, 127, 100), [0, 0, 0, 255], "hard edge to start");
    assert_eq!(pixel(&doc, 128, 100), [255, 255, 255, 255]);

    drag(&mut doc, (128.0, 60.0), (128.0, 180.0));

    let left = pixel(&doc, 127, 120);
    let right = pixel(&doc, 128, 120);
    assert!(
        left[0] > 0 && left[0] < 255,
        "the black side picked up some white: {left:?}"
    );
    assert!(
        right[0] > 0 && right[0] < 255,
        "the white side picked up some black: {right:?}"
    );
    assert!(
        left[0] < right[0],
        "and the gradient still runs the right way: {left:?} -> {right:?}"
    );
}

/// The brush is a disc, not its bounding box. A pixel a brush-radius away along the stroke's
/// normal must be untouched, or the "soft edge" is a square one.
#[test]
fn blur_stays_inside_the_disc() {
    let mut doc = edged_board();
    doc.tool = Tool::Blur;
    doc.brush_size = 20.0;
    doc.set_blur_strength(1.0);

    drag(&mut doc, (128.0, 100.0), (128.0, 100.0));

    assert_eq!(
        pixel(&doc, 127, 100 - 40),
        [0, 0, 0, 255],
        "well outside the disc, untouched"
    );
    assert_ne!(
        pixel(&doc, 127, 100),
        [0, 0, 0, 255],
        "at the centre, blurred"
    );
}

/// Strength 0 is the slider's own floor, and the brush has to be a genuine no-op there — not
/// "almost unchanged". A tool that dirties tiles while doing nothing costs a re-upload and a
/// wasted undo entry every stroke.
#[test]
fn zero_strength_changes_nothing() {
    let mut doc = edged_board();
    doc.tool = Tool::Blur;
    doc.brush_size = 24.0;
    doc.set_blur_strength(0.0);

    drag(&mut doc, (128.0, 60.0), (128.0, 180.0));

    assert_eq!(pixel(&doc, 127, 120), [0, 0, 0, 255]);
    assert_eq!(pixel(&doc, 128, 120), [255, 255, 255, 255]);
    assert!(!doc.history.can_undo(), "and left no undo step behind");
}

/// Blur commits as the pointer moves rather than at pointer-up, which is exactly the shape of
/// a bug that pushes one history entry per event. A stroke is one undo, and undoing it puts
/// every pixel back — including the ones blurred on the very first event.
#[test]
fn a_whole_blur_stroke_is_one_undo() {
    let mut doc = edged_board();
    doc.tool = Tool::Blur;
    doc.brush_size = 24.0;
    doc.set_blur_strength(1.0);

    let (sx, sy) = doc.camera.to_screen(128.0, 40.0);
    doc.pointer_down(sx, sy);
    for step in 1..=6 {
        let (mx, my) = doc.camera.to_screen(128.0, 40.0 + step as f32 * 30.0);
        doc.pointer_move(mx, my);
    }
    let (ex, ey) = doc.camera.to_screen(128.0, 220.0);
    doc.pointer_up(ex, ey);

    assert_ne!(pixel(&doc, 127, 60), [0, 0, 0, 255], "the start blurred");
    assert_ne!(pixel(&doc, 127, 200), [0, 0, 0, 255], "so did the end");

    doc.undo();
    assert_eq!(pixel(&doc, 127, 60), [0, 0, 0, 255], "start restored");
    assert_eq!(pixel(&doc, 127, 200), [0, 0, 0, 255], "end restored too");
    assert!(!doc.history.can_undo(), "one entry for the whole stroke");
}

/// Blur sees the destination, so it is the one paint tool where reading pixels the same stamp
/// already wrote would show up as a smear along the direction of iteration instead of a blur.
/// A symmetric edge blurred by a symmetric stroke has to stay symmetric.
#[test]
fn blur_does_not_smear_in_the_direction_of_iteration() {
    let mut doc = edged_board();
    doc.tool = Tool::Blur;
    doc.brush_size = 32.0;
    doc.set_blur_strength(1.0);

    drag(&mut doc, (128.0, 128.0), (128.0, 128.0));

    for offset in 1..=6 {
        let left = pixel(&doc, 128 - offset, 128);
        let right = pixel(&doc, 127 + offset, 128);
        let sum = left[0] as i32 + right[0] as i32;
        assert!(
            (sum - 255).abs() <= 2,
            "pixels {offset} either side of a black/white edge should still mirror: \
             {left:?} / {right:?}"
        );
    }
}

/// Every other paint tool refuses a text layer, because `text_layer::resync` rebuilds its
/// tiles from the run and would wipe the stroke on the next keystroke. Blur is no different.
#[test]
fn blur_refuses_a_text_layer() {
    let mut doc = Document::new("p".into(), "t", 256, 256);
    doc.resize_viewport(256.0, 256.0, 1.0);
    doc.fit_to_view();
    doc.tool = Tool::Text;
    let (sx, sy) = doc.camera.to_screen(60.0, 60.0);
    doc.pointer_down(sx, sy);
    doc.text_insert("hi");
    doc.commit_text();
    let text_index = (0..doc.layers.len())
        .find(|&i| doc.layers[i].is_text())
        .expect("a text layer");
    doc.active_layer = text_index;

    doc.tool = Tool::Blur;
    doc.brush_size = 24.0;
    doc.set_blur_strength(1.0);
    let before = doc.layers[text_index].tiles().unwrap().clone();

    drag(&mut doc, (60.0, 60.0), (90.0, 60.0));

    assert_eq!(
        &before,
        doc.layers[text_index].tiles().unwrap(),
        "a text layer's cached glyph tiles were left alone"
    );
}

/// The bucket clips its flood to the active selection; blur clips its stamp the same way, so
/// "select, then soften just this" works without masking by hand.
#[test]
fn blur_clips_to_the_active_selection() {
    let mut doc = edged_board();
    doc.selection = Some(Selection {
        shape: SelectionShape::Rect {
            start: (0.0, 0.0),
            end: (256.0, 100.0),
        },
    });
    doc.tool = Tool::Blur;
    doc.brush_size = 40.0;
    doc.set_blur_strength(1.0);

    drag(&mut doc, (128.0, 100.0), (128.0, 100.0));

    assert_ne!(pixel(&doc, 127, 90), [0, 0, 0, 255], "inside the selection");
    assert_eq!(
        pixel(&doc, 127, 110),
        [0, 0, 0, 255],
        "outside it, untouched"
    );
}

/// Tiles hold straight alpha, so the kernel has to premultiply before it averages. Blurring a
/// painted shape against transparency in straight space drags the edge toward whatever colour
/// the fully transparent pixels happen to carry — black, here, which shows as a dark halo.
#[test]
fn blurring_against_transparency_leaves_no_dark_halo() {
    let mut doc = Document::new("p".into(), "t", 256, 256);
    doc.resize_viewport(256.0, 256.0, 1.0);
    doc.fit_to_view();
    let tiles = doc.layers[doc.active_layer].tiles_mut().unwrap();
    // Opaque white on a transparent field, with the transparent pixels carrying black RGB —
    // the exact arrangement a straight-alpha average gets wrong.
    tiles.fill_uniform(DocRect::new(100, 100, 160, 160), [255, 255, 255, 255]);

    doc.tool = Tool::Blur;
    doc.brush_size = 30.0;
    doc.set_blur_strength(1.0);
    drag(&mut doc, (100.0, 130.0), (100.0, 130.0));

    let edge = pixel(&doc, 101, 130);
    assert!(edge[3] < 255, "alpha did soften at the edge");
    assert!(
        edge[0] > 200 && edge[1] > 200 && edge[2] > 200,
        "but the colour stayed white instead of being pulled toward black: {edge:?}"
    );
}
