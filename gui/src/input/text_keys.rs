use super::shortcuts::Modifiers;
use calumma_app::Engine;
use calumma_core::Step;

const KEY_BACKSPACE: &str = "\u{8}";
const KEY_DELETE: &str = "\u{7f}";
const KEY_ESCAPE: &str = "\u{1b}";
const KEY_UP: &str = "\u{f700}";
const KEY_DOWN: &str = "\u{f701}";
const KEY_LEFT: &str = "\u{f702}";
const KEY_RIGHT: &str = "\u{f703}";
const KEY_HOME: &str = "\u{f729}";
const KEY_END: &str = "\u{f72b}";

#[derive(Debug, PartialEq, Eq)]
pub enum TextKeyAction {
    PassThrough,
    Insert(String),
    Backspace,
    DeleteForward,
    DeleteWord { forward: bool },
    Step { step: Step, extend: bool },
    SelectAll,
    Commit,
}

pub fn handle_text_key(text: &str, mods: Modifiers) -> TextKeyAction {
    let command = mods.meta || mods.control;
    let extend = mods.shift;
    let step = |step| TextKeyAction::Step { step, extend };
    match text {
        KEY_ESCAPE => return TextKeyAction::Commit,
        KEY_BACKSPACE if mods.alt => return TextKeyAction::DeleteWord { forward: false },
        KEY_BACKSPACE => return TextKeyAction::Backspace,
        KEY_DELETE if mods.alt => return TextKeyAction::DeleteWord { forward: true },
        KEY_DELETE => return TextKeyAction::DeleteForward,
        KEY_LEFT if command => return step(Step::LineStart),
        KEY_LEFT if mods.alt => return step(Step::WordLeft),
        KEY_LEFT => return step(Step::Left),
        KEY_RIGHT if command => return step(Step::LineEnd),
        KEY_RIGHT if mods.alt => return step(Step::WordRight),
        KEY_RIGHT => return step(Step::Right),
        KEY_UP if command => return step(Step::DocStart),
        KEY_UP => return step(Step::Up),
        KEY_DOWN if command => return step(Step::DocEnd),
        KEY_DOWN => return step(Step::Down),
        KEY_HOME => return step(Step::LineStart),
        KEY_END => return step(Step::LineEnd),
        "\n" | "\r" if !command => return TextKeyAction::Insert("\n".into()),
        _ => {}
    }
    if command {
        if text.eq_ignore_ascii_case("a") {
            return TextKeyAction::SelectAll;
        }
        return TextKeyAction::PassThrough;
    }
    if text.is_empty() || text.chars().any(|c| c.is_control() || is_private_use(c)) {
        return TextKeyAction::PassThrough;
    }
    TextKeyAction::Insert(text.to_string())
}

pub fn apply_text_key(engine: &mut Engine, action: TextKeyAction) -> bool {
    match action {
        TextKeyAction::PassThrough => return false,
        TextKeyAction::Insert(text) => engine.text_insert(&text),
        TextKeyAction::Backspace => engine.text_backspace(),
        TextKeyAction::DeleteForward => engine.text_delete_forward(),
        TextKeyAction::DeleteWord { forward } => engine.text_delete_word(forward),
        TextKeyAction::Step { step, extend } => engine.text_step_caret(step, extend),
        TextKeyAction::SelectAll => {
            engine.text_select_all();
        }
        TextKeyAction::Commit => engine.commit_text(),
    }
    true
}

fn is_private_use(c: char) -> bool {
    ('\u{e000}'..='\u{f8ff}').contains(&c)
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
    fn tool_letters_and_space_are_typed_not_shortcuts() {
        assert_eq!(
            handle_text_key("v", mods()),
            TextKeyAction::Insert("v".into())
        );
        assert_eq!(
            handle_text_key(" ", mods()),
            TextKeyAction::Insert(" ".into())
        );
    }

    #[test]
    fn command_chords_still_reach_the_editor() {
        let command = Modifiers {
            meta: true,
            ..mods()
        };
        assert_eq!(handle_text_key("z", command), TextKeyAction::PassThrough);
        assert_eq!(handle_text_key("a", command), TextKeyAction::SelectAll);
    }

    #[test]
    fn modifier_and_function_keys_pass_through() {
        assert_eq!(
            handle_text_key("\u{10}", mods()),
            TextKeyAction::PassThrough
        );
        assert_eq!(
            handle_text_key("\u{f704}", mods()),
            TextKeyAction::PassThrough
        );
    }

    #[test]
    fn shift_arrow_extends_and_option_backspace_deletes_a_word() {
        let shift = Modifiers {
            shift: true,
            ..mods()
        };
        assert_eq!(
            handle_text_key(KEY_LEFT, shift),
            TextKeyAction::Step {
                step: Step::Left,
                extend: true
            }
        );
        let alt = Modifiers {
            alt: true,
            ..mods()
        };
        assert_eq!(
            handle_text_key(KEY_BACKSPACE, alt),
            TextKeyAction::DeleteWord { forward: false }
        );
    }
}
