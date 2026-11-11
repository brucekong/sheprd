use super::*;

fn info(id: &str, text: &str) -> ShortcutInfo {
    ShortcutInfo {
        id: id.into(),
        label: text.into(),
        title: String::new(),
        text: text.into(),
    }
}

#[test]
fn the_shortcuts_request_and_reply_have_a_stable_wire_form() {
    let request: Request =
        serde_json::from_str(r#"{"method":"chat","caller":{},"op":{"op":"shortcuts"}}"#).unwrap();
    assert!(matches!(
        request,
        Request::Chat(Call {
            op: ChatOp::Shortcuts,
            ..
        })
    ));
    let reply = Response::Chat {
        reply: Reply::Shortcuts {
            shortcuts: vec![info("a", "Carry on")],
            updated_ms: 7,
            complete: true,
        },
    };
    let text = serde_json::to_string(&reply).unwrap();
    assert!(text.contains(r#""kind":"shortcuts""#), "{text}");
    assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), reply);
    assert!(
        serde_json::from_str::<Request>(r#"{"method":"chat","caller":{},"op":{"op":"shortcut"}}"#)
            .is_err()
    );
}

#[test]
fn a_reply_lists_shortcuts_within_its_bound() {
    let (listed, complete) = bounded_shortcuts([info("a", "one"), info("b", "two")]);
    assert_eq!(listed.len(), 2);
    assert!(complete);
    let long = "x".repeat(MAX_SHORTCUT_BYTES / 3);
    let many = (0..5).map(|index| info(&index.to_string(), &long));
    let (listed, complete) = bounded_shortcuts(many);
    assert!(!complete);
    assert!(!listed.is_empty() && listed.len() < 5);
    let total: usize = listed.iter().map(|shortcut| shortcut.text.len() * 2).sum();
    assert!(total <= MAX_SHORTCUT_BYTES);
}

#[test]
fn escaped_text_is_counted_as_encoded_so_a_reply_fits_the_socket() {
    // Newlines, quotes, and backslashes double when encoded.
    let escaped = "\n\"\\".repeat(4_000 / 3);
    let many = (0..64).map(|index| info(&format!("{index:0>36}"), &escaped));
    let (listed, complete) = bounded_shortcuts(many);
    assert!(!complete);
    let reply = Response::Chat {
        reply: Reply::Shortcuts {
            shortcuts: listed,
            updated_ms: u64::MAX,
            complete,
        },
    };
    assert!(serde_json::to_vec(&reply).unwrap().len() < crate::protocol::MAX_MESSAGE);
}
