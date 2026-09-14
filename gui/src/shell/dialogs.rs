use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

pub fn confirm(title: &str, message: &str, ok: &str, cancel: &str) -> bool {
    let result = MessageDialog::new()
        .set_title(title)
        .set_description(message)
        .set_level(MessageLevel::Warning)
        .set_buttons(MessageButtons::OkCancelCustom(
            ok.to_string(),
            cancel.to_string(),
        ))
        .show();
    confirm_accepted(&result, ok)
}

fn confirm_accepted(result: &MessageDialogResult, ok: &str) -> bool {
    match result {
        MessageDialogResult::Ok | MessageDialogResult::Yes => true,
        MessageDialogResult::Custom(label) => label == ok,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{confirm_accepted, MessageDialogResult};

    #[test]
    fn custom_ok_label_confirms() {
        assert!(confirm_accepted(
            &MessageDialogResult::Custom("Delete Project".into()),
            "Delete Project",
        ));
    }

    #[test]
    fn custom_cancel_label_does_not_confirm() {
        assert!(!confirm_accepted(
            &MessageDialogResult::Custom("Cancel".into()),
            "Delete Project",
        ));
    }

    #[test]
    fn stock_ok_still_confirms() {
        assert!(confirm_accepted(&MessageDialogResult::Ok, "Delete Project"));
    }
}
