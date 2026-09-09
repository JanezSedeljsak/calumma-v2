use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

pub fn confirm(title: &str, message: &str, ok: &str, cancel: &str) -> bool {
    MessageDialog::new()
        .set_title(title)
        .set_description(message)
        .set_level(MessageLevel::Warning)
        .set_buttons(MessageButtons::OkCancelCustom(
            ok.to_string(),
            cancel.to_string(),
        ))
        .show()
        == MessageDialogResult::Ok
}
