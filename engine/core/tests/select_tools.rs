use calumma_core::document::*;
use calumma_core::vector::{VectorItem, VectorShape};
use calumma_core::*;

const DOC: u32 = 128;

fn board() -> Document {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.resize_viewport(DOC as f32, DOC as f32, 1.0);
    doc.fit_to_view();
    doc
}

fn paint_square(doc: &mut Document, layer: usize) {
    doc.layers[layer]
        .tiles_mut()
        .unwrap()
        .fill_uniform(DocRect::new(60, 60, 139, 139), [200, 30, 30, 255]);
}

fn rect_item() -> VectorItem {
    VectorItem::Shape(VectorShape {
        shape: Shape {
            tool: Tool::Rect,
            start: (50.0, 50.0),
            end: (110.0, 110.0),
            half_width: 1.0,
            fill: true,
            stroke: false,
        },
        color: [0, 0, 255, 255],
        stroke_color: [0, 0, 255, 255],
    })
}

fn click(doc: &mut Document, x: f32, y: f32) {
    let (sx, sy) = doc.camera.to_screen(x, y);
    doc.pointer_down(sx, sy);
    doc.pointer_up(sx, sy);
}

fn drag_rect(doc: &mut Document, from: (f32, f32), to: (f32, f32)) {
    let (sx0, sy0) = doc.camera.to_screen(from.0, from.1);
    let (sx1, sy1) = doc.camera.to_screen(to.0, to.1);
    doc.pointer_down(sx0, sy0);
    doc.pointer_move(sx1, sy1);
    doc.pointer_up(sx1, sy1);
}

fn lasso(doc: &mut Document, points: &[(f32, f32)]) {
    for (i, &(x, y)) in points.iter().enumerate() {
        let (sx, sy) = doc.camera.to_screen(x, y);
        if i == 0 {
            doc.pointer_down(sx, sy);
        } else {
            doc.pointer_move(sx, sy);
        }
    }
    let &(x, y) = points.last().expect("a point");
    let (sx, sy) = doc.camera.to_screen(x, y);
    doc.pointer_up(sx, sy);
}

/// A committed text layer, typed at a size that leaves a solid glyph to aim at.
fn typed_layer(doc: &mut Document, text: &str) {
    doc.tool = Tool::Text;
    doc.text_style.size = 64.0;
    let (sx, sy) = doc.camera.to_screen(20.0, 60.0);
    doc.pointer_down(sx, sy);
    doc.pointer_up(sx, sy);
    doc.text_insert(text);
    doc.commit_text();
}

/// The first fully opaque pixel of a layer, in document space.
///
/// Every test that clicks *at* artwork rather than near it goes through this. Glyph metrics
/// are the system's, so the only coordinate that can be asserted about is one the layer was
/// asked for — and a failure here names the real cause (nothing rasterized at all) instead of
/// surfacing as an empty selection three lines later.
#[track_caller]
fn opaque_pixel(doc: &Document, layer: usize) -> (i32, i32) {
    let bounds = doc.layers[layer]
        .content_bounds()
        .expect("the layer painted nothing — are there no system fonts?");
    let grid = doc.layers[layer].tiles().expect("a tile cache");
    for y in (bounds.1 as i32)..(bounds.3 as i32) {
        for x in (bounds.0 as i32)..(bounds.2 as i32) {
            if grid.get_pixel(x, y)[3] == 255 {
                return (x, y);
            }
        }
    }
    panic!("no fully opaque pixel inside the layer's own painted bounds");
}

fn mask(doc: &Document) -> &SelectionMask {
    match &doc.selection.as_ref().expect("a selection").shape {
        SelectionShape::Mask(m) => m,
        other => panic!("expected a mask selection, got {other:?}"),
    }
}

#[test]
fn wand_on_vector_layer_selects_the_shape() {
    let mut doc = board();
    doc.add_vector_layer("V", rect_item());
    doc.tool = Tool::MagicWand;
    click(&mut doc, 80.0, 80.0);
    let sel = doc.selection.as_ref().expect("a selection");
    assert!(sel.contains(80.5, 80.5));
    assert!(!sel.contains(10.5, 10.5));
}

