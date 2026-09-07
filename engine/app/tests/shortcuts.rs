use calumma_app::shortcuts::{
    is_marquee_family, is_toggle_layers_shortcut, key_for_tool, tool_for_key,
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
