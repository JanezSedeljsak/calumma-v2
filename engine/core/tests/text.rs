use calumma_core::*;

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

fn ink_pixels(doc: &Document, index: usize) -> usize {
    let Some(grid) = doc.layers[index].tiles() else {
        return 0;
    };
    let mut count = 0;
    for y in 0..doc.height as i32 {
        for x in 0..doc.width as i32 {
            if grid.get_pixel(x, y)[3] > 0 {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn clicking_with_the_text_tool_opens_a_new_text_layer() {
    let mut doc = board();
    let before = doc.layers.len();
    click(&mut doc, 100.0, 100.0);
    assert_eq!(doc.layers.len(), before + 1);
    assert!(doc.text_editing());
    assert!(doc.layers[doc.active_layer].is_text());
    assert_eq!(doc.text_caret(), Some(0));
}

#[test]
fn typing_paints_immediately() {
    let mut doc = board();
    click(&mut doc, 100.0, 100.0);
    let layer = doc.active_layer;
    assert_eq!(ink_pixels(&doc, layer), 0);
    doc.text_insert("A");
    assert!(
        ink_pixels(&doc, layer) > 0,
        "text should rasterize on insert"
    );
    assert_eq!(doc.text_caret(), Some(1));
}

#[test]
fn each_keystroke_re_rasterizes_the_layer() {
    let mut doc = board();
    click(&mut doc, 40.0, 100.0);
    let layer = doc.active_layer;
    doc.text_insert("i");
    let one = ink_pixels(&doc, layer);
    doc.text_insert("iiii");
    let five = ink_pixels(&doc, layer);
    assert!(five > one);
    doc.text_backspace();
    doc.text_backspace();
    doc.text_backspace();
    doc.text_backspace();
    assert_eq!(ink_pixels(&doc, layer), one, "shrinking must clear old ink");
}

#[test]
fn backspace_and_delete_edit_at_the_caret() {
    let mut doc = board();
    click(&mut doc, 60.0, 100.0);
    doc.text_insert("abcd");
    doc.text_step_caret(Step::Left, false);
    doc.text_backspace();
    assert_eq!(doc.active_text_run().unwrap().text, "abd");
    doc.text_delete_forward();
    assert_eq!(doc.active_text_run().unwrap().text, "ab");
    doc.text_step_caret(Step::DocStart, false);
    doc.text_delete_forward();
    assert_eq!(doc.active_text_run().unwrap().text, "b");
    doc.text_backspace();
    assert_eq!(doc.active_text_run().unwrap().text, "b");
}

#[test]
fn newlines_are_ordinary_inserts() {
    let mut doc = board();
    click(&mut doc, 60.0, 60.0);
    doc.text_insert("one\ntwo");
    assert_eq!(doc.active_text_run().unwrap().text, "one\ntwo");
    let (_, _, _, y1) = doc.text_box().unwrap();
    assert!(y1 - 60.0 > 48.0, "two lines should be taller than one");
}

#[test]
fn committing_empty_text_leaves_no_layer_behind() {
    let mut doc = board();
    let before = doc.layers.len();
    click(&mut doc, 100.0, 100.0);
    doc.commit_text();
    assert_eq!(doc.layers.len(), before);
    assert!(!doc.text_editing());
    assert!(!doc.history.can_undo());
}

#[test]
fn a_whole_session_is_one_undo_step() {
    let mut doc = board();
    click(&mut doc, 100.0, 100.0);
    let layer = doc.active_layer;
    doc.text_insert("Hello");
    doc.text_insert(" there");
    assert!(!doc.history.can_undo(), "no history until the session ends");
    doc.commit_text();
    assert_eq!(doc.history.undo_depth(), 1);
    assert!(ink_pixels(&doc, layer) > 0);
    doc.undo();
    assert_eq!(ink_pixels(&doc, layer), 0, "undo clears the whole run");
    doc.redo();
    assert!(ink_pixels(&doc, layer) > 0);
}

#[test]
fn re_entering_keeps_the_text_and_starts_a_fresh_step() {
    let mut doc = board();
    click(&mut doc, 100.0, 100.0);
    doc.text_insert("hi");
    doc.commit_text();
    let layer = doc.active_layer;

    click(&mut doc, 104.0, 100.0);
    assert!(doc.text_editing());
    assert_eq!(doc.text_edit_layer(), Some(layer));
    assert_eq!(doc.layers.len(), 3, "re-entering must not add a layer");
    doc.text_step_caret(Step::DocEnd, false);
    doc.text_insert("!");
    doc.commit_text();
    assert_eq!(doc.active_text_run().unwrap().text, "hi!");
    assert_eq!(doc.history.undo_depth(), 2);
}

#[test]
fn clicking_empty_board_starts_a_second_text_layer() {
    let mut doc = board();
    click(&mut doc, 40.0, 40.0);
    doc.text_insert("one");
    click(&mut doc, 40.0, 400.0);
    doc.text_insert("two");
    doc.commit_text();
    let texts: Vec<&str> = doc
        .layers
        .iter()
        .filter_map(|l| l.run().map(|r| r.text.as_str()))
        .collect();
    assert_eq!(texts, vec!["one", "two"]);
}

#[test]
fn caret_lands_where_the_click_did() {
    let mut doc = board();
    click(&mut doc, 40.0, 100.0);
    doc.text_insert("wide text here");
    doc.commit_text();
    let (x0, y0, x1, y1) = {
        let run = doc.layers.last().unwrap().run().unwrap();
        calumma_core::text_layer::run_box(run)
    };
    click(&mut doc, (x0 + x1) * 0.5, (y0 + y1) * 0.5);
    let caret = doc.text_caret().unwrap();
    assert!(
        caret > 0 && caret < 14,
        "caret should land mid-string, got {caret}"
    );
}

#[test]
fn another_tool_commits_the_session() {
    let mut doc = board();
    click(&mut doc, 100.0, 100.0);
    doc.text_insert("kept");
    doc.tool = Tool::Pen;
    click(&mut doc, 300.0, 300.0);
    assert!(!doc.text_editing());
    assert_eq!(
        doc.layers.iter().filter_map(Layer::run).count(),
        1,
        "the text layer survives the tool switch"
    );
}

#[test]
fn deselect_ends_the_session() {
    let mut doc = board();
    click(&mut doc, 100.0, 100.0);
    doc.text_insert("x");
    doc.deselect();
    assert!(!doc.text_editing());
}

#[test]
fn ink_color_recolors_the_live_run() {
    let mut doc = board();
    click(&mut doc, 100.0, 100.0);
    doc.text_insert("A");
    doc.set_color([255, 0, 0, 255]);
    assert_eq!(doc.active_text_run().unwrap().color, [255, 0, 0, 255]);
    let grid = doc.layers[doc.active_layer].tiles().unwrap();
    let painted = (0..doc.height as i32)
        .flat_map(|y| (0..doc.width as i32).map(move |x| (x, y)))
        .map(|(x, y)| grid.get_pixel(x, y))
        .find(|px| px[3] == 255);
    assert_eq!(painted, Some([255, 0, 0, 255]));
}

#[test]
fn size_and_family_changes_reflow_the_run() {
    let mut doc = board();
    click(&mut doc, 60.0, 200.0);
    doc.text_insert("size");
    let small = doc.text_box().unwrap();
    doc.set_text_size(120.0);
    let large = doc.text_box().unwrap();
    assert!(large.2 - large.0 > small.2 - small.0);
    assert_eq!(doc.active_text_run().unwrap().size, 120.0);

    let other = font_families()
        .into_iter()
        .find(|f| f != &doc.active_text_run().unwrap().family)
        .expect("more than one system font");
    doc.set_text_family(&other);
    assert_eq!(doc.active_text_run().unwrap().family, other);
}

#[test]
fn style_changes_carry_to_the_next_text_layer() {
    let mut doc = board();
    click(&mut doc, 60.0, 60.0);
    doc.set_text_size(96.0);
    doc.set_text_align(TextAlign::Center);
    doc.text_insert("first");
    doc.commit_text();
    click(&mut doc, 60.0, 300.0);
    let run = doc.active_text_run().unwrap();
    assert_eq!(run.size, 96.0);
    assert_eq!(run.align, TextAlign::Center);
}

#[test]
fn out_of_range_sizes_are_clamped_by_the_engine() {
    let mut doc = board();
    click(&mut doc, 60.0, 60.0);
    doc.set_text_size(10_000.0);
    assert_eq!(doc.active_text_run().unwrap().size, TEXT_SIZE_MAX);
    doc.set_text_size(0.0);
    assert_eq!(doc.active_text_run().unwrap().size, TEXT_SIZE_MIN);
    doc.set_text_size(f32::NAN);
    assert!(doc.active_text_run().unwrap().size.is_finite());
}

#[test]
fn a_composition_shows_before_it_is_committed() {
    let mut doc = board();
    click(&mut doc, 100.0, 100.0);
    let layer = doc.active_layer;
    doc.text_set_marked("˚");
    assert!(
        ink_pixels(&doc, layer) > 0,
        "marked text draws on the board"
    );
    assert_eq!(doc.active_text_run().unwrap().text, "");
    doc.text_insert("å");
    assert_eq!(doc.active_text_run().unwrap().text, "å");
    assert!(doc.active_text_run().unwrap().marked.is_empty());
}

#[test]
fn an_abandoned_composition_does_not_survive_commit() {
    let mut doc = board();
    click(&mut doc, 100.0, 100.0);
    doc.text_insert("ok");
    doc.text_set_marked("¨");
    doc.commit_text();
    let run = doc.layers.last().unwrap().run().unwrap();
    assert_eq!(run.text, "ok");
    assert!(run.marked.is_empty());
}

#[test]
fn text_layers_composite_and_export_like_any_layer() {
    let mut doc = board();
    click(&mut doc, 100.0, 100.0);
    doc.text_insert("V");
    doc.commit_text();
    let index = doc.layers.len() - 1;
    let (w, h, rgba) = doc.composite_rgba();
    assert_eq!(w, 512);
    assert_eq!(h, 512);
    assert!(rgba.chunks_exact(4).any(|px| px[3] > 0));
    assert!(doc.layer_rgba(index).is_some(), "text exports as pixels");
    assert!(doc.painted_content_bounds().is_some());
}

/// Transform is one of the three things a text layer *does* answer to — its tiles carry a
/// layer transform like any other, and the run stays editable underneath.
#[test]
fn a_text_layer_transforms_but_still_refuses_paint() {
    let mut doc = board();
    click(&mut doc, 100.0, 100.0);
    doc.text_insert("T");
    doc.commit_text();
    doc.set_active_layer(doc.layers.len() - 1);
    assert!(
        doc.enter_transform(),
        "text scales and rotates like any layer"
    );
    assert!(doc.transform_active);
    doc.exit_transform();

    assert_eq!(doc.tool_block(Tool::Pen), ToolBlock::TextLayer);
    assert_eq!(doc.tool_block(Tool::Text), ToolBlock::None);
    assert_eq!(doc.tool_block(Tool::Move), ToolBlock::None);
}

#[test]
fn editing_a_layer_by_index_puts_the_caret_at_the_end() {
    let mut doc = board();
    click(&mut doc, 100.0, 100.0);
    doc.text_insert("abc");
    doc.commit_text();
    let index = doc.layers.len() - 1;
    assert!(doc.edit_text_layer(index));
    assert_eq!(doc.text_caret(), Some(3));
    assert!(!doc.edit_text_layer(0), "paper is not a text layer");
}

#[test]
fn the_board_reports_a_caret_and_a_box_while_editing() {
    let mut doc = board();
    assert!(doc.text_caret_segment().is_none());
    assert!(doc.text_box().is_none());
    click(&mut doc, 100.0, 100.0);
    doc.text_insert("caret");
    let ((ax, ay), (bx, by)) = doc.text_caret_segment().unwrap();
    assert_eq!(ax, bx);
    assert!(by > ay);
    let (x0, y0, x1, y1) = doc.text_box().unwrap();
    assert!(x1 > x0 && y1 > y0);
    assert!(ax >= x0 - 1.0 && ax <= x1 + 1.0);
    assert!(
        doc.has_animated_overlay(),
        "a blinking caret keeps frames coming"
    );
    assert!(
        !doc.has_live_preview(),
        "but it is an overlay, not a gesture — a caret must not pin the renderer to a full \
         content resync every frame, nor disable the overview proxy"
    );
}

#[test]
fn bold_and_italic_reflow_the_run_and_carry_forward() {
    let mut doc = board();
    click(&mut doc, 60.0, 200.0);
    doc.text_insert("weight");
    let plain = doc.text_box().unwrap();
    doc.set_text_bold(true);
    let bold = doc.text_box().unwrap();
    assert!(doc.active_text_run().unwrap().bold);
    assert!(bold.2 - bold.0 > plain.2 - plain.0, "bold sets wider");

    doc.set_text_italic(true);
    assert!(doc.active_text_run().unwrap().italic);
    doc.commit_text();

    click(&mut doc, 60.0, 400.0);
    let run = doc.active_text_run().unwrap();
    assert!(run.bold && run.italic, "the next layer starts styled too");
    doc.set_text_bold(false);
    assert!(!doc.active_text_run().unwrap().bold);
}

#[test]
fn line_height_reflows_the_run_and_is_clamped() {
    let mut doc = board();
    click(&mut doc, 60.0, 60.0);
    doc.text_insert("one\ntwo");
    let tight = doc.text_box().unwrap();
    doc.set_text_line_height(2.0);
    let loose = doc.text_box().unwrap();
    assert_eq!(doc.active_text_run().unwrap().line_height, 2.0);
    assert!(loose.3 - loose.1 > tight.3 - tight.1);

    doc.set_text_line_height(99.0);
    assert_eq!(
        doc.active_text_run().unwrap().line_height,
        TEXT_LINE_HEIGHT_MAX
    );
    doc.set_text_line_height(f32::NAN);
    assert!(doc.active_text_run().unwrap().line_height.is_finite());
}

#[test]
fn only_an_installed_family_is_accepted() {
    let mut doc = board();
    click(&mut doc, 60.0, 60.0);
    doc.text_insert("font");
    let before = doc.active_text_run().unwrap().family.clone();

    assert!(!doc.set_text_family("Definitely Not Installed"));
    assert!(!doc.set_text_family("  "));
    assert_eq!(doc.active_text_run().unwrap().family, before);

    let other = font_families()
        .into_iter()
        .find(|f| f != &before)
        .expect("more than one system font");
    assert!(doc.set_text_family(&other.to_uppercase()));
    assert_eq!(
        doc.active_text_run().unwrap().family,
        other,
        "the family is stored as the font database spells it"
    );
}

/// Regression: only the first text layer could be hidden. Later ones ignored the eye entirely,
/// wherever they sat in the stack, while a duplicate of the first stayed toggleable — which is
/// the shape of a bug that follows the layer rather than the index.
#[test]
fn every_text_layer_can_be_hidden_not_just_the_first() {
    let mut doc = board();

    click(&mut doc, 100.0, 100.0);
    doc.text_insert("one");
    doc.commit_text();

    click(&mut doc, 100.0, 300.0);
    doc.text_insert("two");
    doc.commit_text();

    let text_layers: Vec<usize> = (0..doc.layers.len())
        .filter(|&i| doc.layers[i].is_text())
        .collect();
    assert_eq!(text_layers.len(), 2, "two text layers were created");

    for &index in &text_layers {
        doc.set_layer_visible(index, false);
        assert!(
            !doc.layers[index].visible,
            "text layer at {index} ({}) refused to hide",
            doc.layers[index].name
        );
        doc.set_layer_visible(index, true);
        assert!(doc.layers[index].visible, "and it comes back");
    }
}

/// `TextEdit.layer` is a position, and every path that shifts the stack today remembers to
/// remap it — but one that forgets would have `commit_text` remove whatever layer moved into
/// that slot. Resolution goes through the layer id, so a stale index costs nothing.
#[test]
fn committing_text_follows_the_layer_id_not_a_stale_index() {
    let mut doc = board();
    click(&mut doc, 100.0, 100.0);
    doc.text_insert("keep me");
    let edited_id = doc.layers[doc.active_layer].id.clone();
    let before = doc.layers.len();

    // Exactly what a missed remap looks like: the edit still points at a real layer, just the
    // wrong one — here Paper, which an index-trusting commit would have been free to delete.
    doc.text_edit.as_mut().expect("editing").layer = 0;
    doc.commit_text();

    assert_eq!(doc.layers.len(), before, "no layer was removed");
    assert!(
        doc.layers.iter().any(|l| l.id == edited_id),
        "the edited layer survived"
    );
    assert!(doc.layers[0].is_paper(), "and Paper was not the casualty");
}
