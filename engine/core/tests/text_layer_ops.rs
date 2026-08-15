use calumma_core::*;

/// What happens to a text layer when the *rest* of the document moves under it: layers
/// removed or merged, tools that paint, undo. A text layer's tiles are a cache of its run, so
/// every one of these has a way to go wrong that an ordinary raster layer does not have.
fn board() -> Document {
    let mut doc = Document::new("p".into(), "t", 512, 512);
    doc.resize_viewport(512.0, 512.0, 1.0);
    doc.fit_to_view();
    doc.tool = Tool::Text;
    doc.text_style.size = 48.0;
    doc
}

fn click(doc: &mut Document, x: f32, y: f32) {
    let (sx, sy) = doc.camera.to_screen(x, y);
    doc.pointer_down(sx, sy);
}

fn drag(doc: &mut Document, from: (f32, f32), to: (f32, f32)) {
    let (sx, sy) = doc.camera.to_screen(from.0, from.1);
    let (ex, ey) = doc.camera.to_screen(to.0, to.1);
    doc.pointer_down(sx, sy);
    doc.pointer_move(ex, ey);
    doc.pointer_up(ex, ey);
}

fn ink(doc: &Document, index: usize) -> usize {
    let Some(grid) = doc.layers[index].tiles() else {
        return 0;
    };
    (0..doc.height as i32)
        .flat_map(|y| (0..doc.width as i32).map(move |x| (x, y)))
        .filter(|(x, y)| grid.get_pixel(*x, *y)[3] > 0)
        .count()
}

fn typed(doc: &mut Document, text: &str) -> usize {
    click(doc, 100.0, 100.0);
    doc.text_insert(text);
    doc.commit_text();
    doc.layers.len() - 1
}

#[test]
fn emptying_an_existing_text_layer_stays_undoable() {
    let mut doc = board();
    let layer = typed(&mut doc, "hello");
    let painted = ink(&doc, layer);
    assert!(painted > 0);

    assert!(doc.edit_text_layer(layer));
    for _ in 0..5 {
        doc.text_backspace();
    }
    doc.commit_text();
    assert_eq!(ink(&doc, layer), 0, "the glyphs are gone");
    assert_eq!(doc.layers.len() - 1, layer, "the layer itself stays");
    assert_eq!(doc.history.undo_depth(), 2, "deleting the text is a step");

    doc.undo();
    assert_eq!(ink(&doc, layer), painted, "undo brings the glyphs back");
}

#[test]
fn removing_a_layer_ends_the_session_instead_of_stranding_it() {
    let mut doc = board();
    click(&mut doc, 100.0, 100.0);
    doc.text_insert("keep");
    doc.remove_layer(1);
    assert!(!doc.text_editing(), "the session cannot outlive the stack");
    assert!(doc.text_caret_segment().is_none());

    let layer = doc.layers.iter().position(Layer::is_text).unwrap();
    assert_eq!(doc.layers[layer].run().unwrap().text, "keep");
    doc.text_insert("!!");
    assert_eq!(
        doc.layers[layer].run().unwrap().text,
        "keep",
        "typing after the session ended must not reach a layer by index"
    );
}

#[test]
fn switching_the_active_layer_commits_the_session() {
    let mut doc = board();
    click(&mut doc, 100.0, 100.0);
    doc.text_insert("done");
    doc.set_active_layer(0);
    assert!(!doc.text_editing());
    assert_eq!(doc.history.undo_depth(), 1);
}

#[test]
fn undo_commits_the_session_before_it_walks_back() {
    let mut doc = board();
    let layer = typed(&mut doc, "first");
    let one_word = ink(&doc, layer);
    assert!(doc.edit_text_layer(layer));
    doc.text_insert(" second");
    doc.undo();
    assert!(!doc.text_editing(), "undo ends the session it would fight");
    assert_eq!(
        doc.layers[layer].run().unwrap().text,
        "first",
        "undo takes back what was typed, not only what was painted"
    );
    assert_eq!(ink(&doc, layer), one_word);
    doc.redo();
    assert_eq!(doc.layers[layer].run().unwrap().text, "first second");
    assert!(ink(&doc, layer) > one_word);
}

/// The run is what a project stores — its tiles are rebuilt on open. An undo that repainted
/// the old glyphs but left the new string behind would come back on the next launch.
#[test]
fn undo_leaves_the_run_and_the_pixels_agreeing() {
    let mut doc = board();
    let layer = typed(&mut doc, "kept");
    assert!(doc.edit_text_layer(layer));
    doc.text_insert(" and more");
    doc.commit_text();
    doc.undo();

    let run = doc.layers[layer].run().unwrap().clone();
    assert_eq!(run.text, "kept");
    let painted = ink(&doc, layer);
    assert!(calumma_core::text_layer::resync(&mut doc.layers[layer]));
    assert_eq!(
        ink(&doc, layer),
        painted,
        "re-rasterizing the restored run must change nothing"
    );
}