/// The wand has to reach a text layer's tile cache — slice A's whole point.
///
/// The pixel it clicks is *found*, not guessed. A glyph's painted box depends on the family
/// the system resolved and on that face's side bearings, so a hard-coded offset from the run
/// origin sits a pixel or two from the edge of the ink on the machine it was written on and
/// outside it on the next one.
#[test]
fn wand_on_text_layer_selects_painted_pixels() {
    let mut doc = board();
    typed_layer(&mut doc, "Hi");
    let layer = doc.active_layer;
    let (x, y) = opaque_pixel(&doc, layer);

    doc.tool = Tool::MagicWand;
    click(&mut doc, x as f32 + 0.5, y as f32 + 0.5);
    let sel = doc.selection.as_ref().expect("a selection");
    assert!(
        sel.contains(x as f32 + 0.5, y as f32 + 0.5),
        "the wand selected something other than the glyph pixel it was given"
    );
    assert!(
        doc.layers[layer].run().is_some(),
        "and left the run editable"
    );
}

#[test]
fn select_color_finds_every_matching_blob() {
    let mut doc = board();
    let layer = doc.active_layer;
    paint_square(&mut doc, layer);
    doc.layers[layer]
        .tiles_mut()
        .unwrap()
        .fill_uniform(DocRect::new(10, 10, 30, 30), [200, 30, 30, 255]);
    doc.tool = Tool::SelectColor;
    doc.set_select_color([200, 30, 30, 255]);
    click(&mut doc, 20.0, 20.0);
    let sel = doc.selection.as_ref().expect("a selection");
    assert!(sel.contains(20.5, 20.5));
    assert!(sel.contains(100.5, 100.5));
}

#[test]
fn select_color_click_samples_the_match_swatch() {
    let mut doc = board();
    let layer = doc.active_layer;
    paint_square(&mut doc, layer);
    doc.tool = Tool::SelectColor;
    doc.set_select_color([1, 2, 3, 255]);
    click(&mut doc, 100.0, 100.0);
    assert_eq!(doc.select_color(), [200, 30, 30, 255]);
}

#[test]
fn marquee_intersects_with_layer_ink() {
    let mut doc = board();
    let layer = doc.active_layer;
    paint_square(&mut doc, layer);
    doc.tool = Tool::SelectRect;
    drag_rect(&mut doc, (0.0, 0.0), (200.0, 200.0));
    let sel = doc.selection.as_ref().expect("a selection");
    assert!(sel.contains(100.5, 100.5));
    assert!(!sel.contains(10.5, 10.5));
    assert!(!sel.contains(150.5, 150.5));
}

#[test]
fn marquee_outside_ink_leaves_selection_unchanged() {
    let mut doc = board();
    let layer = doc.active_layer;
    paint_square(&mut doc, layer);
    doc.tool = Tool::SelectRect;
    drag_rect(&mut doc, (0.0, 0.0), (40.0, 40.0));
    assert!(doc.selection.is_none());
}

#[test]
fn lasso_commits_as_a_mask_intersecting_ink() {
    let mut doc = board();
    let layer = doc.active_layer;
    paint_square(&mut doc, layer);
    doc.tool = Tool::SelectLasso;
    for (x, y) in [(0.0, 0.0), (200.0, 0.0), (200.0, 200.0), (0.0, 200.0)] {
        let (sx, sy) = doc.camera.to_screen(x, y);
        if doc.stroke_active {
            doc.pointer_move(sx, sy);
        } else {
            doc.pointer_down(sx, sy);
        }
    }
    let (sx, sy) = doc.camera.to_screen(0.0, 200.0);
    doc.pointer_up(sx, sy);
    let _ = mask(&doc);
    assert!(doc.selection.as_ref().unwrap().contains(100.5, 100.5));
    assert!(!doc.selection.as_ref().unwrap().contains(10.5, 10.5));
}

