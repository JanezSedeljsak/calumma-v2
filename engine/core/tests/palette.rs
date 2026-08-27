use calumma_core::palette::*;

#[test]
fn project_color_wraps_the_palette() {
    assert_eq!(project_color(0), PROJECT_COLORS[0]);
    assert_eq!(project_color(PROJECT_COLORS.len()), PROJECT_COLORS[0]);
    assert_eq!(project_color(PROJECT_COLORS.len() + 3), PROJECT_COLORS[3]);
}

#[test]
fn random_project_color_is_always_from_the_palette() {
    for _ in 0..64 {
        assert!(PROJECT_COLORS.contains(&random_project_color()));
    }
}

#[test]
fn color_for_seed_is_stable_and_in_palette() {
    let first = color_for_seed("project-a");
    assert_eq!(first, color_for_seed("project-a"));
    assert!(PROJECT_COLORS.contains(&first));
}

#[test]
fn fallback_board_is_darker_on_dark_theme() {
    let dark = BoardColors::fallback(true);
    let light = BoardColors::fallback(false);
    assert!(dark.desk[0] < light.desk[0]);
    assert_eq!(dark.paper_border[0], 255);
    assert_eq!(light.paper_border[0], 0);
}

/// The shader reads these out of `PaperUniforms` and `CanvasSkeleton` reads them over
/// `calm_desk_metrics`, so the loading placeholder lands on the same lattice as the real desk.
/// A cell smaller than its own rule, or a cross wider than a cell, would draw a solid field
/// rather than squared paper.
#[test]
fn desk_metrics_describe_a_grid_that_reads_as_squared_paper() {
    let desk = DeskMetrics::DEFAULT;
    assert!(desk.cell > desk.line_width * 2.0);
    assert!(desk.cross_arm * 2.0 < desk.cell);
    assert!(desk.cross_line_width > 0.0);
    assert!((0.0..1.0).contains(&DeskMetrics::LINE_ALPHA));
}
