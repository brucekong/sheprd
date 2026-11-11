use super::*;
use crate::text_shortcuts::Store;
use sheprd_remote_control::control::shortcuts::{TextShortcut, TextShortcuts};

#[gpui::test]
fn shortcuts_need_a_window(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        assert_eq!(
            code(&serve(call(ChatOp::Shortcuts), cx)),
            Some(ErrorCode::NoWindow)
        );
    });
}

#[gpui::test]
fn shortcuts_list_in_order_with_their_labels(cx: &mut gpui::TestAppContext) {
    let (_view, cx) = cx.add_window_view(fixture_window);
    cx.cx.update(|cx| {
        Store::hold(
            cx,
            TextShortcuts::new(
                [
                    TextShortcut {
                        id: "a".into(),
                        title: "Carry on".into(),
                        text: "Please continue".into(),
                    },
                    TextShortcut {
                        id: "b".into(),
                        title: String::new(),
                        text: "Review the diff\nthen stop".into(),
                    },
                ],
                42,
            ),
        );
        let Response::Chat {
            reply:
                Reply::Shortcuts {
                    shortcuts,
                    updated_ms,
                    complete,
                },
        } = serve(call(ChatOp::Shortcuts), cx)
        else {
            panic!("not a list of shortcuts");
        };
        assert_eq!(updated_ms, 42);
        assert!(complete);
        let labels: Vec<_> = shortcuts.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, ["Carry on", "Review the diff"]);
        assert_eq!(shortcuts[1].text, "Review the diff\nthen stop");
    });
}
