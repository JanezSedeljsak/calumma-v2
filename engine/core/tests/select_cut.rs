//! Copy works on every layer kind; clearing the selected pixels only works on the ones that
//! have pixels to clear. Slice A's stated minimum was "select and copy working everywhere",
//! and left cut/delete on vector and text layers as a deliberate open question — this file
//! pins what actually happens today: a cut copies and leaves the layer untouched.

use calumma_core::*;

const DOC: u32 = 128;
const RED: [u8; 4] = [200, 30, 30, 255];

fn board() -> Document {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.resize_viewport(DOC as f32, DOC as f32, 1.0);
    doc.fit_to_view();
    doc
}

fn rect_item() -> vector::VectorItem {
    vector::VectorItem::Shape(vector::VectorShape {
        shape: Shape {
            tool: Tool::Rect,
            start: (40.0, 40.0),
            end: (90.0, 90.0),
            half_width: 1.0,
            fill: true,
            stroke: false,
        },
        color: RED,
        stroke_color: RED,
    })
}

fn click(doc: &mut Document, x: f32, y: f32) {
    let (sx, sy) = doc.camera.to_screen(x, y);
    doc.pointer_down(sx, sy);
    doc.pointer_up(sx, sy);
}

#[test]
fn clearing_a_selection_on_a_raster_layer_actually_clears_it() {
    let mut doc = board();
    let layer = doc.active_layer;
    doc.layers[layer]
        .tiles_mut()
        .unwrap()
        .fill_uniform(DocRect::new(10, 10, 30, 30), RED);
    doc.tool = Tool::MagicWand;
    click(&mut doc, 20.0, 20.0);
    assert!(doc.clear_selection_pixels(), "the positive control clears");
    assert_eq!(
        doc.layers[layer].tiles().unwrap().get_pixel(20, 20)[3],
        0,
        "the pixel is gone"
    );
}

/// The item is what makes copy possible at all here (`select_sample` evaluates it directly),
/// and it is exactly what a clear must not touch — a vector layer has no tiles to clear
/// pixels *from*.
#[test]
fn cutting_a_selection_on_a_vector_layer_copies_and_leaves_the_item_alone() {
    let mut doc = board();
    doc.add_vector_layer("V", rect_item());
    let layer = doc.active_layer;
    doc.tool = Tool::MagicWand;
    click(&mut doc, 60.0, 60.0);
    assert!(doc.selection.is_some(), "the shape was selected");

    let (_, _, buf) = doc.selection_rgba().expect("copy still works");
    assert!(
        buf.chunks_exact(4).any(|px| px[3] > 0),
        "and it copied pixels"
    );

    assert!(
        !doc.clear_selection_pixels(),
        "clearing refuses a layer with nothing to clear pixels from"
    );
    assert!(
        doc.layers[layer].content.item().is_some(),
        "the item is exactly as it was"
    );
    assert_eq!(doc.layers[layer].content.item(), Some(&rect_item()));
}

/// Same shape of answer on a text layer: the run stays editable, and cut is copy-only.
#[test]
fn cutting_a_selection_on_a_text_layer_copies_and_leaves_the_run_alone() {
    let mut doc = board();
    doc.tool = Tool::Text;
    doc.text_style.size = 64.0;
    let (sx, sy) = doc.camera.to_screen(20.0, 60.0);
    doc.pointer_down(sx, sy);
    doc.pointer_up(sx, sy);
    doc.text_insert("Hi");
    doc.commit_text();
    let layer = doc.active_layer;
    let before = doc.layers[layer].run().unwrap().text.clone();

    doc.tool = Tool::MagicWand;
    let ink = doc.layers[layer].content_bounds().expect("glyphs");
    click(&mut doc, ink.0 + 1.0, (ink.1 + ink.3) / 2.0);
    assert!(doc.selection.is_some());

    let (_, _, buf) = doc.selection_rgba().expect("copy still works");
    assert!(buf.chunks_exact(4).any(|px| px[3] > 0));

    assert!(!doc.clear_selection_pixels(), "text has no tiles to clear");
    assert_eq!(doc.layers[layer].run().unwrap().text, before);
}
