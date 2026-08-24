//! `Document::composite_overview` — the whole-document thumbnail behind recents, project
//! thumbs and the workspace extend overlay.
//!
//! It samples the stack per pixel rather than compositing the full document and shrinking it,
//! so the things worth pinning are the sampling grid (aspect, the cap, the degenerate one-
//! pixel axis) and that it agrees with the full composite about what is visible.

use calumma_core::tile::DocRect;
use calumma_core::vector::{VectorItem, VectorShape};
use calumma_core::{BlendMode, Document, LayerTransform, Shape, Tool};

fn doc(w: u32, h: u32) -> Document {
    Document::new("p".into(), "t", w, h)
}

fn paint(doc: &mut Document, index: usize, rect: DocRect, rgba: [u8; 4]) {
    doc.layers[index]
        .tiles_mut()
        .unwrap()
        .paint_rect(rect, |_, _, _| Some(rgba));
}

fn pixel(rgba: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * w + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

#[test]
fn an_overview_fits_inside_the_cap_and_keeps_the_aspect() {
    let (w, h, rgba) = doc(800, 200).composite_overview(64);
    assert!(w <= 64 && h <= 64, "{w}x{h}");
    assert_eq!(w, 64, "the long side lands on the cap");
    assert_eq!(h, 16, "and the short side keeps the 4:1 aspect");
    assert_eq!(rgba.len(), (w as usize) * (h as usize) * 4);
}

/// A thumbnail never scales a small board *up* — there is no more detail to show, and the
/// shell would rather draw 32 real pixels than 512 blurry ones.
#[test]
fn a_document_smaller_than_the_cap_is_left_at_its_own_size() {
    let (w, h, _) = doc(32, 24).composite_overview(512);
    assert_eq!((w, h), (32, 24));
}

/// A one-pixel axis would divide by `tw - 1`, so both axes have their own guard.
#[test]
fn a_single_pixel_axis_does_not_divide_by_zero() {
    for (dw, dh) in [(1, 64), (64, 1), (1, 1)] {
        let (w, h, rgba) = doc(dw, dh).composite_overview(16);
        assert!(w >= 1 && h >= 1, "{dw}x{dh} gave {w}x{h}");
        assert_eq!(rgba.len(), (w as usize) * (h as usize) * 4);
    }
}

#[test]
fn a_zero_cap_still_produces_at_least_one_pixel() {
    let (w, h, rgba) = doc(64, 64).composite_overview(0);
    assert_eq!((w, h), (1, 1));
    assert_eq!(rgba.len(), 4);
}

#[test]
fn an_overview_of_the_default_board_is_the_paper_colour() {
    let (w, h, rgba) = doc(64, 64).composite_overview(8);
    for y in 0..h {
        for x in 0..w {
            assert_eq!(pixel(&rgba, w, x, y), [255, 255, 255, 255]);
        }
    }
}

/// Hidden layers, layers at zero opacity and layers with nothing painted contribute nothing —
/// the same three filters the live composite applies, so a thumbnail cannot show something the
/// board does not.
#[test]
fn an_overview_leaves_out_what_the_board_leaves_out() {
    let mut d = doc(64, 64);
    d.remove_layer(0);

    d.add_layer("Hidden");
    let hidden = d.active_layer;
    paint(&mut d, hidden, DocRect::new(0, 0, 63, 63), [255, 0, 0, 255]);
    d.set_layer_visible(hidden, false);

    d.add_layer("Empty");
    let empty = d.active_layer;
    assert!(d.layers[empty].content_bounds().is_none());

    let (w, h, rgba) = d.composite_overview(8);
    for y in 0..h {
        for x in 0..w {
            assert_eq!(
                pixel(&rgba, w, x, y),
                [0, 0, 0, 0],
                "nothing visible, so nothing drawn"
            );
        }
    }
}

#[test]
fn an_overview_composites_the_stack_in_order() {
    let mut d = doc(64, 64);
    d.add_layer("Under");
    let under = d.active_layer;
    paint(&mut d, under, DocRect::new(0, 0, 63, 63), [255, 0, 0, 255]);
    d.add_layer("Over");
    let over = d.active_layer;
    paint(&mut d, over, DocRect::new(0, 0, 63, 63), [0, 0, 255, 255]);

    let (w, _, rgba) = d.composite_overview(8);
    assert_eq!(
        pixel(&rgba, w, 4, 4),
        [0, 0, 255, 255],
        "the topmost opaque layer wins"
    );
}

#[test]
fn an_overview_honours_layer_opacity_and_blend_mode() {
    let mut d = doc(64, 64);
    d.add_layer("Over");
    let over = d.active_layer;
    paint(&mut d, over, DocRect::new(0, 0, 63, 63), [0, 0, 0, 255]);
    d.layers[over].blend_mode = BlendMode::Multiply;

    let (w, _, multiplied) = d.composite_overview(8);
    assert_eq!(
        pixel(&multiplied, w, 4, 4),
        [0, 0, 0, 255],
        "black multiplied onto white paper is black"
    );

    d.layers[over].opacity = 0.0;
    let (w, _, transparent) = d.composite_overview(8);
    assert_eq!(
        pixel(&transparent, w, 4, 4),
        [255, 255, 255, 255],
        "at zero opacity the paper shows through untouched"
    );
}

/// A vector layer has no tiles, so the overview has to reach it through its items — otherwise
/// a drawing made entirely of shapes would thumbnail as an empty board.
#[test]
fn an_overview_includes_a_vector_layer() {
    let mut d = doc(64, 64);
    d.remove_layer(0);
    let layer = d.add_vector_layer("V");
    *d.layers[layer].content.items_mut().unwrap() = vec![VectorItem::Shape(VectorShape {
        shape: Shape {
            tool: Tool::Rect,
            start: (0.0, 0.0),
            end: (63.0, 63.0),
            half_width: 1.0,
            fill: true,
            stroke: false,
        },
        color: [0, 200, 0, 255],
        stroke_color: [0, 200, 0, 255],
    })];

    let (w, _, rgba) = d.composite_overview(8);
    let px = pixel(&rgba, w, 4, 4);
    assert_eq!(px[3], 255, "the shape is there: {px:?}");
    assert!(px[1] > px[0] && px[1] > px[2], "and it is the green one");
}

/// The overview and the full composite are two samplings of one stack, so a pixel picked out
/// of each has to agree — that is what makes the thumbnail a preview rather than a guess.
#[test]
fn an_overview_agrees_with_the_full_composite() {
    let mut d = doc(64, 64);
    d.add_layer("Ink");
    let ink = d.active_layer;
    paint(&mut d, ink, DocRect::new(0, 0, 31, 63), [200, 30, 40, 255]);

    let (w, _, overview) = d.composite_overview(64);
    let (fw, _, full) = d.composite_rgba();
    for (x, y) in [(4u32, 4u32), (10, 40), (48, 8), (60, 60)] {
        assert_eq!(
            pixel(&overview, w, x, y),
            pixel(&full, fw, x, y),
            "at ({x}, {y})"
        );
    }
}

/// A layer moved by `⌘T` has to thumbnail where it *is*, not where its pixels are stored —
/// the transform is non-destructive, so only the sampling knows about it.
#[test]
fn an_overview_samples_a_layer_through_its_transform() {
    let mut d = doc(64, 64);
    d.remove_layer(0);
    d.add_layer("Ink");
    let ink = d.active_layer;
    paint(&mut d, ink, DocRect::new(0, 0, 15, 15), [0, 0, 0, 255]);

    let (w, _, before) = d.composite_overview(64);
    assert_eq!(pixel(&before, w, 4, 4)[3], 255, "ink starts top-left");
    assert_eq!(pixel(&before, w, 40, 40)[3], 0);

    d.layers[ink].transform = Some(LayerTransform {
        offset_x: 40.0,
        offset_y: 40.0,
        ..LayerTransform::default()
    });
    let (w, _, after) = d.composite_overview(64);
    assert_eq!(pixel(&after, w, 4, 4)[3], 0, "it left the corner");
    assert_eq!(pixel(&after, w, 44, 44)[3], 255, "and arrived down-right");
}

/// The regression guard for the overview proxy having been blind to vector layers: a mixed
/// document has to read the same through the thumbnail and through the flatten.
///
/// Sampled away from the shape's own edge on purpose. The two paths pick different points
/// inside a pixel — the flatten samples centres (`x + 0.5`), the overview samples the grid it
/// is scaling onto — so an antialiased edge lands on a different coverage in each. That gap is
/// sub-pixel, and the overview only runs when the board is zoomed far enough out that a whole
/// document pixel is smaller than a screen one.
#[test]
fn a_mixed_document_reads_the_same_through_the_overview_and_the_flatten() {
    let mut d = doc(64, 64);
    d.add_layer("Ink");
    let ink = d.active_layer;
    paint(&mut d, ink, DocRect::new(4, 4, 30, 30), [200, 30, 40, 255]);

    let shapes = d.add_vector_layer("V");
    *d.layers[shapes].content.items_mut().unwrap() = vec![VectorItem::Shape(VectorShape {
        shape: Shape {
            tool: Tool::Rect,
            start: (20.0, 20.0),
            end: (58.0, 58.0),
            half_width: 1.0,
            fill: true,
            stroke: false,
        },
        color: [0, 90, 220, 255],
        stroke_color: [0, 90, 220, 255],
    })];

    let (w, h, overview) = d.composite_overview(64);
    let (fw, fh, full) = d.composite_rgba();
    assert_eq!((w, h), (fw, fh));

    let inside_the_shape = [(30u32, 30u32), (40, 50), (55, 55)];
    let raster_only = [(6u32, 6u32), (10, 25), (28, 8)];
    let empty = [(2u32, 60u32), (62, 2)];
    for (x, y) in inside_the_shape
        .iter()
        .chain(&raster_only)
        .chain(&empty)
        .copied()
    {
        assert_eq!(
            pixel(&overview, w, x, y),
            pixel(&full, fw, x, y),
            "at ({x}, {y})"
        );
    }
    assert_eq!(
        pixel(&overview, w, 40, 50),
        [0, 90, 220, 255],
        "and the shape really is the colour it was drawn in"
    );
}
