//! Meeting a computer's own daemon, which keeps its text shortcuts
//! (`sheprd_remote_control::control::shortcuts`). This computer's is one
//! socket away, or its file when no daemon runs; another machine's is asked
//! over SSH, as `device_tags` asks for its tags. Everything here blocks: run
//! it on the background executor.

use super::Error;
use crate::{device_tags::Daemon, remote, teleport::Host};
use sheprd_client::ScriptLimits;
use sheprd_remote_control::{
    app::shortcuts,
    control::shortcuts::{MAX_WIRE_BYTES, TextShortcuts},
};
use std::{sync::atomic::AtomicBool, time::Duration};

const IDLE: Duration = Duration::from_secs(20);

/// Finds the program without installing it, as `device_tags` does: sharing
/// shortcuts is no reason to put a daemon on a machine. A build older than
/// shortcuts refuses the subcommand and prints nothing, as a machine without
/// the program does. Standard input is drained so the sender is never left
/// writing to no one.
pub(crate) const SCRIPT: &str = "\
program=$(command -v sheprd-remote-control || command -v herdr-remote-control || true)
[ -n \"$program\" ] || program=\"$HOME/.local/bin/sheprd-remote-control\"
if [ -x \"$program\" ]; then \"$program\" shortcuts --offer 2>/dev/null || true; fi
cat >/dev/null
";

/// As [`SCRIPT`], reading the shortcuts without offering any.
pub(crate) const READ_SCRIPT: &str = "\
program=$(command -v sheprd-remote-control || command -v herdr-remote-control || true)
[ -n \"$program\" ] || program=\"$HOME/.local/bin/sheprd-remote-control\"
if [ -x \"$program\" ]; then \"$program\" shortcuts 2>/dev/null || true; fi
cat >/dev/null
";

/// Offers `offer`, if any, to the computer's daemon and returns the
/// shortcuts that stand, or `None` when the computer has nothing that keeps
/// them.
pub(super) fn exchange(
    daemon: &Daemon,
    offer: Option<TextShortcuts>,
    cancelled: &AtomicBool,
) -> Result<Option<TextShortcuts>, Error> {
    match daemon {
        Daemon::Local => {
            let Ok(state_dir) = remote::state_dir() else {
                return Ok(None);
            };
            Ok(Some(shortcuts::exchange(&state_dir, offer)?))
        }
        Daemon::Remote(target) => {
            let host = Host::new(target).map_err(|_| Error::Unsupported)?;
            let (script, input) = match &offer {
                Some(offer) => (SCRIPT, serde_json::to_vec(offer).map_err(Error::Answer)?),
                None => (READ_SCRIPT, Vec::new()),
            };
            let mut output = Vec::new();
            host.stream(
                script,
                input.as_slice(),
                &mut output,
                ScriptLimits {
                    output: MAX_WIRE_BYTES as u64,
                    idle: IDLE,
                },
                cancelled,
            )
            .map_err(Error::Run)?;
            parse(&output)
        }
    }
}

/// The host's answer, bounded; `None` when it printed nothing.
pub(crate) fn parse(output: &[u8]) -> Result<Option<TextShortcuts>, Error> {
    if output.iter().all(u8::is_ascii_whitespace) {
        return Ok(None);
    }
    let answer: TextShortcuts = serde_json::from_slice(output).map_err(Error::Answer)?;
    Ok(Some(TextShortcuts::new(
        answer.shortcuts,
        answer.updated_ms,
    )))
}
