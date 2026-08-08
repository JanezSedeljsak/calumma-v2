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