#[test]
fn selection_tools_are_not_blocked_on_vector_layers() {
    let mut doc = board();
    doc.add_vector_layer("V", rect_item());
    for tool in [Tool::SelectRect, Tool::MagicWand, Tool::SelectColor] {
        assert_eq!(doc.tool_block(tool), ToolBlock::None, "{tool:?}");
    }
}

/// A marquee, an ellipse and a lasso *describe* a region; the wand and colour range *read* one.
/// A layer with nothing painted has an answer for the first three and none for the other two,
/// and that is exactly where the tool gate draws its line.
#[test]
fn region_select_tools_survive_a_layer_with_nothing_on_it() {
    let doc = board();
    assert!(
        doc.layers[doc.active_layer].content_bounds().is_none(),
        "a fresh layer is the case under test"
    );
    for tool in [Tool::SelectRect, Tool::SelectEllipse, Tool::SelectLasso] {
        assert_eq!(doc.tool_block(tool), ToolBlock::None, "{tool:?}");
    }
    for tool in [Tool::MagicWand, Tool::SelectColor] {
        assert_eq!(doc.tool_block(tool), ToolBlock::NoContent, "{tool:?}");
    }
}

/// The first gesture on a fresh document must not be a toast. With nothing to hug, the region
/// stands as drawn — which is what a marquee means everywhere else, too.
#[test]
fn a_marquee_on_an_empty_layer_is_the_region_it_drew() {
    let mut doc = board();
    doc.tool = Tool::SelectRect;
    drag_rect(&mut doc, (20.0, 20.0), (90.0, 90.0));
    let sel = doc.selection.as_ref().expect("a selection");
    assert!(matches!(sel.shape, SelectionShape::Rect { .. }));
    assert!(sel.contains(50.5, 50.5));
    assert!(!sel.contains(10.5, 10.5));
}

#[test]
fn a_lasso_on_an_empty_layer_keeps_its_polygon() {
    let mut doc = board();
    doc.tool = Tool::SelectLasso;
    lasso(
        &mut doc,
        &[(10.0, 10.0), (100.0, 10.0), (100.0, 100.0), (10.0, 100.0)],
    );
    let sel = doc.selection.as_ref().expect("a selection");
    assert!(matches!(sel.shape, SelectionShape::Lasso { .. }));
    assert!(sel.contains(50.5, 50.5));
}

/// The other half of the rule: once there *is* ink, the region hugs it, and a region that
/// reaches none of it leaves the previous selection alone rather than replacing it with
/// nothing — the same answer a wand click on empty space gives.
#[test]
fn once_there_is_ink_the_region_commits_as_a_mask() {
    let mut doc = board();
    let layer = doc.active_layer;
    paint_square(&mut doc, layer);
    doc.tool = Tool::SelectRect;
    drag_rect(&mut doc, (0.0, 0.0), (200.0, 200.0));
    assert!(matches!(
        doc.selection.as_ref().unwrap().shape,
        SelectionShape::Mask(_)
    ));
}

#[test]
fn a_locked_layer_refuses_every_select_tool() {
    let mut doc = board();
    let layer = doc.active_layer;
    paint_square(&mut doc, layer);
    doc.layers[layer].locked = true;
    for tool in [
        Tool::SelectRect,
        Tool::SelectEllipse,
        Tool::SelectLasso,
        Tool::MagicWand,
        Tool::SelectColor,
    ] {
        assert_eq!(doc.tool_block(tool), ToolBlock::LayerLocked, "{tool:?}");
    }
}

#[test]
fn a_lasso_of_fewer_than_three_distinct_points_selects_nothing() {
    let mut doc = board();
    doc.tool = Tool::SelectLasso;
    lasso(&mut doc, &[(20.0, 20.0), (20.0, 20.0), (60.0, 20.0)]);
    assert!(
        doc.selection.is_none(),
        "two distinct points do not enclose anything"
    );
}

