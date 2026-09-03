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

#[test]
fn wand_on_text_layer_selects_painted_pixels() {
    let mut doc = board();
    doc.tool = Tool::Text;
    doc.pointer_down(40.0, 40.0);
    doc.text_insert("Hi");
    doc.commit_text();
    doc.tool = Tool::MagicWand;
    click(&mut doc, 42.0, 42.0);
    assert!(doc.selection.is_some());
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