#[test]
fn paint_tools_are_refused_on_a_text_layer() {
    let mut doc = board();
    let layer = typed(&mut doc, "paint");
    let glyphs = ink(&doc, layer);
    doc.set_active_layer(layer);
    assert!(!doc.active_layer_accepts_paint());

    doc.tool = Tool::Pen;
    drag(&mut doc, (300.0, 300.0), (400.0, 400.0));
    assert_eq!(ink(&doc, layer), glyphs, "a stroke must not land on glyphs");

    doc.tool = Tool::Rect;
    drag(&mut doc, (300.0, 300.0), (400.0, 400.0));
    assert_eq!(ink(&doc, layer), glyphs, "nor a shape");

    doc.tool = Tool::Fill;
    click(&mut doc, 350.0, 350.0);
    assert_eq!(ink(&doc, layer), glyphs, "nor a fill");

    doc.clear_active_layer();
    assert_eq!(ink(&doc, layer), glyphs, "nor a clear");
    assert_eq!(doc.history.undo_depth(), 1, "a refused paint is not a step");
    assert!(doc.layers[layer].is_text(), "the run is still editable");
}

#[test]
fn rasterizing_gives_the_pixels_up_to_the_paint_tools() {
    let mut doc = board();
    let layer = typed(&mut doc, "rasterize");
    let glyphs = ink(&doc, layer);
    doc.set_active_layer(layer);

    assert!(doc.rasterize_text_layer(layer));
    assert!(!doc.layers[layer].is_text(), "the run is gone");
    assert!(doc.layers[layer].run().is_none());
    assert_eq!(ink(&doc, layer), glyphs, "the glyphs stay, as pixels");
    assert!(doc.active_layer_accepts_paint());

    doc.tool = Tool::Pen;
    drag(&mut doc, (300.0, 300.0), (400.0, 400.0));
    assert!(ink(&doc, layer) > glyphs, "paint lands now");

    assert!(!doc.rasterize_text_layer(layer), "only once");
    assert!(!doc.rasterize_text_layer(0), "paper is not text");
    assert!(!doc.rasterize_text_layer(99));
}

#[test]
fn rasterizing_the_layer_being_typed_into_ends_the_session_first() {
    let mut doc = board();
    click(&mut doc, 100.0, 100.0);
    doc.text_insert("live");
    let layer = doc.text_edit_layer().unwrap();
    assert!(doc.rasterize_text_layer(layer));
    assert!(!doc.text_editing());
    assert!(ink(&doc, layer) > 0, "what was typed is kept");
    assert_eq!(doc.history.undo_depth(), 1);
}

#[test]
fn merging_onto_a_text_layer_keeps_the_merged_pixels() {
    let mut doc = board();
    let layer = typed(&mut doc, "merge");
    doc.duplicate_layer(layer);
    let copy = doc.layers.len() - 1;
    doc.layers[copy].run_offset_for_test();
    let merged_total = ink(&doc, layer) + ink(&doc, copy);

    assert!(doc.merge_layer_down(copy));
    let destination = doc.layers.len() - 1;
    assert!(
        !doc.layers[destination].is_text(),
        "a merged-into text layer becomes pixels, or the next keystroke erases the merge"
    );
    assert!(ink(&doc, destination) > 0);
    assert!(ink(&doc, destination) <= merged_total);
}

#[test]
fn duplicating_a_text_layer_gives_two_editable_runs() {
    let mut doc = board();
    let layer = typed(&mut doc, "twin");
    assert!(doc.duplicate_layer(layer));
    let copy = doc.layers.len() - 1;
    assert!(doc.layers[copy].is_text());

    assert!(doc.edit_text_layer(copy));
    doc.text_insert("s");
    doc.commit_text();
    assert_eq!(doc.layers[copy].run().unwrap().text, "twins");
    assert_eq!(
        doc.layers[layer].run().unwrap().text,
        "twin",
        "the original keeps its own run"
    );
}

trait RunOffsetForTest {
    fn run_offset_for_test(&mut self);
}

/// Nudges a duplicate off the original so the two do not paint the same pixels, which is what
/// makes "the merge kept both" a claim worth checking.
impl RunOffsetForTest for Layer {
    fn run_offset_for_test(&mut self) {
        if let Some(run) = self.content.run_mut() {
            run.origin.1 += 120.0;
        }
        calumma_core::text_layer::resync(self);
    }
}
