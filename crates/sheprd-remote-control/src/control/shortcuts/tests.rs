use super::*;

fn shortcut(id: &str, title: &str, text: &str) -> TextShortcut {
    TextShortcut {
        id: id.into(),
        title: title.into(),
        text: text.into(),
    }
}

#[test]
fn shortcuts_are_tidied_within_bounds() {
    let kept = TextShortcuts::new(
        [
            shortcut("a1", "  Carry   on ", "Carry on with the plan."),
            shortcut("b2", "Blank", "  \n "),
            shortcut("a1", "Again", "a repeated id"),
            shortcut("not plain!", "Odd id", "text"),
            shortcut("", "No id", "text"),
            shortcut("c3", "Bell\u{7}", "Run CI\u{7}\n\tthen stop."),
        ],
        5,
    );
    assert_eq!(kept.updated_ms, 5);
    assert_eq!(
        kept.shortcuts,
        [
            shortcut("a1", "Carry on", "Carry on with the plan."),
            shortcut("c3", "", "Run CI\n\tthen stop."),
        ]
    );
}

#[test]
fn limits_count_characters() {
    let long = TextShortcuts::new(
        [shortcut(
            "x",
            &"é".repeat(MAX_TITLE_CHARS + 9),
            &"ü".repeat(MAX_TEXT_CHARS + 9),
        )],
        1,
    );
    assert_eq!(long.shortcuts[0].title.chars().count(), MAX_TITLE_CHARS);
    assert_eq!(long.shortcuts[0].text.chars().count(), MAX_TEXT_CHARS);
    let many = (0..MAX_SHORTCUTS + 5).map(|index| shortcut(&format!("s{index}"), "", "go"));
    assert_eq!(TextShortcuts::new(many, 1).shortcuts.len(), MAX_SHORTCUTS);
}

#[test]
fn a_blank_title_shows_the_start_of_the_text() {
    assert_eq!(shortcut("a", "Run CI", "just ci").label(), "Run CI");
    assert_eq!(
        shortcut("a", "", "\n  Explain   what you changed\nand why").label(),
        "Explain what you changed"
    );
    let label = shortcut("a", "", &"word ".repeat(40)).label();
    assert_eq!(label.chars().count(), MAX_TITLE_CHARS);
    assert!(label.ends_with('…'));
}

#[test]
fn the_latest_edit_wins_whole() {
    let stored = TextShortcuts::new([shortcut("a", "Carry on", "Carry on.")], 100);
    let newer = TextShortcuts::new([shortcut("b", "Review", "Review the diff.")], 200);
    assert_eq!(stored.clone().merged(newer.clone(), 1_000), newer);
    assert_eq!(
        newer.clone().merged(stored.clone(), 1_000),
        newer,
        "an older edit loses"
    );
    assert_eq!(
        stored.clone().merged(stored.clone(), 1_000),
        stored,
        "a tie keeps the stored copy"
    );
    let ahead = TextShortcuts::new([shortcut("c", "", "From the future")], 9_999);
    assert_eq!(
        stored.merged(ahead, 1_000).updated_ms,
        1_000,
        "a clock ahead counts as now"
    );
}

#[test]
fn the_wire_takes_a_missing_title() {
    let parsed: TextShortcuts =
        serde_json::from_str(r#"{"shortcuts":[{"id":"a","text":"go"}],"updated_ms":3}"#).unwrap();
    assert_eq!(parsed.shortcuts, [shortcut("a", "", "go")]);
}

#[test]
fn the_largest_list_fits_its_requests() {
    let wide = "𝄞".repeat(MAX_TEXT_CHARS);
    let title = "𝄞".repeat(MAX_TITLE_CHARS);
    let full = TextShortcuts::new(
        (0..MAX_SHORTCUTS).map(|index| shortcut(&format!("{index:0>64}"), &title, &wide)),
        u64::MAX,
    );
    assert_eq!(full.shortcuts.len(), MAX_SHORTCUTS);
    assert!(serde_json::to_vec(&full).unwrap().len() < MAX_WIRE_BYTES);
    let lines = TextShortcuts::new([shortcut("n", "", &"\n".repeat(MAX_TEXT_CHARS / 2))], 1);
    assert!(serde_json::to_vec(&lines).unwrap().len() < MAX_WIRE_BYTES / MAX_SHORTCUTS);
}

#[test]
fn an_edit_after_a_clock_went_back_still_wins() {
    // Saved at 1000; both clocks now read 900. The phone stamps its edit
    // just after the copy it replaces.
    let saved = TextShortcuts::new([shortcut("a", "", "old")], 1_000);
    let edit = TextShortcuts::new([shortcut("a", "", "new")], 1_001);
    assert_eq!(saved.clone().merged(edit.clone(), 900), edit);
    // A clock far ahead still counts as just after the saved copy, not as
    // its own time.
    let ahead = TextShortcuts::new([shortcut("b", "", "ahead")], 9_999);
    assert_eq!(saved.merged(ahead, 900).updated_ms, 1_001);
}
