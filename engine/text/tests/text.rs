use calumma_text::{
    caret_rect, index_at_point, measure, rasterize, step_index, Step, TextAlign, TextRun,
};

/// A paragraph long enough to wrap several times at `WRAP`, so caret questions have to be
/// answered against visual rows rather than against the one line the string holds.
const PARAGRAPH: &str = "hello world this is a long line";
const WRAP: f32 = 150.0;

fn wrapped() -> TextRun {
    TextRun {
        wrap_width: Some(WRAP),
        ..run(PARAGRAPH)
    }
    .clamped()
}

fn run(text: &str) -> TextRun {
    TextRun {
        text: text.to_string(),
        size: 32.0,
        origin: (100.0, 50.0),
        ..TextRun::default()
    }
    .clamped()
}

#[test]
fn empty_run_rasterizes_to_nothing() {
    assert!(rasterize(&run("")).is_none());
}

#[test]
fn rasterized_text_has_ink() {
    let raster = rasterize(&run("Calumma")).expect("text should rasterize");
    assert!(raster.width > 0 && raster.height > 0);
    assert_eq!(
        raster.rgba.len(),
        (raster.width as usize) * (raster.height as usize) * 4
    );
    assert!(raster.rgba.chunks_exact(4).any(|px| px[3] > 0));
}

#[test]
fn raster_sits_at_the_run_origin() {
    let raster = rasterize(&run("Hi")).expect("text should rasterize");
    assert!((raster.origin_x - 100).abs() < 40);
    assert!((raster.origin_y - 50).abs() < 80);
}

#[test]
fn longer_text_measures_wider() {
    let (narrow, _) = measure(&run("i"));
    let (wide, _) = measure(&run("iiiiiiiiii"));
    assert!(wide > narrow);
}

#[test]
fn wrapping_splits_lines() {
    let mut wrapped = run("one two three four five six seven eight");
    wrapped.wrap_width = Some(120.0);
    let (_, tall) = measure(&wrapped.clone().clamped());
    let (_, short) = measure(&run("one"));
    assert!(tall > short, "wrapped text should take more lines");
}

#[test]
fn caret_advances_with_the_text() {
    let r = run("abc");
    let start = caret_rect(&r, 0);
    let end = caret_rect(&r, 3);
    assert!(end.x > start.x);
    assert!((start.y - 50.0).abs() < 2.0);
    assert!(start.height > 0.0);
}

#[test]
fn caret_of_empty_run_sits_at_the_origin() {
    let caret = caret_rect(&run(""), 0);
    assert!((caret.x - 100.0).abs() < 2.0);
    assert!((caret.y - 50.0).abs() < 2.0);
}

#[test]
fn hit_testing_round_trips_through_the_caret() {
    let r = run("hello world");
    let caret = caret_rect(&r, 6);
    let index = index_at_point(&r, caret.x + 1.0, caret.y + caret.height * 0.5);
    assert_eq!(index, 6);
}

/// Hebrew, right-to-left. `leading_edge`/`trailing_edge` (layout.rs) branch on
/// `glyph.level.is_rtl()` specifically so the caret and hit-testing agree with the reading
/// direction rather than with byte order — nothing exercised that branch before this.
const HEBREW: &str = "שלום עולם";

#[test]
fn rtl_caret_runs_right_to_left() {
    let r = run(HEBREW);
    let start = caret_rect(&r, 0);
    let end = caret_rect(&r, HEBREW.len());
    assert!(
        end.x < start.x,
        "an RTL run's caret should move left as the logical index advances, not right: \
         start={start:?} end={end:?}"
    );
}

#[test]
fn rtl_hit_testing_round_trips_through_the_caret() {
    let r = run(HEBREW);
    let mid = HEBREW.char_indices().nth(2).map(|(i, _)| i).unwrap();
    let caret = caret_rect(&r, mid);
    // The mirror of the LTR probe above: advancing into the *next* glyph in reading order
    // moves left on screen for RTL text, not right.
    let index = index_at_point(&r, caret.x - 1.0, caret.y + caret.height * 0.5);
    assert_eq!(index, mid);
}

#[test]
fn caret_steps_respect_char_boundaries() {
    let r = run("ačb");
    assert_eq!(step_index(&r, 0, Step::Right), 1);
    assert_eq!(step_index(&r, 1, Step::Right), 3);
    assert_eq!(step_index(&r, 3, Step::Left), 1);
    assert_eq!(step_index(&r, 0, Step::Left), 0);
    assert_eq!(step_index(&r, 1, Step::DocEnd), r.text.len());
    assert_eq!(step_index(&r, 4, Step::DocStart), 0);
}

#[test]
fn vertical_steps_move_between_lines() {
    let r = run("first\nsecond");
    let down = step_index(&r, 1, Step::Down);
    assert!(down > 5, "caret should land on the second line, got {down}");
    let back = step_index(&r, down, Step::Up);
    assert!(
        back <= 5,
        "caret should return to the first line, got {back}"
    );
}

#[test]
fn line_ends_clamp_to_the_line() {
    let r = run("ab\ncdef");
    assert_eq!(step_index(&r, 1, Step::LineStart), 0);
    assert_eq!(step_index(&r, 1, Step::LineEnd), 2);
    assert_eq!(step_index(&r, 4, Step::LineStart), 3);
    assert_eq!(step_index(&r, 4, Step::LineEnd), 7);
}

