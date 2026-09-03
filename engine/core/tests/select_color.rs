//! Select by colour — Photoshop's Color Range, as far as this engine takes it.
//!
//! Three rules carry the tool and each has a test here: the match colour is a *stored* swatch
//! rather than whatever was clicked last, the tolerance is that dialog's Fuzziness and so
//! re-runs live, and the walk is scoped to the active layer's ink so Paper white never floods
//! through the empty tiles above it.

use calumma_core::*;

const DOC: u32 = 128;
const RED: [u8; 4] = [200, 30, 30, 255];
const BLUE: [u8; 4] = [30, 30, 200, 255];

fn board() -> Document {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.resize_viewport(DOC as f32, DOC as f32, 1.0);
    doc.fit_to_view();
    doc.tool = Tool::SelectColor;
    doc.set_tolerance(8);
    doc
}

fn fill(doc: &mut Document, rect: DocRect, color: [u8; 4]) {
    let layer = doc.active_layer;
    doc.layers[layer]
        .tiles_mut()
        .expect("a raster layer")
        .fill_uniform(rect, color);
}

/// Two red blobs with a gap, and one blue one, so contiguity and colour can be told apart.
fn three_blobs() -> Document {
    let mut doc = board();
    fill(&mut doc, DocRect::new(10, 10, 30, 30), RED);
    fill(&mut doc, DocRect::new(90, 10, 110, 30), RED);
    fill(&mut doc, DocRect::new(50, 90, 70, 110), BLUE);
    doc
}

fn click(doc: &mut Document, x: f32, y: f32) {
    let (sx, sy) = doc.camera.to_screen(x, y);
    doc.pointer_down(sx, sy);
    doc.pointer_up(sx, sy);
}

#[track_caller]
fn selected(doc: &Document, x: f32, y: f32) -> bool {
    doc.selection
        .as_ref()
        .expect("a selection")
        .contains(x + 0.5, y + 0.5)
}

#[test]
fn a_click_takes_every_matching_blob_not_just_the_one_under_it() {
    let mut doc = three_blobs();
    click(&mut doc, 20.0, 20.0);
    assert!(selected(&doc, 20.0, 20.0), "the blob clicked");
    assert!(selected(&doc, 100.0, 20.0), "and the one across the canvas");
    assert!(!selected(&doc, 60.0, 100.0), "but not the blue one");
}

/// The wand is the contiguous half of the same question, and the difference between the two is
/// the whole reason this tool exists.
#[test]
fn the_wand_takes_only_the_blob_it_landed_on() {
    let mut doc = three_blobs();
    doc.tool = Tool::MagicWand;
    click(&mut doc, 20.0, 20.0);
    assert!(selected(&doc, 20.0, 20.0));
    assert!(
        !selected(&doc, 100.0, 20.0),
        "the far blob is a separate blob"
    );
}

#[test]
fn a_click_samples_into_the_match_swatch() {
    let mut doc = three_blobs();
    doc.set_select_color([1, 2, 3, 255]);
    click(&mut doc, 60.0, 100.0);
    assert_eq!(doc.select_color(), BLUE);
}

/// The swatch is the match colour, not a note about one: ringing it re-runs the selection over
/// the layer that is already open, which is what Color Range does when you re-sample.
#[test]
fn changing_the_match_swatch_re_runs_the_selection() {
    let mut doc = three_blobs();
    click(&mut doc, 60.0, 100.0);
    assert!(selected(&doc, 60.0, 100.0), "blue first");

    doc.set_select_color(RED);
    assert!(selected(&doc, 20.0, 20.0), "both red blobs now");
    assert!(selected(&doc, 100.0, 20.0));
    assert!(!selected(&doc, 60.0, 100.0), "and no longer the blue one");
}

/// Tolerance is Fuzziness for this tool alone. Widening it far enough makes every colour on the
/// layer one match.
#[test]
fn widening_the_tolerance_re_runs_and_opens_the_selection_up() {
    let mut doc = three_blobs();
    click(&mut doc, 20.0, 20.0);
    assert!(!selected(&doc, 60.0, 100.0));

    doc.set_tolerance(255);
    assert!(
        selected(&doc, 60.0, 100.0),
        "at full tolerance every painted pixel matches"
    );
}

