use calumma_app::shortcuts::{
    is_redo_shortcut, is_toggle_layers_shortcut, is_transform_shortcut, is_undo_shortcut,
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

    if editor_open && is_toggle_layers_shortcut(text, mods.meta, mods.alt) {
        return ShellKeyAction::ToggleLayers;
    }

    match handle_editor_key(text, mods, editor_open, can_undo, can_redo) {
        EditorKeyAction::None => ShellKeyAction::None,
        EditorKeyAction::Undo => ShellKeyAction::Editor(EditorKeyAction::Undo),
        EditorKeyAction::Redo => ShellKeyAction::Editor(EditorKeyAction::Redo),
        EditorKeyAction::Save => ShellKeyAction::Editor(EditorKeyAction::Save),
        EditorKeyAction::ToggleTransform => {
            ShellKeyAction::Editor(EditorKeyAction::ToggleTransform)
        }
        EditorKeyAction::PickTool(tool) => ShellKeyAction::Editor(EditorKeyAction::PickTool(tool)),
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
    if mods.control || mods.meta || mods.alt {
        if text.eq_ignore_ascii_case("s") {
            return EditorKeyAction::Save;
        }
        return EditorKeyAction::None;
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
    OpenSettings,
    NewProject,
    ToggleLayers,
    Editor(EditorKeyAction),
}

pub enum EditorKeyAction {
    None,
    Undo,
    Redo,
    Save,
    ToggleTransform,
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