/// A vector layer has no tiles at all, so before `select_sample` this was the gesture that
/// could not be made. The item has to survive it.
#[test]
fn a_marquee_over_a_vector_layer_hugs_the_shape_without_baking_it() {
    let mut doc = board();
    doc.add_vector_layer("V", rect_item());
    doc.tool = Tool::SelectRect;
    drag_rect(&mut doc, (0.0, 0.0), (128.0, 128.0));
    let sel = doc.selection.as_ref().expect("a selection");
    assert!(sel.contains(80.5, 80.5), "inside the shape");
    assert!(!sel.contains(20.5, 20.5), "and not the empty corner");
    assert!(doc.layers[doc.active_layer].content.item().is_some());
}

/// Alpha counts toward the tolerance, so the empty space *around* a drawing is a colour like
/// any other. Clicking beside a sketch is how you get at a background to fill or delete, and
/// scoping the flood to the ink turned that into a silent no-op.
#[test]
fn the_wand_selects_the_empty_space_beside_a_drawing() {
    let mut doc = board();
    let layer = doc.active_layer;
    paint_square(&mut doc, layer);
    doc.tool = Tool::MagicWand;
    click(&mut doc, 10.0, 10.0);
    let sel = doc.selection.as_ref().expect("a selection");
    assert!(sel.contains(10.5, 10.5), "the corner clicked");
    assert!(sel.contains(5.5, 100.5), "and the rest of the empty ground");
    assert!(!sel.contains(100.5, 100.5), "but not the painted square");
}

/// The other half of the same rule: a flood that starts *on* the ink stops at its edge rather
/// than running out into the empty canvas.
#[test]
fn a_flood_started_on_the_ink_stops_at_its_edge() {
    let mut doc = board();
    let layer = doc.active_layer;
    paint_square(&mut doc, layer);
    doc.tool = Tool::MagicWand;
    click(&mut doc, 100.0, 100.0);
    let sel = doc.selection.as_ref().expect("a selection");
    assert!(sel.contains(100.5, 100.5));
    assert!(!sel.contains(10.5, 10.5));
}

/// Colour range is the one that stays scoped to the painted box: it walks every pixel instead
/// of following a blob, so an unbounded walk would cost the canvas on every knob tick.
#[test]
fn colour_range_stays_scoped_to_the_ink_while_the_wand_does_not() {
    let mut doc = board();
    let layer = doc.active_layer;
    paint_square(&mut doc, layer);
    doc.tool = Tool::MagicWand;
    click(&mut doc, 10.0, 10.0);
    let wand = doc.selection.as_ref().unwrap().bounds();

    doc.tool = Tool::SelectColor;
    click(&mut doc, 100.0, 100.0);
    let range = doc.selection.as_ref().unwrap().bounds();
    assert!(
        wand.min_x < range.min_x,
        "the wand reached the canvas edge ({wand:?}) and the range did not ({range:?})"
    );
}

/// Ellipse gets the same treatment as Rect: the geometry hugs the layer's ink rather than
/// selecting the transparent corners of its own bounding box.
#[test]
fn ellipse_marquee_intersects_with_layer_ink() {
    let mut doc = board();
    let layer = doc.active_layer;
    paint_square(&mut doc, layer); // opaque 60..139 square
    doc.tool = Tool::SelectEllipse;
    // A circle inscribed in this box clips every corner of the painted square while its
    // centre stays well inside both shapes — the case a plain bounding-box intersect gets
    // wrong.
    drag_rect(&mut doc, (50.0, 50.0), (150.0, 150.0));
    let sel = doc.selection.as_ref().expect("a selection");
    assert!(matches!(sel.shape, SelectionShape::Mask(_)));
    assert!(
        sel.contains(100.5, 100.5),
        "inside both the ellipse and the ink"
    );
    assert!(
        !sel.contains(62.5, 62.5),
        "ink sits here, but past the ellipse's own curved boundary"
    );
    assert!(
        !sel.contains(10.5, 10.5),
        "outside the ellipse's box entirely"
    );
}

