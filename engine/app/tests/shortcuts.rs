use calumma_app::shortcuts::{
    is_clip_shortcut, is_fit_zoom_shortcut, is_marquee_family, is_toggle_layers_shortcut,
    is_zoom_in_shortcut, is_zoom_out_shortcut, key_for_tool, tool_for_key,
};
use calumma_core::Tool;

#[test]
fn tool_keys_match_legacy_table() {
    assert_eq!(tool_for_key('p'), Some(Tool::Pen));
    assert_eq!(tool_for_key('m'), Some(Tool::SelectRect));
    assert_eq!(tool_for_key('z'), None);
}

#[test]
fn marquee_family_resolves_to_select_rect_key() {
    assert!(is_marquee_family(Tool::SelectLasso));
    assert_eq!(key_for_tool(Tool::SelectEllipse), Some('m'));
}

#[test]
fn toggle_layers_shortcut() {
    assert!(is_toggle_layers_shortcut("l", true, true));
    assert!(is_toggle_layers_shortcut("L", true, true));
    assert!(!is_toggle_layers_shortcut("l", true, false));
    assert!(!is_toggle_layers_shortcut("l", false, true));
}

#[test]
fn zoom_shortcuts() {
    assert!(is_zoom_in_shortcut("=", true, false));
    assert!(is_zoom_in_shortcut("+", true, false));
    assert!(!is_zoom_in_shortcut("=", false, false));
    assert!(is_zoom_out_shortcut("-", true, false));
    assert!(!is_zoom_out_shortcut("-", false, false));
    assert!(is_fit_zoom_shortcut("0", false, false, false));
    assert!(!is_fit_zoom_shortcut("0", true, false, false));
}

#[test]
fn clip_shortcut() {
    assert!(is_clip_shortcut("g", true, true));
    assert!(is_clip_shortcut("G", true, true));
    assert!(!is_clip_shortcut("g", true, false));
}
