use super::*;
use control::{ChatCommand, ControlCommand, Invocation};

fn parse(args: &[&str]) -> Result<Invocation, CliError> {
    LaunchOptions::parse(args.iter().copied()).map(|options| match options.mode {
        LaunchMode::Control(invocation) => invocation,
        mode => panic!("{args:?} parsed as {mode:?}"),
    })
}

#[test]
fn chat_shortcuts_lists_them_with_the_shared_options() {
    let invocation = parse(&["chat", "shortcuts", "--json", "--window", "3"]).unwrap();
    assert_eq!(
        invocation.command,
        ControlCommand::Chat(ChatCommand::Shortcuts)
    );
    assert!(invocation.json);
    assert_eq!(invocation.window, Some(3));
    assert_eq!(
        parse(&["chat", "shortcuts"]).unwrap().command,
        ControlCommand::Chat(ChatCommand::Shortcuts)
    );
}

#[test]
fn chat_shortcuts_takes_nothing_else() {
    assert!(parse(&["chat", "shortcuts", "add"]).is_err());
    assert!(parse(&["chat", "shortcuts", "--pane", "w1:p1"]).is_err());
}