/// An ellipse on an empty layer is still just the geometry it drew — the same rule the rect
/// and lasso tests already pin, stated for the third shape.
#[test]
fn an_ellipse_on_an_empty_layer_keeps_its_own_shape() {
    let mut doc = board();
    doc.tool = Tool::SelectEllipse;
    drag_rect(&mut doc, (20.0, 20.0), (100.0, 100.0));
    let sel = doc.selection.as_ref().expect("a selection");
    assert!(matches!(sel.shape, SelectionShape::Ellipse { .. }));
}

/// None of the five select tools touch `History` — a selection is scoped state, not an edit to
/// the document's content, and undoing a paint stroke must not also quietly change what is
/// selected or vice versa.
#[test]
fn no_select_tool_pushes_an_undo_step() {
    let mut doc = board();
    let layer = doc.active_layer;
    paint_square(&mut doc, layer);
    assert!(
        !doc.history.can_undo(),
        "painting the fixture is not on the undo stack"
    );

    doc.tool = Tool::SelectRect;
    drag_rect(&mut doc, (0.0, 0.0), (200.0, 200.0));
    doc.tool = Tool::MagicWand;
    click(&mut doc, 100.0, 100.0);
    doc.tool = Tool::SelectColor;
    doc.set_select_color([200, 30, 30, 255]);
    click(&mut doc, 100.0, 100.0);
    doc.invert_selection();
    doc.select_all();
    doc.deselect();

    assert!(
        !doc.history.can_undo(),
        "a full lap of every select command left nothing to undo"
    );
}

/// `invert_selection` commits any open text session first, the way every non-text command
/// does — its canvas-selection behaviour must not be reached while the keyboard still belongs
/// to a run.
#[test]
fn invert_selection_commits_an_open_text_session_first() {
    let mut doc = board();
    doc.tool = Tool::Text;
    let (sx, sy) = doc.camera.to_screen(20.0, 20.0);
    doc.pointer_down(sx, sy);
    doc.pointer_up(sx, sy);
    doc.text_insert("hi");
    assert!(doc.text_editing());

    doc.invert_selection();
    assert!(!doc.text_editing(), "the session closed");
    assert!(
        doc.selection.is_some(),
        "and the canvas selection was built"
    );
}

/// A layer mask has to narrow what the wand can reach the same way it narrows what a click can
/// grab — `select_sample`'s whole reason to exist is that agreement. Painting a square and
/// masking most of it out leaves a small window the wand can select and a masked-out ring it
/// cannot, on the same layer's own painted pixels.
#[test]
fn the_wand_respects_a_layer_mask() {
    let mut doc = board();
    let layer = doc.active_layer;
    paint_square(&mut doc, layer); // 60..139
    let mut mask = vec![0u8; (DOC * DOC) as usize];
    for y in 90..110 {
        for x in 90..110 {
            mask[(y * DOC + x) as usize] = 255;
        }
    }
    doc.layers[layer].set_mask(Some(mask));

    doc.tool = Tool::MagicWand;
    click(&mut doc, 100.0, 100.0);
    let sel = doc.selection.as_ref().expect("a selection");
    assert!(sel.contains(100.5, 100.5), "inside the unmasked window");
    assert!(
        !sel.contains(65.5, 65.5),
        "painted, but masked out — must not read as ink"
    );
}

/// Paper is unchanged by any of this: opaque white everywhere, same as before slice A, so the
/// wand can still select the background of an otherwise empty document by clicking on it.
#[test]
fn select_tools_read_paper_as_solid_white() {
    let mut doc = board();
    let paper = (0..doc.layers.len())
        .find(|&i| doc.layers[i].is_paper())
        .expect("Paper");
    doc.active_layer = paper;
    doc.tool = Tool::MagicWand;
    click(&mut doc, 10.0, 10.0);
    let sel = doc.selection.as_ref().expect("a selection");
    assert!(sel.contains(10.5, 10.5));
    assert!(
        sel.contains((DOC as f32) - 1.0, (DOC as f32) - 1.0),
        "paper covers edge to edge"
    );
}
