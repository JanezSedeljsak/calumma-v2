use calumma_app::shortcuts::{
    is_clip_shortcut, is_fit_zoom_shortcut, is_redo_shortcut, is_toggle_layers_shortcut,
    is_transform_shortcut, is_undo_shortcut, is_zoom_in_shortcut, is_zoom_out_shortcut,
    tool_for_key,
};
use calumma_core::Tool;

#[derive(Clone, Copy)]
pub struct Modifiers {
    pub control: bool,
    pub meta: bool,
    pub shift: bool,
    pub alt: bool,
}

pub fn handle_shell_key(
    text: &str,
    mods: Modifiers,
    any_modal_open: bool,
    editor_open: bool,
    can_undo: bool,
    can_redo: bool,
) -> ShellKeyAction {
    if text == "\u{1b}" {
        return ShellKeyAction::Escape;
    }

    if (mods.meta || mods.control) && text == "," {
        return ShellKeyAction::OpenSettings;
    }

    if (mods.meta || mods.control)
        && text.eq_ignore_ascii_case("n")
        && editor_open
        && !any_modal_open
    {
        return ShellKeyAction::NewProject;
    }

    if any_modal_open {
        return ShellKeyAction::None;
    }

    if (mods.meta || mods.control) && !mods.shift && !mods.alt && text.eq_ignore_ascii_case("v") {
        return ShellKeyAction::Paste;
    }

    if editor_open && (text == "\n" || text == "\r") {
        return ShellKeyAction::Return;
    }

    if editor_open && is_toggle_layers_shortcut(text, mods.meta, mods.alt) {
        return ShellKeyAction::ToggleLayers;
    }

    if editor_open && is_clip_shortcut(text, mods.meta, mods.alt) {
        return ShellKeyAction::ClipLayer;
    }

    match handle_editor_key(text, mods, editor_open, can_undo, can_redo) {
        EditorKeyAction::None => ShellKeyAction::None,
        action => ShellKeyAction::Editor(action),
    }
}

pub fn handle_editor_key(
    text: &str,
    mods: Modifiers,
    editor_open: bool,
    can_undo: bool,
    can_redo: bool,
) -> EditorKeyAction {
    if !editor_open {
        return EditorKeyAction::None;
    }

    if is_undo_shortcut(text, mods.meta, mods.control, mods.shift) && can_undo {
        return EditorKeyAction::Undo;
    }
    if is_redo_shortcut(text, mods.meta, mods.control, mods.shift) && can_redo {
        return EditorKeyAction::Redo;
    }
    if is_transform_shortcut(text, mods.meta, mods.control) {
        return EditorKeyAction::ToggleTransform;
    }
    if is_zoom_in_shortcut(text, mods.meta, mods.control) {
        return EditorKeyAction::ZoomIn;
    }
    if is_zoom_out_shortcut(text, mods.meta, mods.control) {
        return EditorKeyAction::ZoomOut;
    }
    if mods.control || mods.meta || mods.alt {
        if text.eq_ignore_ascii_case("s") {
            return EditorKeyAction::Save;
        }
        return EditorKeyAction::None;
    }
    if is_fit_zoom_shortcut(text, mods.meta, mods.control, mods.alt) {
        return EditorKeyAction::FitZoom;
    }
    if text.len() == 1 {
        let key = text.chars().next().unwrap().to_ascii_lowercase();
        if let Some(tool) = tool_for_key(key) {
            return EditorKeyAction::PickTool(tool);
        }
    }
    EditorKeyAction::None
}

pub fn handle_key_release(text: &str, mods: Modifiers) -> KeyReleaseAction {
    if text == " " {
        return KeyReleaseAction::Space(false);
    }
    if mods.alt {
        return KeyReleaseAction::Alt(false);
    }
    if mods.meta || mods.control {
        return KeyReleaseAction::Meta(false);
    }
    if mods.shift {
        return KeyReleaseAction::Shift(false);
    }
    KeyReleaseAction::None
}

pub fn handle_key_press_for_modifiers(text: &str, mods: Modifiers) -> KeyPressModifierAction {
    if text == " " {
        return KeyPressModifierAction::Space(true);
    }
    if mods.alt {
        return KeyPressModifierAction::Alt(true);
    }
    KeyPressModifierAction::None
}

pub enum ShellKeyAction {
    None,
    Escape,
    Return,
    OpenSettings,
    NewProject,
    Paste,
    ToggleLayers,
    ClipLayer,
    Editor(EditorKeyAction),
}

pub enum EditorKeyAction {
    None,
    Undo,
    Redo,
    Save,
    ToggleTransform,
    ZoomIn,
    ZoomOut,
    FitZoom,
    PickTool(Tool),
}

pub enum KeyPressModifierAction {
    None,
    Space(bool),
    Alt(bool),
}

pub enum KeyReleaseAction {
    None,
    Space(bool),
    Alt(bool),
    Meta(bool),
    Shift(bool),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mods() -> Modifiers {
        Modifiers {
            control: false,
            meta: false,
            shift: false,
            alt: false,
        }
    }

    #[test]
    fn return_is_a_shell_shortcut_in_the_editor() {
        assert!(matches!(
            handle_shell_key("\n", mods(), false, true, false, false),
            ShellKeyAction::Return
        ));
    }

    #[test]
    fn command_v_pastes_in_the_editor_and_on_the_landing_screen() {
        let command = Modifiers {
            meta: true,
            ..mods()
        };
        for editor_open in [true, false] {
            assert!(matches!(
                handle_shell_key("v", command, false, editor_open, false, false),
                ShellKeyAction::Paste
            ));
        }
        assert!(matches!(
            handle_shell_key("v", mods(), false, true, false, false),
            ShellKeyAction::Editor(EditorKeyAction::PickTool(_))
        ));
    }

    #[test]
    fn return_does_not_submit_while_a_modal_is_open() {
        assert!(matches!(
            handle_shell_key("\n", mods(), true, true, false, false),
            ShellKeyAction::None
        ));
    }
}
