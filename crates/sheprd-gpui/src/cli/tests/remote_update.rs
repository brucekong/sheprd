//! `update install [--confirmed]` and `host update [HOST]`: updating Sheprd
//! on this machine from its dialog, or on another from this app's.
use super::*;
use control::{ControlCommand, HostCommand, UpdateCommand};

fn command(args: &[&str]) -> Result<ControlCommand, CliError> {
    match LaunchOptions::parse(args.iter().copied()) {
        Ok(LaunchOptions {
            mode: LaunchMode::Control(invocation),
            ..
        }) => Ok(invocation.command),
        Ok(other) => panic!("update parsed as {:?}", other.mode),
        Err(error) => Err(error),
    }
}

#[test]
fn update_install_is_unconfirmed_unless_said() {
    assert_eq!(
        command(&["update", "install"]).unwrap(),
        ControlCommand::Update(UpdateCommand::Install { confirmed: false })
    );
    assert_eq!(
        command(&["update", "install", "--confirmed", "--json"]).unwrap(),
        ControlCommand::Update(UpdateCommand::Install { confirmed: true })
    );
    assert_eq!(
        command(&["update"]).unwrap(),
        ControlCommand::Update(UpdateCommand::Status)
    );
}

#[test]
fn host_update_names_a_host_or_the_callers() {
    assert_eq!(
        command(&["host", "update", "studio"]).unwrap(),
        ControlCommand::Host(HostCommand::Update(Some("studio".into())))
    );
    assert_eq!(
        command(&["host", "update"]).unwrap(),
        ControlCommand::Host(HostCommand::Update(None))
    );
}
