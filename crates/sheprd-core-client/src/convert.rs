//! Moving an agent in a herdr-core terminal into a chat: the policy of
//! `sheprd_client::herdr::convert`, asked of herdr-core instead of Herdr 1.
//! The app and the remote control daemon share it.
//!
//! Only an idle agent moves, and everything that could refuse is checked
//! before it is touched. It stops on Ctrl+C (`terminal.keys`) until the shell
//! that started it is back in front, and the `sheprd-acp --resume` line is
//! then typed into that shell (`terminal.send`). The line names the chat's
//! endpoint, and the terminal is labelled with it (`terminal.label`), as a
//! chat started in a terminal of its own is, so every client finds it.
//!
//! herdr-core reports no agent session, unlike Herdr 1's integrations. Claude
//! Code records its own in `~/.claude/sessions/<pid>.json`, by the process
//! herdr-core reports in front, so Claude moves; the other agents wait for
//! herdr-core to carry a session reference (for instance in their OSC 7501
//! records).

use crate::{Core, Result, agent_kind, backend::info, wire::Terminal};
use serde::Deserialize;
use sheprd_backend::{AgentStatus, Process, TerminalId, TerminalInfo};
use sheprd_chat_protocol::{Agent, ENDPOINT_ENV, EndpointId, TOKEN, restore};
use sheprd_client::herdr::{
    AgentKind,
    convert::{self, Resume},
};
use std::{
    io,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

/// The names a chat program runs under, as the OS reports them.
const CHAT_PROGRAMS: &[&str] = &["herdr-acp", "sheprd-acp"];

/// Herdr 1's pane variable, cleared so a chat in a herdr-core terminal never
/// reports to a Herdr 1 pane the core happened to inherit.
const PANE_ENV: &str = "HERDR_PANE_ID";

/// The chat endpoint `info` runs: its token label, unless herdr-core reports
/// another program than a chat in front. A terminal an agent moved out of
/// keeps the label once its chat ends and its shell is back in front.
pub fn chat_endpoint(info: &TerminalInfo) -> Option<EndpointId> {
    let endpoint = info.labels.get(TOKEN)?.parse().ok()?;
    let chat = info.foreground.as_ref().is_none_or(|process| {
        let name = process.name.strip_suffix(".exe").unwrap_or(&process.name);
        CHAT_PROGRAMS.contains(&name)
    });
    chat.then_some(endpoint)
}

/// Whether the terminal's agent is one that can move at all, however busy
/// it is.
pub fn movable(info: &TerminalInfo) -> Result<Agent, convert::Error> {
    if chat_endpoint(info).is_some() {
        return Err(convert::Error::AlreadyChat);
    }
    let kind = agent_kind(info).ok_or(convert::Error::NoAgent)?;
    let agent: Agent = kind
        .name()
        .parse()
        .map_err(|_| convert::Error::Unknown(kind.name().to_owned()))?;
    if !convert::convertible(agent) {
        return Err(convert::Error::Unsupported(agent));
    }
    Ok(agent)
}

/// Whether the terminal's agent may move now: movable, and between turns.
pub fn check(info: &TerminalInfo) -> Result<Agent, convert::Error> {
    let agent = movable(info)?;
    match info.agent {
        AgentStatus::Idle | AgentStatus::Done => Ok(agent),
        AgentStatus::Working | AgentStatus::Blocked | AgentStatus::Unknown => {
            Err(convert::Error::Busy(agent))
        }
    }
}

/// The process in front when it runs `agent`.
fn agent_process(info: &TerminalInfo, agent: Agent) -> Result<&Process, convert::Error> {
    info.foreground
        .as_ref()
        .filter(|process| runs(process, agent))
        .ok_or(convert::Error::NotRunning(agent))
}

/// Whether `process` is `agent` itself, not a shell or another program.
fn runs(process: &Process, agent: Agent) -> bool {
    AgentKind::identify(None, &process.name, &process.argv)
        .is_some_and(|kind| kind.name() == agent.name())
}

/// Whether the shell the terminal started has it again: the process in
/// front is the terminal's own program. Any other program in front, such as
/// one the agent's command line runs after it (`claude; less log`), must not
/// get the chat's line.
pub fn shell_back(record: &Terminal, agent: Agent) -> bool {
    record
        .pid
        .zip(record.foreground.as_ref())
        .is_some_and(|(pid, front)| front.pid == pid && !front_runs(front, agent))
}

/// Whether the process in front is `agent`.
fn front_runs(front: &crate::wire::Foreground, agent: Agent) -> bool {
    AgentKind::identify(None, &front.name, &front.argv)
        .is_some_and(|kind| kind.name() == agent.name())
}

/// Whether `agent` is the program the terminal started, so stopping it
/// ends the terminal rather than handing it back to a shell.
fn agent_is_program(record: &Terminal, agent: Agent) -> bool {
    record
        .pid
        .zip(record.foreground.as_ref())
        .is_some_and(|(pid, front)| front.pid == pid && front_runs(front, agent))
}

/// What Claude Code writes about one running CLI.
#[derive(Deserialize)]
struct ClaudeRecord {
    pid: u32,
    #[serde(rename = "sessionId")]
    session_id: String,
}

/// Where Claude Code records the CLI with process ID `pid`.
pub fn claude_record(home: &Path, pid: u32) -> PathBuf {
    home.join(".claude")
        .join("sessions")
        .join(format!("{pid}.json"))
}

/// The session the agent in `process` runs, from what `read` finds in the
/// record the agent keeps of itself. Only Claude Code keeps one; a record
/// of another process, or a session no line can carry, is none.
pub fn session(
    agent: Agent,
    process: &Process,
    home: &Path,
    read: impl FnOnce(&Path) -> io::Result<String>,
) -> Result<String, convert::Error> {
    let none = || convert::Error::NoCoreSession(agent);
    if agent != Agent::Claude {
        return Err(none());
    }
    let text = read(&claude_record(home, process.pid)).map_err(|_| none())?;
    let record: ClaudeRecord = serde_json::from_str(&text).map_err(|_| none())?;
    (record.pid == process.pid && restore::session_id_is_plain(&record.session_id))
        .then_some(record.session_id)
        .ok_or_else(none)
}

/// What continues the conversation, read from the running agent.
pub fn resume(
    info: &TerminalInfo,
    agent: Agent,
    home: &Path,
    read: impl FnOnce(&Path) -> io::Result<String>,
) -> Result<Resume, convert::Error> {
    let process = agent_process(info, agent)?;
    let session = session(agent, process, home, read)?;
    let cwd = info
        .cwd
        .as_deref()
        .and_then(Path::to_str)
        .filter(|cwd| !cwd.is_empty())
        .ok_or(convert::Error::NotRunning(agent))?
        .to_owned();
    let flags = match agent {
        Agent::Claude => AgentKind::Claude.kept_flags(&process.argv),
        _ => Vec::new(),
    };
    Ok(Resume {
        agent,
        cwd,
        flags,
        session,
    })
}

/// The line that starts `program` on `resume`'s session at `endpoint` in a
/// Unix shell, through `env`, so the variables reach it whatever the shell.
pub fn chat_line(
    resume: &Resume,
    program: &Path,
    endpoint: &EndpointId,
) -> Result<String, convert::Error> {
    let program = program.to_str().ok_or(convert::Error::Line)?;
    let endpoint = format!("{ENDPOINT_ENV}={endpoint}");
    let pane = format!("{PANE_ENV}=");
    let mut words = vec!["env", pane.as_str(), endpoint.as_str(), program];
    words.extend(resume.words());
    sheprd_client::quoted_line(&words).ok_or(convert::Error::Line)
}

/// What the core reports of `terminal` now, as it sends it.
fn record(core: &Core, terminal: &TerminalId) -> Result<Terminal> {
    core.list(false)?
        .terminals
        .into_iter()
        .find(|record| record.address == terminal.as_str())
        .ok_or_else(|| convert::Error::Moved.into())
}

/// Blocking: moves the agent in `terminal`, which a client last saw as
/// `agent`, into a chat run by `program`, a path on this machine. `home`
/// holds the agent's own records. Refusals before the agent stops leave it
/// running; `cancelled` ends the move between rounds. Returns the chat's
/// endpoint, which the terminal is labelled with.
pub fn run(
    core: &Core,
    terminal: &TerminalId,
    agent: Agent,
    program: &Path,
    home: &Path,
    cancelled: &AtomicBool,
) -> Result<EndpointId> {
    let raw = record(core, terminal)?;
    let found = info(&raw)?;
    if check(&found)? != agent {
        return Err(convert::Error::Moved.into());
    }
    // Refused before it is touched: stopping it would end the terminal.
    if agent_is_program(&raw, agent) {
        return Err(convert::Error::NoShell(agent).into());
    }
    let resume = resume(&found, agent, home, |path| std::fs::read_to_string(path))?;
    resume.folder_here()?;
    let endpoint = EndpointId::random();
    let line = chat_line(&resume, program, &endpoint)?;
    convert::stop(
        agent,
        || record(core, terminal).map(|now| shell_back(&now, agent)),
        || core.keys(terminal.as_str(), &["ctrl+c"]),
        || std::thread::sleep(convert::POLL),
        cancelled,
    )?;
    // Stopped once the agent left, as when chats were turned off: its
    // session is left to continue by hand, and no chat starts.
    if cancelled.load(std::sync::atomic::Ordering::Acquire) {
        return Err(convert::Error::Stopped.into());
    }
    core.send(&crate::wire::SendText {
        terminal: terminal.as_str(),
        text: &line,
        ..Default::default()
    })?;
    core.label(
        terminal.as_str(),
        &std::collections::BTreeMap::from([(TOKEN, endpoint.as_str())]),
    )?;
    Ok(endpoint)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
