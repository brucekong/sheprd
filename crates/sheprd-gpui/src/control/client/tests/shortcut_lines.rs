use super::*;
use crate::control::protocol::chat::ShortcutInfo;

#[test]
fn shortcuts_read_as_labels_with_their_text_beneath() {
    let shortcuts = [
        ShortcutInfo {
            id: "a".into(),
            label: "Carry on".into(),
            title: "Carry on".into(),
            text: "Please continue".into(),
        },
        ShortcutInfo {
            id: "b".into(),
            label: "Review the diff".into(),
            title: String::new(),
            text: "Review the diff\nthen stop".into(),
        },
    ];
    assert_eq!(
        chat::describe_shortcuts(&shortcuts, true),
        "Carry on\n    Please continue\nReview the diff\n    Review the diff\n    then stop"
    );
    assert!(chat::describe_shortcuts(&shortcuts, false).ends_with("one answer holds)"));
}
