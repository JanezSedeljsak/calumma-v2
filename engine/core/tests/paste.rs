//! Pasting into an open document, and the data loss that used to be the answer when it did not
//! fit.
//!
//! `an_oversized_paste_keeps_every_pixel` is the regression that matters: the old path anchored
//! the blit top-left and let `paint_rect`'s bounds intersect eat the rest, so the bottom-right
//! of an oversized image was never written anywhere.

use calumma_core::document::*;
use calumma_core::paste::PasteOutcome;
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
fn an_image_that_fits_is_not_moved_and_does_not_overflow() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    let rgba = two_tone(32, 32);
    assert_eq!(
        doc.paste_image_as_layer("Pasted", &rgba, 32, 32),
        PasteOutcome::Native
    );
    let b = opaque_bounds(&doc, doc.active_layer);
    assert_eq!((b.min_x, b.min_y, b.max_x, b.max_y), (0, 0, 31, 31));
    let grid = doc.layers[doc.active_layer].tiles().unwrap();
    assert_eq!(
        grid.bounds(),
        grid.doc_bounds(),
        "storage is still the canvas"
    );
}

/// The bug, stated as the thing that must not happen again: every corner of the source is
/// somewhere on the pasted layer, at the resolution it arrived in.
#[test]
fn an_oversized_paste_keeps_every_pixel() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    let rgba = two_tone(200, 200);
    assert_eq!(
        doc.paste_image_as_layer("Pasted", &rgba, 200, 200),
        PasteOutcome::Overflowing
    );
    let layer = doc.active_layer;
    let b = opaque_bounds(&doc, layer);
    assert_eq!(
        (b.max_x - b.min_x + 1, b.max_y - b.min_y + 1),
        (200, 200),
        "native size, not resampled: {b:?}"
    );
    let mid_y = (b.min_y + b.max_y) / 2;
    assert_eq!(
        pixel(&doc, layer, b.min_x, mid_y),
        [255, 0, 0, 255],
        "the left edge is there"
    );
    assert_eq!(
        pixel(&doc, layer, b.max_x, mid_y),
        [0, 0, 255, 255],
        "and so is the right edge, which is what the crop threw away"
    );
}

/// Centred, so the middle of the picture is the part on the paper — which means a negative
/// origin, the thing a document-sized grid could not express at all.
#[test]
fn an_oversized_paste_is_centred_on_the_canvas() {
    let mut doc = Document::new("p".into(), "t", 100, 100);
    let rgba = two_tone(400, 200);
    assert_eq!(
        doc.paste_image_as_layer("Pasted", &rgba, 400, 200),
        PasteOutcome::Overflowing
    );
    let b = opaque_bounds(&doc, doc.active_layer);
    assert_eq!((b.min_x, b.min_y), (-150, -50));
    assert_eq!((b.max_x, b.max_y), (249, 149));
}

#[test]
fn the_canvas_is_never_resized_by_a_paste() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    let rgba = two_tone(500, 300);
    doc.paste_image_as_layer("Pasted", &rgba, 500, 300);
    assert_eq!((doc.width, doc.height), (64, 64));
}

/// Overflow is per layer and opt-in. Pasting something huge must not quietly let a brush
/// scribble off the paper on the layer that was already there.
#[test]
fn overflow_belongs_to_the_pasted_layer_alone() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    let existing = doc.active_layer;
    let rgba = two_tone(200, 200);
    doc.paste_image_as_layer("Pasted", &rgba, 200, 200);
    let grid = doc.layers[existing].tiles().unwrap();
    assert_eq!(grid.bounds(), grid.doc_bounds());
    let pasted = doc.layers[doc.active_layer].tiles().unwrap();
    assert!(pasted.bounds().min_x < 0);
}

/// The composite is the canvas, so what hangs off the paper contributes nothing to it — the
/// same thing `Camera::paper_scissor` does on the board.
#[test]
fn what_overflows_stays_off_the_composite() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    let rgba = two_tone(200, 200);
    doc.paste_image_as_layer("Pasted", &rgba, 200, 200);
    let (w, h, out) = doc.composite_rgba();
    assert_eq!((w, h), (64, 64));
    assert_eq!(out.len(), 64 * 64 * 4);
}

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
