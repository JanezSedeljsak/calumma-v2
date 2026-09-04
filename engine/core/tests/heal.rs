use calumma_core::document::*;
use calumma_core::*;

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

/// A uniform blue field with a small, uniform red "blemish" — the shape the healing brush
/// exists for. Source and destination are both on the same field, so healing from a clean patch
/// should erase the blemish rather than paint a visible patch of blue over it.
fn blemished_board() -> Document {
    let mut doc = Document::new("p".into(), "t", 256, 256);
    doc.resize_viewport(256.0, 256.0, 1.0);
    doc.fit_to_view();
    let tiles = doc.layers[doc.active_layer].tiles_mut().unwrap();
    tiles.fill_uniform(DocRect::new(0, 0, 255, 255), [40, 60, 220, 255]);
    tiles.fill_uniform(DocRect::new(126, 126, 129, 129), [220, 40, 40, 255]);
    doc
}

#[test]
fn heal_erases_a_small_blemish_against_a_clean_source() {
    let mut doc = blemished_board();
    doc.tool = Tool::Heal;
    doc.brush_size = 40.0;
    // A clean, uniform patch of the same field, far enough from the blemish to carry none of
    // it into the blur margin.
    doc.set_clone_anchor(20.0, 20.0);

    let before = pixel(&doc, 128, 128);
    assert_eq!(before, [220, 40, 40, 255], "the blemish, to start");

    drag(&mut doc, (128.0, 128.0), (128.0, 128.0));

    let after = pixel(&doc, 128, 128);
    assert!(
        after[0] < before[0] && after[2] > before[2],
        "healed back toward the field's blue rather than staying red: {after:?}"
    );
}

/// The split is `src − blur(src) + blur(dst)`: a source with no texture of its own (uniform)
/// contributes nothing but the destination's own blurred surroundings, so a small blemish
/// heals almost completely rather than merely fading.
#[test]
fn a_uniform_source_heals_a_small_blemish_almost_completely() {
    let mut doc = blemished_board();
    doc.tool = Tool::Heal;
    doc.brush_size = 40.0;
    doc.set_clone_anchor(20.0, 20.0);

    drag(&mut doc, (128.0, 128.0), (128.0, 128.0));

    let after = pixel(&doc, 128, 128);
    assert!(
        after[0] < 80 && after[2] > 180,
        "close to the field's own blue, not a lingering red: {after:?}"
    );
}

/// The texture half of the split: a source with detail of its own carries that detail into an
/// otherwise-flat destination, which is what keeps a healed patch of skin looking like skin
/// instead of an airbrushed smudge.
#[test]
fn heal_carries_the_sources_own_texture_into_a_flat_destination() {
    let mut doc = Document::new("p".into(), "t", 256, 256);
    doc.resize_viewport(256.0, 256.0, 1.0);
    doc.fit_to_view();
    let tiles = doc.layers[doc.active_layer].tiles_mut().unwrap();
    tiles.fill_uniform(DocRect::new(0, 0, 255, 255), [128, 128, 128, 255]);
    // A striped source: alternating light/dark columns two pixels wide, well away from the
    // flat destination the stroke heals into.
    for x in (0..40).step_by(2) {
        tiles.fill_uniform(DocRect::new(x, 0, x, 255), [40, 40, 40, 255]);
    }

    doc.tool = Tool::Heal;
    doc.brush_size = 30.0;
    doc.set_clone_anchor(20.0, 128.0);

    drag(&mut doc, (150.0, 128.0), (150.0, 128.0));

    let a = pixel(&doc, 148, 128)[0] as i32;
    let b = pixel(&doc, 149, 128)[0] as i32;
    assert!(
        (a - b).abs() > 20,
        "neighbouring healed pixels should still show the source's stripes: {a} vs {b}"
    );
}

/// Painting with no source set is a no-op, like the clone stamp.
#[test]
fn heal_with_no_source_changes_nothing() {
    let mut doc = blemished_board();
    doc.tool = Tool::Heal;
    doc.brush_size = 40.0;

    drag(&mut doc, (128.0, 128.0), (128.0, 128.0));

    assert_eq!(pixel(&doc, 128, 128), [220, 40, 40, 255]);
    assert!(!doc.history.can_undo(), "and left no undo step behind");
}

/// Heal commits as the pointer moves, exactly like blur and clone — a stroke is one undo.
#[test]
fn a_whole_heal_stroke_is_one_undo() {
    let mut doc = blemished_board();
    doc.tool = Tool::Heal;
    doc.brush_size = 40.0;
    doc.set_clone_anchor(20.0, 20.0);

    drag(&mut doc, (128.0, 128.0), (128.0, 128.0));
    assert_ne!(pixel(&doc, 128, 128), [220, 40, 40, 255], "healed");

    doc.undo();
    assert_eq!(
        pixel(&doc, 128, 128),
        [220, 40, 40, 255],
        "blemish restored"
    );
    assert!(!doc.history.can_undo(), "one entry for the whole stroke");
}

/// Every other paint tool refuses a text layer; the healing brush is no different.
#[test]
fn heal_refuses_a_text_layer() {
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

    doc.tool = Tool::Heal;
    doc.brush_size = 24.0;
    doc.set_clone_anchor(10.0, 10.0);
    let before = doc.layers[text_index].tiles().unwrap().clone();

    drag(&mut doc, (60.0, 60.0), (90.0, 60.0));

    assert_eq!(
        &before,
        doc.layers[text_index].tiles().unwrap(),
        "a text layer's cached glyph tiles were left alone"
    );
}
