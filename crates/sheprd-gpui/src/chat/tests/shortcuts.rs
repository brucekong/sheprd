//! Text shortcuts in the composer: listed first when a slash is typed, and
//! in the popover of their own button. One sends at once on an empty draft,
//! as on the phone, and otherwise joins the draft.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use crate::text_shortcuts::Store;
use gpui::{Modifiers, TestAppContext, VisualTestContext};
use sheprd_chat_protocol::{Command, SlashCommand};
use sheprd_client::ConnectTarget;
use sheprd_remote_control::control::shortcuts::{TextShortcut, TextShortcuts};

fn hold_shortcuts(cx: &mut TestAppContext) {
    cx.update(|cx| {
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
                        text: "Explain what you changed".into(),
                    },
                ],
                10,
            ),
        );
    });
}

fn with_commands() -> ServerMessage {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Idle;
    transcript.entries = vec![agent(1, "Ready.")];
    transcript.commands = vec![SlashCommand {
        description: "Review the changes".into(),
        hint: None,
        name: "review".into(),
    }];
    ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    }
}

fn next_command(stream: &mut Peer) -> Command {
    match read(stream) {
        Some(ClientMessage::Command { command, .. }) => command,
        other => panic!("a command, got {other:?}"),
    }
}

fn prompt(text: &str) -> Command {
    Command::Prompt {
        text: text.into(),
        attachments: Vec::new(),
    }
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("missing {selector}"));
    cx.simulate_click(bounds.center(), Modifiers::default());
}

#[gpui::test]
fn a_slash_lists_shortcuts_above_the_commands(cx: &mut TestAppContext) {
    hold_shortcuts(cx);
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, with_commands(), cx);
    cx.simulate_input("/");
    poll(&view, &program, cx);
    let first = cx.debug_bounds("chat-shortcut-completion-0").unwrap();
    let second = cx.debug_bounds("chat-shortcut-completion-1").unwrap();
    let command = cx.debug_bounds("chat-completion-0").unwrap();
    assert!(first.bottom() <= second.top());
    assert!(second.bottom() <= command.top());
    // The query matches a shortcut's text as well as its label.
    cx.simulate_input("changed");
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-shortcut-completion-0").is_some());
    assert!(cx.debug_bounds("chat-shortcut-completion-1").is_none());
    assert!(cx.debug_bounds("chat-completion-0").is_none());
}

#[gpui::test]
fn tab_on_a_shortcut_sends_it_in_place_of_the_slash(cx: &mut TestAppContext) {
    hold_shortcuts(cx);
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, with_commands(), cx);
    cx.simulate_input("/carry");
    poll(&view, &program, cx);
    cx.simulate_keystrokes("tab");
    assert_eq!(next_command(&mut stream), prompt("Please continue"));
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-completions").is_none());
}

#[gpui::test]
fn clicking_a_slash_shortcut_sends_it(cx: &mut TestAppContext) {
    hold_shortcuts(cx);
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, with_commands(), cx);
    cx.simulate_input("/");
    poll(&view, &program, cx);
    click(cx, "chat-shortcut-completion-1");
    assert_eq!(
        next_command(&mut stream),
        prompt("Explain what you changed")
    );
}

#[gpui::test]
fn the_button_opens_a_popover_whose_shortcut_sends_on_an_empty_draft(cx: &mut TestAppContext) {
    hold_shortcuts(cx);
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, with_commands(), cx);
    let attach = cx.debug_bounds("chat-attach").unwrap();
    let button = cx.debug_bounds("chat-shortcuts").unwrap();
    assert!(attach.right() <= button.left(), "beside the +");
    click(cx, "chat-shortcuts");
    poll(&view, &program, cx);
    let popover = cx.debug_bounds("chat-shortcuts-popover").unwrap();
    let composer = cx.debug_bounds("chat-composer").unwrap();
    assert!(popover.bottom() <= composer.top(), "over the composer");
    for selector in [
        "chat-shortcuts-search",
        "chat-shortcut-0",
        "chat-shortcut-1",
        "chat-shortcuts-edit",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "{selector}");
    }
    click(cx, "chat-shortcut-0");
    assert_eq!(next_command(&mut stream), prompt("Please continue"));
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-shortcuts-popover").is_none());
}

#[gpui::test]
fn a_shortcut_joins_a_draft_without_sending(cx: &mut TestAppContext) {
    hold_shortcuts(cx);
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, with_commands(), cx);
    cx.simulate_input("Look at main.rs.");
    click(cx, "chat-shortcuts");
    poll(&view, &program, cx);
    click(cx, "chat-shortcut-1");
    poll(&view, &program, cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(
        next_command(&mut stream),
        prompt("Look at main.rs. Explain what you changed")
    );
}

#[gpui::test]
fn the_popover_search_finds_and_enter_takes_the_first(cx: &mut TestAppContext) {
    hold_shortcuts(cx);
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, with_commands(), cx);
    click(cx, "chat-shortcuts");
    poll(&view, &program, cx);
    // The search field holds the keyboard once the popover opens.
    cx.simulate_input("explain");
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-shortcut-0").is_some());
    assert!(cx.debug_bounds("chat-shortcut-1").is_none());
    cx.simulate_keystrokes("enter");
    assert_eq!(
        next_command(&mut stream),
        prompt("Explain what you changed")
    );
}

#[gpui::test]
fn the_button_and_escape_close_the_popover(cx: &mut TestAppContext) {
    hold_shortcuts(cx);
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, with_commands(), cx);
    click(cx, "chat-shortcuts");
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-shortcuts-popover").is_some());
    click(cx, "chat-shortcuts");
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-shortcuts-popover").is_none());
    click(cx, "chat-shortcuts");
    poll(&view, &program, cx);
    cx.simulate_keystrokes("escape");
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-shortcuts-popover").is_none());
}

#[gpui::test]
fn an_empty_list_says_so_in_the_popover(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, with_commands(), cx);
    click(cx, "chat-shortcuts");
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-shortcuts-none").is_some());
    assert!(cx.debug_bounds("chat-shortcuts-edit").is_some());
}
