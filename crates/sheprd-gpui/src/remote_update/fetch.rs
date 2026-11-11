//! Running `sheprd update ... --json` on another machine and reading its
//! answer. Everything here blocks, so it runs on the background executor.
//!
//! The command line is the contract (`docs/cli.md`): one JSON document on
//! stdout, and on failure an exit code from its table with
//! `{"error": {"code", "message"}}`. A shell's 127 means no Sheprd was
//! found. The output is untrusted: bounded, parsed strictly, and every
//! string stripped of control characters and cut to a bound here, so
//! drawing never sees raw remote text.

use super::{Error, Remote};
use crate::teleport::Host;
use serde::Deserialize;
use sheprd_client::ScriptLimits;
use std::{sync::atomic::AtomicBool, time::Duration};

/// One status answer is a few hundred bytes.
const OUTPUT: u64 = 16 * 1024;
/// The command waits for the app's answer, which comes at once.
const IDLE: Duration = Duration::from_secs(30);
/// The longest version or reason kept.
const TEXT: usize = 300;

// Exit codes of `docs/cli.md`.
const INVALID: i32 = 2;
const NOT_RUNNING: i32 = 3;
const UNSUPPORTED: i32 = 8;
const NOT_FOUND: i32 = 127;

/// What `sheprd update status` asks.
pub(super) const STATUS: &[&str] = &["update", "status", "--json"];
/// What the confirmed install asks.
pub(super) const INSTALL: &[&str] = &["update", "install", "--confirmed", "--json"];

/// The script that runs the machine's Sheprd with `args`: the command line
/// on its `PATH` (Homebrew's, `~/.local/bin`, packages), else the app's
/// executable in a macOS bundle. Older installs still name it `herdr-gpui`
/// or `Herdr.app`.
pub(super) fn script(args: &[&str]) -> String {
    let args = args.join(" ");
    format!(
        // i18n: data
        r#"for sheprd in sheprd herdr-gpui /Applications/Sheprd.app/Contents/MacOS/sheprd "$HOME/Applications/Sheprd.app/Contents/MacOS/sheprd" /Applications/Herdr.app/Contents/MacOS/Herdr; do
  if command -v "$sheprd" >/dev/null 2>&1; then
    exec "$sheprd" {args}
  fi
done
exit {NOT_FOUND}
"#
    )
}

/// Runs the machine's Sheprd with `args` and reads its update answer.
pub(super) fn run(host: &Host, args: &[&str], cancelled: &AtomicBool) -> Result<Remote, Error> {
    let limits = ScriptLimits {
        output: OUTPUT,
        idle: IDLE,
    };
    let mut output = Vec::new();
    match host.stream(
        &script(args),
        std::io::empty(),
        &mut output,
        limits,
        cancelled,
    ) {
        Ok(_) => parse(&output),
        Err(error) => Err(classify(error, &output)),
    }
}

#[derive(Deserialize)]
struct Answer {
    state: String,
    #[serde(default)]
    latest: Option<String>,
    #[serde(default)]
    current: Option<String>,
    #[serde(default)]
    detail: Option<String>,
}

#[derive(Deserialize)]
struct Failure {
    error: FailureBody,
}

#[derive(Deserialize)]
struct FailureBody {
    #[serde(default)]
    message: String,
}

fn clean(text: Option<String>) -> Option<String> {
    text.map(|text| crate::notifications::safe_text(&text, TEXT))
        .filter(|text| !text.trim().is_empty())
}

/// One update answer from `output`.
pub(super) fn parse(output: &[u8]) -> Result<Remote, Error> {
    let answer: Answer = serde_json::from_slice(output).map_err(Error::Answer)?;
    Ok(Remote {
        phase: super::Phase::parse(&answer.state),
        latest: clean(answer.latest),
        current: clean(answer.current),
        detail: clean(answer.detail),
    })
}

/// A failed run as the pill explains it.
pub(super) fn classify(error: sheprd_client::Error, stdout: &[u8]) -> Error {
    let code = match &error {
        sheprd_client::Error::ScriptExit { status, .. } => status.code(),
        _ => None,
    };
    match code {
        Some(NOT_FOUND) => Error::Missing,
        Some(NOT_RUNNING) => Error::NotRunning,
        // An app from before `update install` takes no such verb.
        Some(INVALID) => Error::TooOld,
        Some(UNSUPPORTED) => refused(stdout).unwrap_or(Error::TooOld),
        Some(_) => refused(stdout).unwrap_or(Error::Run(error)),
        None => Error::Run(error),
    }
}

/// The app's own reason for refusing, when it printed one.
fn refused(stdout: &[u8]) -> Option<Error> {
    let failure: Failure = serde_json::from_slice(stdout).ok()?;
    clean(Some(failure.error.message)).map(Error::Refused)
}