#[test]
fn tolerance_zero_takes_only_the_exact_colour() {
    let mut doc = board();
    fill(&mut doc, DocRect::new(10, 10, 30, 30), RED);
    fill(&mut doc, DocRect::new(50, 10, 70, 30), [205, 30, 30, 255]);
    doc.set_tolerance(0);
    click(&mut doc, 20.0, 20.0);
    assert!(selected(&doc, 20.0, 20.0));
    assert!(
        !selected(&doc, 60.0, 20.0),
        "five levels off is not a match"
    );
}

/// The knob is shared with the bucket and the wand, and only this tool treats a turn of it as a
/// question about the selection already on screen.
#[test]
fn the_same_tolerance_knob_leaves_a_wand_selection_alone() {
    let mut doc = three_blobs();
    doc.tool = Tool::MagicWand;
    click(&mut doc, 20.0, 20.0);
    let before = doc.selection.clone();

    doc.set_tolerance(255);
    assert_eq!(
        doc.selection.is_some(),
        before.is_some(),
        "the wand applies a new tolerance to its next click, not to its last one"
    );
    assert!(!selected(&doc, 60.0, 100.0));
}

#[test]
fn a_knob_set_to_what_it_already_was_changes_nothing() {
    let mut doc = three_blobs();
    click(&mut doc, 20.0, 20.0);
    doc.selection = None;
    doc.set_tolerance(doc.tolerance);
    doc.set_select_color(doc.select_color());
    assert!(
        doc.selection.is_none(),
        "an idempotent set must not rebuild a selection that was deliberately dropped"
    );
}

/// The scope is the layer's own ink. Paper sits underneath with white in every tile, and a
/// colour range on the layer above must never reach through it.
#[test]
fn the_walk_is_scoped_to_the_active_layers_ink() {
    let mut doc = board();
    fill(&mut doc, DocRect::new(10, 10, 30, 30), RED);
    doc.set_select_color(RED);
    click(&mut doc, 20.0, 20.0);
    let mask_bounds = doc.selection.as_ref().unwrap().bounds();
    assert!(
        mask_bounds.min_x >= 10 && mask_bounds.max_x <= 30,
        "the mask reached past the ink: {mask_bounds:?}"
    );
}

#[test]
fn the_swatch_only_re_runs_while_the_tool_is_in_hand() {
    let mut doc = three_blobs();
    click(&mut doc, 20.0, 20.0);
    doc.tool = Tool::Pen;
    doc.set_select_color(BLUE);
    assert!(
        selected(&doc, 20.0, 20.0),
        "a swatch changed with another tool in hand leaves the selection alone"
    );
}

#[test]
fn a_click_off_the_ink_leaves_the_selection_alone() {
    let mut doc = three_blobs();
    click(&mut doc, 20.0, 20.0);
    click(&mut doc, 126.0, 126.0);
    assert!(
        selected(&doc, 20.0, 20.0),
        "a click past the layer's painted box is a miss, not a deselect"
    );
}

#[test]
fn select_color_reaches_a_vector_layer() {
    let mut doc = board();
    doc.add_vector_layer(
        "V",
        vector::VectorItem::Shape(vector::VectorShape {
            shape: Shape {
                tool: Tool::Rect,
                start: (40.0, 40.0),
                end: (90.0, 90.0),
                half_width: 1.0,
                fill: true,
                stroke: false,
            },
            color: BLUE,
            stroke_color: BLUE,
        }),
    );
    click(&mut doc, 60.0, 60.0);
    assert_eq!(doc.select_color(), BLUE);
    assert!(selected(&doc, 60.0, 60.0));
    assert!(
        doc.layers[doc.active_layer].content.item().is_some(),
        "sampling must not rasterize the layer it read"
    );
}

