use calumma_core::document::*;
use calumma_core::*;

/// One paintable layer above Paper, with a solid red square painted into the middle of an
/// otherwise transparent field — a shape with a clear boundary and a clear outside.
fn board_with_square() -> Document {
    let mut doc = Document::new("p".into(), "t", 256, 256);
    doc.resize_viewport(256.0, 256.0, 1.0);
    doc.fit_to_view();
    let tiles = doc.layers[doc.active_layer].tiles_mut().unwrap();
    tiles.fill_uniform(DocRect::new(60, 60, 139, 139), [200, 30, 30, 255]);
    doc
}

fn click(doc: &mut Document, x: f32, y: f32) {
    let (sx, sy) = doc.camera.to_screen(x, y);
    doc.pointer_down(sx, sy);
    doc.pointer_up(sx, sy);
}

fn mask(doc: &Document) -> &SelectionMask {
    match &doc.selection.as_ref().expect("a selection").shape {
        SelectionShape::Mask(m) => m,
        other => panic!("expected a mask selection, got {other:?}"),
    }
}

#[test]
fn wand_selects_the_contiguous_region_it_was_clicked_on() {
    let mut doc = board_with_square();
    doc.tool = Tool::MagicWand;
    click(&mut doc, 100.0, 100.0);

    let sel = doc.selection.as_ref().expect("a selection");
    assert!(sel.contains(100.5, 100.5), "the square is selected");
    assert!(sel.contains(60.5, 60.5), "right to its corner");
    assert!(!sel.contains(59.5, 59.5), "and not past it");
    assert!(!sel.contains(200.5, 200.5), "nor the field around it");
}

/// The mask is cropped to what the flood actually reached, not left at the size of the scope
/// it was allowed to search. Copy, cut and delete all iterate `bounds()`, so an uncropped mask
/// would make every one of them walk the whole document.
#[test]
fn wand_bounds_are_tight_to_the_region() {
    let mut doc = board_with_square();
    doc.tool = Tool::MagicWand;
    click(&mut doc, 100.0, 100.0);

    let bounds = doc.selection.as_ref().unwrap().bounds();
    assert_eq!(
        (bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y),
        (60, 60, 139, 139)
    );
}

/// Clicking the transparent field selects the field — alpha counts toward the tolerance, which
/// is what makes "select the empty space around a sketch" work at all.
#[test]
fn wand_selects_transparency_too() {
    let mut doc = board_with_square();
    doc.tool = Tool::MagicWand;
    click(&mut doc, 10.0, 10.0);

    let sel = doc.selection.as_ref().expect("a selection");
    assert!(sel.contains(10.5, 10.5), "the empty field is selected");
    assert!(!sel.contains(100.5, 100.5), "the square is not");
}

/// The wand reads the *active* layer, not the composite. Clicking the transparent hole in a
/// sketch has to answer about that sketch, not about the Paper showing through it.
#[test]
fn wand_reads_the_active_layer_not_the_composite() {
    let mut doc = board_with_square();
    let paper = (0..doc.layers.len())
        .find(|&i| doc.layers[i].is_paper())
        .expect("Paper");
    assert_ne!(paper, doc.active_layer, "the square is on its own layer");

    doc.tool = Tool::MagicWand;
    click(&mut doc, 10.0, 10.0);
    let sel = doc.selection.as_ref().expect("a selection");
    assert!(
        !sel.contains(100.5, 100.5),
        "Paper is opaque white everywhere, so a composite read would have selected \
         everything but the square's color — this must be the active layer's transparency"
    );
    assert!(sel.contains(10.5, 10.5));
}

/// The wand replaces the selection, so a previous one must not clip it. The bucket, which
/// paints *into* the selection, deliberately does the opposite.
#[test]
fn a_previous_selection_does_not_clip_the_next_wand() {
    let mut doc = board_with_square();
    doc.selection = Some(Selection {
        shape: SelectionShape::Rect {
            start: (0.0, 0.0),
            end: (100.0, 100.0),
        },
    });
    doc.tool = Tool::MagicWand;
    click(&mut doc, 100.0, 100.0);

    let sel = doc.selection.as_ref().expect("a selection");
    assert!(
        sel.contains(135.5, 135.5),
        "the wand reached the whole square, past the old rect selection"
    );
}

