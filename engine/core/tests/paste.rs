//! Pasting into an open document, and the data loss that used to be the answer when it did
//! not fit.
//!
//! `everything_outside_the_paper_used_to_be_destroyed` is the regression that matters: the
//! old path anchored the blit top-left and let `paint_rect`'s bounds intersect eat the rest,
//! so the bottom-right of an oversized image was never written anywhere.

use calumma_core::document::*;
use calumma_core::limits;
use calumma_core::paste::{PasteFit, PasteOutcome};
use calumma_core::selection::{Selection, SelectionShape};
use calumma_core::tile::DocRect;

fn pixel(doc: &Document, index: usize, x: i32, y: i32) -> [u8; 4] {
    doc.layers[index].tiles().unwrap().get_pixel(x, y)
}

/// A `w` × `h` image split down the middle: red on the left, blue on the right. Both halves
/// have to survive a paste, which is what the crop broke.
fn two_tone(w: u32, h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (w as usize) * (h as usize) * 4];
    for y in 0..h as usize {
        for x in 0..w as usize {
            let i = (y * w as usize + x) * 4;
            let left = x < (w as usize) / 2;
            out[i..i + 4].copy_from_slice(&if left {
                [255u8, 0, 0, 255]
            } else {
                [0u8, 0, 255, 255]
            });
        }
    }
    out
}

fn opaque_bounds(doc: &Document, index: usize) -> DocRect {
    doc.layers[index]
        .tiles()
        .unwrap()
        .opaque_bounds()
        .expect("the pasted layer has pixels")
}

#[test]
fn paste_image_creates_new_layer_at_selection_origin() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    doc.selection = Some(Selection {
        shape: SelectionShape::Rect {
            start: (10.0, 10.0),
            end: (12.0, 12.0),
        },
    });
    let rgba = vec![5u8, 6, 7, 255, 5, 6, 7, 255, 5, 6, 7, 255, 5, 6, 7, 255];
    let before = doc.layers.len();
    assert_eq!(
        doc.paste_image_as_layer("Pasted", &rgba, 2, 2),
        PasteOutcome::Native
    );
    assert_eq!(doc.layers.len(), before + 1);
    assert_eq!(pixel(&doc, doc.active_layer, 10, 10), [5, 6, 7, 255]);
    assert_eq!(pixel(&doc, doc.active_layer, 0, 0), [0, 0, 0, 0]);
}

#[test]
fn an_image_that_fits_is_not_scaled_or_moved() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    let rgba = two_tone(32, 32);
    assert_eq!(
        doc.paste_image_as_layer("Pasted", &rgba, 32, 32),
        PasteOutcome::Native
    );
    let b = opaque_bounds(&doc, doc.active_layer);
    assert_eq!((b.min_x, b.min_y, b.max_x, b.max_y), (0, 0, 31, 31));
}

/// The bug, stated as the thing that must not happen again: every corner of the source has to
/// be somewhere on the pasted layer.
#[test]
fn everything_outside_the_paper_used_to_be_destroyed() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    let rgba = two_tone(200, 200);
    let outcome = doc.paste_image_as_layer("Pasted", &rgba, 200, 200);
    assert_eq!(outcome, PasteOutcome::Scaled);
    let layer = doc.active_layer;
    let b = opaque_bounds(&doc, layer);
    assert!(b.max_x - b.min_x >= 60, "the whole width landed: {b:?}");
    let mid_y = (b.min_y + b.max_y) / 2;
    assert_eq!(
        pixel(&doc, layer, b.min_x + 2, mid_y),
        [255, 0, 0, 255],
        "the left half is there"
    );
    assert_eq!(
        pixel(&doc, layer, b.max_x - 2, mid_y),
        [0, 0, 255, 255],
        "and so is the right half, which is what the crop threw away"
    );
}

#[test]
fn scale_to_fit_centres_and_keeps_the_aspect_ratio() {
    let mut doc = Document::new("p".into(), "t", 100, 100);
    let rgba = two_tone(400, 200);
    assert_eq!(
        doc.paste_image_as_layer("Pasted", &rgba, 400, 200),
        PasteOutcome::Scaled
    );
    let b = opaque_bounds(&doc, doc.active_layer);
    let (w, h) = (b.max_x - b.min_x + 1, b.max_y - b.min_y + 1);
    assert_eq!((w, h), (100, 50), "2:1 stays 2:1 and fills the long side");
    assert_eq!(b.min_x, 0);
    assert_eq!(b.min_y, 25, "centred on the short axis");
}

#[test]
fn grow_canvas_takes_the_image_at_native_size() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    doc.set_paste_fit(PasteFit::GrowCanvas);
    let rgba = two_tone(200, 120);
    assert_eq!(
        doc.paste_image_as_layer("Pasted", &rgba, 200, 120),
        PasteOutcome::Grown
    );
    assert_eq!((doc.width, doc.height), (200, 120));
    let b = opaque_bounds(&doc, doc.active_layer);
    assert_eq!((b.max_x - b.min_x + 1, b.max_y - b.min_y + 1), (200, 120));
}

/// Growing is top-left anchored, exactly like a manual canvas resize, so nothing that was
/// already on the board moves out from under the user.
#[test]
fn grow_canvas_leaves_existing_artwork_where_it_was() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    doc.set_paste_fit(PasteFit::GrowCanvas);
    let existing = doc.active_layer;
    doc.layers[existing]
        .tiles_mut()
        .unwrap()
        .set_pixel(5, 5, [9, 9, 9, 255]);
    let rgba = two_tone(200, 200);
    assert_eq!(
        doc.paste_image_as_layer("Pasted", &rgba, 200, 200),
        PasteOutcome::Grown
    );
    assert_eq!(pixel(&doc, existing, 5, 5), [9, 9, 9, 255]);
}

/// The two modes compose rather than one of them failing: past `MAX_CANVAS_SIDE` the paper
/// grows as far as it is allowed and the remainder is scaled.
#[test]
fn an_image_past_the_canvas_ceiling_grows_then_scales() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    doc.set_paste_fit(PasteFit::GrowCanvas);
    let side = limits::MAX_CANVAS_SIDE + 400;
    let rgba = vec![255u8; (side as usize) * 4 * 4];
    let outcome = doc.paste_image_as_layer("Pasted", &rgba, side, 4);
    assert_eq!(outcome, PasteOutcome::GrownAndScaled);
    assert_eq!(doc.width, limits::MAX_CANVAS_SIDE);
}

#[test]
fn the_default_is_scale_to_fit() {
    let doc = Document::new("p".into(), "t", 64, 64);
    assert_eq!(doc.paste_fit(), PasteFit::ScaleToFit);
}

/// A paste that writes nothing takes its layer back out rather than leaving an empty one
/// behind to explain a failure.
#[test]
fn a_fully_transparent_image_leaves_no_layer_behind() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    let before = doc.layers.len();
    let rgba = vec![0u8; 16 * 16 * 4];
    assert_eq!(
        doc.paste_image_as_layer("Pasted", &rgba, 16, 16),
        PasteOutcome::Failed
    );
    assert_eq!(doc.layers.len(), before);
}

#[test]
fn a_malformed_buffer_is_refused_without_adding_a_layer() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    let before = doc.layers.len();
    assert_eq!(
        doc.paste_image_as_layer("Pasted", &[1, 2, 3], 8, 8),
        PasteOutcome::Failed
    );
    assert_eq!(
        doc.paste_image_as_layer("Pasted", &[], 0, 0),
        PasteOutcome::Failed
    );
    assert_eq!(doc.layers.len(), before);
}
