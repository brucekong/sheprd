use super::*;
use core::prelude::v1::test;

fn shortcut(id: &str, title: &str, text: &str) -> TextShortcut {
    TextShortcut {
        id: id.into(),
        title: title.into(),
        text: text.into(),
    }
}

fn transcript(commands: &[&str]) -> Transcript {
    let mut transcript = Transcript::new("Claude");
    transcript.commands = commands
        .iter()
        .map(|name| SlashCommand {
            name: (*name).into(),
            description: String::new(),
            hint: None,
        })
        .collect();
    transcript
}

#[test]
fn a_slash_lists_shortcuts_before_the_agents_commands() {
    let shortcuts = [
        shortcut("a", "Carry on", "Please continue"),
        shortcut("b", "Review", "Review the diff"),
    ];
    let transcript = transcript(&["review", "compact"]);
    let found = completions(&shortcuts, &transcript, "/");
    assert_eq!(
        found,
        [
            Completion::Shortcut(&shortcuts[0]),
            Completion::Shortcut(&shortcuts[1]),
            Completion::Command(&transcript.commands[0]),
            Completion::Command(&transcript.commands[1]),
        ]
    );
    let found = completions(&shortcuts, &transcript, "/rev");
    assert_eq!(
        found,
        [
            Completion::Shortcut(&shortcuts[1]),
            Completion::Command(&transcript.commands[0]),
        ]
    );
    // The query matches a shortcut's text as well as its label.
    let found = completions(&shortcuts, &transcript, "/continue");
    assert_eq!(found, [Completion::Shortcut(&shortcuts[0])]);
}

#[test]
fn only_a_lone_slash_word_opens_the_list() {
    let shortcuts = [shortcut("a", "Carry on", "Please continue")];
    let transcript = transcript(&["review"]);
    assert!(completions(&shortcuts, &transcript, "").is_empty());
    assert!(completions(&shortcuts, &transcript, "carry").is_empty());
    assert!(completions(&shortcuts, &transcript, "/carry on").is_empty());
}

#[test]
fn the_slash_list_shows_a_bounded_number_of_shortcuts() {
    let shortcuts: Vec<_> = (0..SLASH_SHORTCUTS + 4)
        .map(|index| shortcut(&format!("s{index}"), "", "text"))
        .collect();
    let transcript = transcript(&[]);
    assert_eq!(
        completions(&shortcuts, &transcript, "/").len(),
        SLASH_SHORTCUTS
    );
}

#[test]
fn the_popover_finds_shortcuts_in_order() {
    let shortcuts = [
        shortcut("a", "Carry on", "Please continue"),
        shortcut("b", "", "Review the diff"),
        shortcut("c", "Explain", "Explain what you changed"),
    ];
    let ids = |query| {
        found(&shortcuts, query)
            .map(|shortcut| shortcut.id.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(""), ["a", "b", "c"]);
    assert_eq!(ids("e"), ["a", "b", "c"]);
    assert_eq!(ids("CHANGED"), ["c"]);
    assert!(ids("nothing").is_empty());
}

#[test]
fn a_shortcut_sends_alone_or_joins_the_draft() {
    assert_eq!(applied("", true, "carry on"), ("carry on".into(), true));
    assert_eq!(applied("  ", true, "carry on"), ("carry on".into(), true));
    // Files waiting to go keep it in the draft, as on the phone.
    assert_eq!(applied("", false, "carry on"), ("carry on".into(), false));
    assert_eq!(
        applied("look at", true, "carry on"),
        ("look at carry on".into(), false)
    );
}
