use super::{AppWindow, PresetRow, Theme as UiTheme, Tokens as UiTokens};
use crate::app_icon;
use crate::shell::Theme;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

pub fn apply_theme(ui: &AppWindow, theme: &Theme) {
    let palette = ui.global::<UiTheme>();
    palette.set_bg(theme.bg);
    palette.set_surface(theme.surface);
    palette.set_surface_hover(theme.surface_hover);
    palette.set_text(theme.text);
    palette.set_text_muted(theme.text_muted);
    palette.set_accent_teal(theme.accent_teal);
    palette.set_accent_orange(theme.accent_orange);
    palette.set_island_border(theme.island_border);
    palette.set_control_border(theme.control_border);
    palette.set_control_focus_border(theme.control_focus_border);
    palette.set_danger(theme.danger);
    palette.set_paper(theme.paper);

    let metrics = &theme.metrics;
    let tokens = ui.global::<UiTokens>();
    tokens.set_radius_sm(metrics.radius_sm);
    tokens.set_radius_md(metrics.radius_md);
    tokens.set_radius_lg(metrics.radius_lg);
    tokens.set_radius_window(metrics.radius_window);
    tokens.set_radius_island(metrics.radius_island);
    tokens.set_space_xs(metrics.space_xs);
    tokens.set_space_sm(metrics.space_sm);
    tokens.set_space_md(metrics.space_md);
    tokens.set_space_lg(metrics.space_lg);
    tokens.set_space_xl(metrics.space_xl);
    tokens.set_space_xxl(metrics.space_xxl);
    tokens.set_control_height(metrics.control_height);
    tokens.set_label_size(metrics.label_size);
    tokens.set_label_tracking(metrics.label_tracking * 10.0);
    tokens.set_body_size(metrics.body_size);
    tokens.set_title_size(metrics.title_size);
    tokens.set_brand_size(metrics.brand_size);
    tokens.set_main_min_width(metrics.main_min_width);
    tokens.set_main_min_height(metrics.main_min_height);
    tokens.set_new_project_width(metrics.new_project_width);
    tokens.set_new_project_height(metrics.new_project_height);
    tokens.set_paste_min_width(metrics.paste_min_width);
    tokens.set_paste_max_width(metrics.paste_max_width);
    tokens.set_paste_min_height(metrics.paste_min_height);
    tokens.set_paste_width_ratio(metrics.paste_width_ratio);

    let presets: Vec<PresetRow> = theme
        .presets
        .iter()
        .map(|p| PresetRow {
            label: SharedString::from(p.label.as_str()),
            width: p.width as i32,
            height: p.height as i32,
        })
        .collect();
    ui.set_presets(ModelRc::new(VecModel::from(presets)));
    ui.set_app_icon(app_icon::mark_image());
}
