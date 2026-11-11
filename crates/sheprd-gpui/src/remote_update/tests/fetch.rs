//! Reading what `sheprd update ... --json` prints on another machine.
use super::*;
use crate::remote_update::fetch;

fn exit(code: i32) -> sheprd_client::Error {
    #[cfg(unix)]
    let status = {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(code << 8)
    };
    #[cfg(windows)]
    let status = {
        use std::os::windows::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(code as u32)
    };
    sheprd_client::Error::ScriptExit {
        status,
        stderr: String::new(),
    }
}

#[test]
fn an_answer_is_read_and_its_text_cleaned() {
    let read = fetch::parse(
        br#"{"state":"available","latest":"20261109.2\u001b[31m","current":"20261101.1","detail":null}"#,
    )
    .unwrap();
    assert_eq!(read.phase, Phase::Offered);
    assert_eq!(read.current.as_deref(), Some("20261101.1"));
    let latest = read.latest.unwrap();
    assert!(latest.starts_with("20261109.2"));
    assert!(!latest.chars().any(char::is_control), "{latest:?}");
}

#[test]
fn an_answer_from_an_app_without_its_version_still_reads() {
    let read = fetch::parse(br#"{"state":"ready","latest":"20261109.2","detail":null}"#).unwrap();
    assert_eq!(read.phase, Phase::Offered);
    assert_eq!(read.current, None);
}

#[test]
fn output_that_is_not_an_answer_is_refused() {
    assert!(matches!(
        fetch::parse(b"Sheprd is up to date"),
        Err(Error::Answer(_))
    ));
}

#[test]
fn exit_codes_say_what_is_missing() {
    assert!(matches!(fetch::classify(exit(127), b""), Error::Missing));
    assert!(matches!(fetch::classify(exit(3), b""), Error::NotRunning));
    // An app from before `update install` calls the verb invalid.
    assert!(matches!(fetch::classify(exit(2), b""), Error::TooOld));
    assert!(matches!(
        fetch::classify(
            exit(1),
            br#"{"error":{"code":"failed","message":"Only the Sheprd that connects..."}}"#
        ),
        Error::Refused(message) if message.starts_with("Only the Sheprd")
    ));
    assert!(matches!(
        fetch::classify(
            exit(8),
            br#"{"error":{"code":"unsupported","message":"Updates are off in this build: Local build"}}"#
        ),
        Error::Refused(_)
    ));
    assert!(matches!(
        fetch::classify(exit(1), b"garbage"),
        Error::Run(_)
    ));
}

#[test]
fn the_script_tries_the_command_line_then_app_bundles() {
    let script = fetch::script(fetch::INSTALL);
    assert!(script.contains("exec \"$sheprd\" update install --confirmed --json"));
    let sheprd = script.find(" sheprd ").unwrap();
    let bundle = script.find("/Applications/Sheprd.app").unwrap();
    assert!(sheprd < bundle, "the command line comes first");
    assert!(script.trim_end().ends_with("exit 127"));
}