/// Tolerance is one number for the bucket and the wand both, because they are one traversal.
/// A wand that disagreed with the bucket about what "contiguous" means would be a bug report.
#[test]
fn tolerance_widens_what_the_wand_reaches() {
    let mut doc = Document::new("p".into(), "t", 64, 64);
    doc.resize_viewport(64.0, 64.0, 1.0);
    doc.fit_to_view();
    {
        let tiles = doc.layers[doc.active_layer].tiles_mut().unwrap();
        tiles.fill_uniform(DocRect::new(0, 0, 31, 63), [100, 100, 100, 255]);
        // A near-identical neighbour: within a generous tolerance, outside a strict one.
        tiles.fill_uniform(DocRect::new(32, 0, 63, 63), [112, 100, 100, 255]);
    }
    doc.tool = Tool::MagicWand;

    doc.set_tolerance(2);
    click(&mut doc, 10.0, 10.0);
    assert!(
        !doc.selection.as_ref().unwrap().contains(40.5, 10.5),
        "a strict tolerance stops at the seam"
    );

    doc.set_tolerance(60);
    click(&mut doc, 10.0, 10.0);
    assert!(
        doc.selection.as_ref().unwrap().contains(40.5, 10.5),
        "a generous one crosses it"
    );
}

/// The whole point of keeping the wand inside `SelectionShape`: everything downstream already
/// goes through `contains`, so clipping a paint stroke needed no changes to accept a bitmap.
#[test]
fn a_wand_selection_clips_the_bucket_like_any_other() {
    let mut doc = board_with_square();
    doc.tool = Tool::MagicWand;
    click(&mut doc, 100.0, 100.0);

    doc.tool = Tool::Fill;
    doc.set_color([0, 0, 255, 255]);
    click(&mut doc, 100.0, 100.0);

    let inside = doc.layers[doc.active_layer].tiles().unwrap();
    assert_eq!(
        inside.get_pixel(100, 100),
        [0, 0, 255, 255],
        "filled inside"
    );
    assert_eq!(
        inside.get_pixel(40, 40),
        [0, 0, 0, 0],
        "and nothing leaked outside the wand selection"
    );
}

/// A click that reaches nothing must leave no selection at all. An empty-but-present selection
/// is worse than none: it silently clips every paint stroke to nothing.
#[test]
fn a_wand_click_off_the_canvas_leaves_the_selection_alone() {
    let mut doc = board_with_square();
    doc.tool = Tool::MagicWand;
    click(&mut doc, -50.0, -50.0);
    assert!(doc.selection.is_none());
}

/// The marching ants are drawn from an outline traced once at commit, never by walking the
/// bitmap per frame. A solid rectangle's boundary is four runs — one per side — which is what
/// merging is for: unmerged it would be 320 one-pixel segments for this square alone.
#[test]
fn the_outline_is_traced_once_and_merged_into_runs() {
    let mut doc = board_with_square();
    doc.tool = Tool::MagicWand;
    click(&mut doc, 100.0, 100.0);

    let mut sides: Vec<[f32; 4]> = mask(&doc).outline().to_vec();
    sides.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(
        sides,
        [
            [60.0, 60.0, 60.0, 140.0],
            [60.0, 60.0, 140.0, 60.0],
            [60.0, 140.0, 140.0, 140.0],
            [140.0, 60.0, 140.0, 140.0],
        ],
        "four sides, four runs, on pixel edges"
    );
}

/// A region with a hole in it traces both boundaries — the outer edge and the hole's — because
/// a run is emitted wherever the neighbour is unselected, whichever side it is on.
#[test]
fn a_hole_gets_its_own_outline() {
    let mut doc = Document::new("p".into(), "t", 128, 128);
    doc.resize_viewport(128.0, 128.0, 1.0);
    doc.fit_to_view();
    {
        let tiles = doc.layers[doc.active_layer].tiles_mut().unwrap();
        tiles.fill_uniform(DocRect::new(20, 20, 99, 99), [10, 200, 10, 255]);
        tiles.fill_uniform(DocRect::new(50, 50, 69, 69), [0, 0, 0, 255]);
    }
    doc.tool = Tool::MagicWand;
    click(&mut doc, 30.0, 30.0);

    let sel = doc.selection.as_ref().expect("a selection");
    assert!(sel.contains(30.5, 30.5), "the ring is selected");
    assert!(!sel.contains(60.5, 60.5), "the hole is not");
    assert_eq!(
        mask(&doc).outline().len(),
        8,
        "four runs for the outer square, four for the hole"
    );
}