#[test]
fn marked_text_is_laid_out_at_the_caret() {
    let mut composing = run("ab");
    composing.marked = "^".to_string();
    composing.marked_at = 1;
    assert_eq!(composing.display_text(), "a^b");
    assert_eq!(composing.display_index(0), 0);
    assert_eq!(composing.display_index(2), 3);
    let (with_mark, _) = measure(&composing);
    let (without, _) = measure(&run("ab"));
    assert!(with_mark > without);
}

#[test]
fn alignment_shifts_the_caret_inside_the_box() {
    let mut left = run("hi");
    left.wrap_width = Some(400.0);
    let mut centered = left.clone();
    centered.align = TextAlign::Center;
    assert!(caret_rect(&centered, 0).x > caret_rect(&left, 0).x);
}

#[test]
fn clamping_repairs_a_malformed_run() {
    let repaired = TextRun {
        size: f32::NAN,
        line_height: 99.0,
        family: "   ".to_string(),
        origin: (f32::INFINITY, 0.0),
        wrap_width: Some(1.0),
        marked_at: 900,
        ..TextRun::default()
    }
    .clamped();
    assert!(repaired.size.is_finite());
    assert!(repaired.line_height <= 4.0);
    assert!(!repaired.family.trim().is_empty());
    assert_eq!(repaired.origin, (0.0, 0.0));
    assert_eq!(repaired.wrap_width, Some(16.0));
    assert_eq!(repaired.marked_at, 0);
}

#[test]
fn the_caret_walks_down_the_visual_rows_of_a_wrapped_paragraph() {
    let r = wrapped();
    let start = caret_rect(&r, 0);
    let end = caret_rect(&r, r.text.len());
    let (_, height) = measure(&r);
    assert!(
        height > start.height * 2.0,
        "the paragraph should wrap onto several rows"
    );
    assert!(
        end.y > start.y,
        "the last caret sits below the first: {start:?} vs {end:?}"
    );
    let mut rows: Vec<f32> = (0..=r.text.len()).map(|i| caret_rect(&r, i).y).collect();
    rows.dedup();
    assert!(rows.len() > 2, "carets should span the rows, got {rows:?}");
    assert!(
        rows.windows(2).all(|w| w[1] >= w[0]),
        "rows must advance in reading order, got {rows:?}"
    );
}

#[test]
fn a_click_on_a_wrapped_row_round_trips_through_the_caret() {
    let r = wrapped();
    for index in [0usize, 8, 14, 20, 27] {
        let caret = caret_rect(&r, index);
        let hit = index_at_point(&r, caret.x + 1.0, caret.y + caret.height * 0.5);
        assert_eq!(hit, index, "caret at {index} did not survive a click");
    }
}

#[test]
fn vertical_steps_cross_wrapped_rows() {
    let r = wrapped();
    let down = step_index(&r, 2, Step::Down);
    assert!(down > 2, "down should advance, got {down}");
    assert!(
        caret_rect(&r, down).y > caret_rect(&r, 2).y,
        "down should land on a lower row"
    );
    let back = step_index(&r, down, Step::Up);
    assert!(
        caret_rect(&r, back).y < caret_rect(&r, down).y,
        "up should return to the row above"
    );
}

#[test]
fn line_ends_stop_at_the_wrapped_row_not_the_paragraph() {
    let r = wrapped();
    let end = step_index(&r, 2, Step::LineEnd);
    assert!(
        end < r.text.len(),
        "line end must not run to the end of the paragraph, got {end}"
    );
    assert_eq!(
        caret_rect(&r, end).y,
        caret_rect(&r, 2).y,
        "line end stays on its own row"
    );
    let start = step_index(&r, 20, Step::LineStart);
    assert!(start > 0, "line start must not jump to the paragraph start");
    assert_eq!(caret_rect(&r, start).y, caret_rect(&r, 20).y);
    assert_eq!(step_index(&r, 20, Step::DocStart), 0);
    assert_eq!(step_index(&r, 2, Step::DocEnd), r.text.len());
}

#[test]
fn caret_steps_cross_whole_grapheme_clusters() {
    let r = run("a👋🏽b");
    let after_emoji = step_index(&r, 1, Step::Right);
    assert_eq!(
        &r.text[1..after_emoji],
        "👋🏽",
        "one step should clear the whole cluster"
    );
    assert_eq!(step_index(&r, after_emoji, Step::Left), 1);

    let flag = run("a🇸🇮b");
    let after_flag = step_index(&flag, 1, Step::Right);
    assert_eq!(&flag.text[1..after_flag], "🇸🇮");
}

#[test]
fn bold_and_italic_are_part_of_the_layout() {
    let plain = run("Handgloves");
    let bold = TextRun {
        bold: true,
        ..plain.clone()
    };
    let italic = TextRun {
        italic: true,
        ..plain.clone()
    };
    assert_ne!(measure(&bold), measure(&plain));
    assert_ne!(
        rasterize(&italic).map(|r| r.rgba),
        rasterize(&plain).map(|r| r.rgba)
    );
    assert_eq!(caret_rect(&plain, 0).x, caret_rect(&bold, 0).x);
    assert!(caret_rect(&bold, 10).x > caret_rect(&plain, 10).x);
}

#[test]
fn line_height_stretches_the_block_without_moving_the_first_row() {
    let single = run("one\ntwo\nthree");
    let loose = TextRun {
        line_height: 2.0,
        ..single.clone()
    }
    .clamped();
    assert!(measure(&loose).1 > measure(&single).1);
    assert_eq!(caret_rect(&loose, 0).y, caret_rect(&single, 0).y);
    assert!(caret_rect(&loose, 8).y > caret_rect(&single, 8).y);
}