#[test]
fn select_color_reaches_a_text_layer() {
    let mut doc = board();
    doc.tool = Tool::Text;
    doc.text_style.size = 64.0;
    let (sx, sy) = doc.camera.to_screen(20.0, 60.0);
    doc.pointer_down(sx, sy);
    doc.pointer_up(sx, sy);
    doc.text_insert("HH");
    doc.commit_text();
    doc.tool = Tool::SelectColor;
    doc.set_tolerance(40);

    let ink = doc.layers[doc.active_layer]
        .content_bounds()
        .expect("glyphs");
    let mut hit = None;
    for y in (ink.1 as i32)..(ink.3 as i32) {
        for x in (ink.0 as i32)..(ink.2 as i32) {
            if doc.layers[doc.active_layer]
                .tiles()
                .unwrap()
                .get_pixel(x, y)[3]
                == 255
            {
                hit = Some((x, y));
                break;
            }
        }
        if hit.is_some() {
            break;
        }
    }
    let (hx, hy) = hit.expect("an opaque glyph pixel");
    click(&mut doc, hx as f32 + 0.5, hy as f32 + 0.5);
    assert!(selected(&doc, hx as f32, hy as f32));
    assert!(
        doc.layers[doc.active_layer].run().is_some(),
        "the run stays editable"
    );
}

/// Where the two shipped plans meet. A style span is only real if it reaches the pixels, and
/// colour range only works if it reads the pixels a text layer actually has — so colouring one
/// word and then selecting by that colour proves both ends at once.
#[test]
fn a_coloured_word_can_be_selected_by_its_colour() {
    let mut doc = board();
    doc.tool = Tool::Text;
    doc.text_style.size = 64.0;
    doc.color = [0, 0, 0, 255];
    let (sx, sy) = doc.camera.to_screen(10.0, 60.0);
    doc.pointer_down(sx, sy);
    doc.pointer_up(sx, sy);
    doc.text_insert("AB");

    doc.text_step_caret(Step::Left, true);
    doc.color = BLUE;
    doc.apply_ink_to_text();
    doc.commit_text();

    let layer = doc.active_layer;
    let grid = doc.layers[layer].tiles().expect("a tile cache");
    let ink = doc.layers[layer].content_bounds().expect("glyphs");
    let mut blue_pixel = None;
    for y in (ink.1 as i32)..(ink.3 as i32) {
        for x in (ink.0 as i32)..(ink.2 as i32) {
            let px = grid.get_pixel(x, y);
            if px[3] == 255 && px[2] > 150 && px[0] < 100 {
                blue_pixel = Some((x, y));
                break;
            }
        }
        if blue_pixel.is_some() {
            break;
        }
    }
    let (bx, by) = blue_pixel.expect("the second glyph rasterized in the span's colour");

    doc.tool = Tool::SelectColor;
    doc.set_tolerance(30);
    click(&mut doc, bx as f32 + 0.5, by as f32 + 0.5);
    assert!(selected(&doc, bx as f32, by as f32));
    assert!(
        doc.layers[layer].run().is_some(),
        "selecting by colour left the run editable"
    );
}

/// The two paths differ on purpose. A *click* that lands off the ink is a mis-aim and leaves
/// the selection alone; a *re-run* driven by a knob is live feedback, so a swatch that matches
/// nothing empties the selection — and moving the knob back brings it straight back, because
/// the re-run reads the swatch rather than what is currently selected.
#[test]
fn a_re_run_that_matches_nothing_empties_and_then_recovers() {
    let mut doc = three_blobs();
    doc.set_tolerance(0);
    click(&mut doc, 20.0, 20.0);
    assert!(selected(&doc, 20.0, 20.0));

    // Near the red, but not it: no exact match at fuzziness zero, well inside reach above it.
    doc.set_select_color([180, 50, 50, 255]);
    assert!(
        doc.selection.is_none(),
        "no pixel on the layer is exactly that colour"
    );

    doc.set_tolerance(64);
    assert!(
        selected(&doc, 20.0, 20.0),
        "widening the fuzziness brings it back"
    );
}
