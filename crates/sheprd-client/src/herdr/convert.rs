//! Moving a terminal agent into a chat: same pane, same conversation. The
//! agent's own CLI stops, and `sheprd-acp --resume` continues its session in
//! the shell it leaves behind. This is the policy the app and the remote
//! control daemon share; each asks Herdr in its own way and types the line.
//!
//! Only an idle agent moves: stopping one mid-turn would lose the turn.
//! Everything that could refuse is checked before the agent is touched, so
//! a refusal leaves it running. A CLI stops on Ctrl+C alone, which every
//! agent here takes as "clear the draft" and then "quit". A signal is not
//! used because Codex dies on SIGTERM with its terminal modes still set, and
//! the line typed next would reach the shell in the wrong keyboard mode; a
//! typed `/exit` is not used because a draft would be sent with it.

mod error;

pub use error::Error;

use super::{
    AgentKind,
    processes::agent_processes,
    snapshot::{Pane, PaneStatus, ProcessInfo, SessionKind},
};
use crate::protocol::{AgentStatus, ClientShellAgent, ClientShellSnapshot};
use sheprd_chat_protocol::{Agent, EndpointId, TOKEN, restore};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

/// How often the pane is asked whether its shell is back.
pub const POLL: Duration = Duration::from_millis(500);
/// How many times it is asked. Copilot takes about ten seconds to leave.
pub const ROUNDS: u32 = 60;
/// Ctrl+C presses sent, one each round: enough for a draft, a menu, and the
/// two presses that quit, with some to spare. Past these the agent only
/// needs time.
pub const INTERRUPTS: u32 = 8;

/// The agents that can move: Herdr reports their session, and either
/// `sheprd-acp` reads their log or their ACP server replays it. Herdr has no
/// session for Gemini, and nothing shows Pi's history yet, so it would open
/// as an empty chat.
pub fn convertible(agent: Agent) -> bool {
    match agent {
        Agent::Claude | Agent::Codex | Agent::Copilot | Agent::Opencode => true,
        Agent::Antigravity | Agent::Gemini | Agent::Pi => false,
    }
}

/// Whether the pane already runs a chat program, by the token it reports.
fn runs_chat(tokens: &[(String, String)]) -> bool {
    tokens
        .iter()
        .find(|(name, _)| name == TOKEN)
        .is_some_and(|(_, value)| value.parse::<EndpointId>().is_ok())
}

/// Whether the pane's agent is one that can move at all, however busy it is.
pub fn movable(reported: &ClientShellAgent) -> Result<Agent, Error> {
    if runs_chat(&reported.tokens) {
        return Err(Error::AlreadyChat);
    }
    let name = reported.agent.as_deref().ok_or(Error::NoAgent)?;
    let agent: Agent = name.parse().map_err(|_| Error::Unknown(name.to_owned()))?;
    if !convertible(agent) {
        return Err(Error::Unsupported(agent));
    }
    Ok(agent)
}

/// Whether what a client sees of a pane lets its agent move.
pub fn check(reported: &ClientShellAgent) -> Result<Agent, Error> {
    let agent = movable(reported)?;
    match reported.agent_status {
        AgentStatus::Idle | AgentStatus::Done => Ok(agent),
        AgentStatus::Working | AgentStatus::Blocked | AgentStatus::Unknown => {
            Err(Error::Busy(agent))
        }
    }
}

/// The pane of `tab` whose agent a tab's menu would move: the tab's focused
/// pane when that one can, else its first that can. A busy agent counts, so
/// the action stays and says why it has to wait rather than vanishing
/// mid-turn. The agent comes with it, to tell a pane that has since
/// changed hands.
pub fn tab_pane<'a>(
    snapshot: &'a ClientShellSnapshot,
    workspace: &str,
    tab: &str,
) -> Option<(&'a str, Agent)> {
    snapshot
        .agents
        .iter()
        .filter(|agent| agent.workspace_id == workspace && agent.tab_id == tab)
        .filter_map(|agent| Some((agent, movable(agent).ok()?)))
        .min_by_key(|(agent, _)| !agent.focused)
        .map(|(reported, agent)| (reported.pane_id.as_str(), agent))
}

/// Whether Herdr's own answer, fresher than a client's snapshot, still
/// shows `agent` settled in the pane. Only a turn known to be over may end:
/// an agent waiting for an answer, or one the daemon cannot place, stays.
pub fn settled(pane: &Pane, agent: Agent) -> Result<(), Error> {
    if pane.agent.as_deref() != Some(agent.name()) {
        return Err(Error::Moved);
    }
    match pane.status {
        PaneStatus::Done | PaneStatus::Idle => Ok(()),
        PaneStatus::Other | PaneStatus::Working => Err(Error::Busy(agent)),
    }
}

/// What continues the conversation, read from the running agent.
#[derive(Debug, PartialEq, Eq)]
pub struct Resume {
    pub agent: Agent,
    pub cwd: String,
    /// The original command line's flags, for an agent that takes them.
    pub flags: Vec<String>,
    pub session: String,
}

impl Resume {
    /// The arguments of `sheprd-acp`, none of them needing a shell.
    pub fn words(&self) -> Vec<&str> {
        let mut words = vec![
            "--resume",
            self.session.as_str(),
            "--cwd",
            self.cwd.as_str(),
            self.agent.name(),
        ];
        if !self.flags.is_empty() {
            words.push("--");
            words.extend(self.flags.iter().map(String::as_str));
        }
        words
    }

    /// The line typed into the pane's shell to run `program`, a path on
    /// the pane's own machine. Refusing happens here, before the agent
    /// stops, so a folder or flag no line can carry costs nothing.
    pub fn local_line(&self, program: &Path) -> Result<String, Error> {
        let program = program.to_str().ok_or(Error::Line)?;
        let mut words = vec![program];
        words.extend(self.words());
        crate::quoted_line(&words).ok_or(Error::Line)
    }
}

/// The session in the pane, as Herdr's own integration for `agent` reported
/// it, and in a form `sheprd-acp --resume` takes.
pub fn session(pane: &Pane, agent: Agent) -> Result<String, Error> {
    let reported = pane.agent_session.as_ref().ok_or(Error::NoSession(agent))?;
    AgentKind::parse(agent.name())
        .filter(|kind| kind.accepts(reported))
        .filter(|_| reported.kind == SessionKind::Id)
        .filter(|_| restore::session_id_is_plain(&reported.value))
        .map(|_| reported.value.clone())
        .ok_or(Error::NoSession(agent))
}

/// Whether the pane's shell, rather than a job it started, has the terminal.
pub fn shell_in_front(info: &ProcessInfo) -> bool {
    info.foreground_process_group_id.is_some() && info.foreground_process_group_id == info.shell_pid
}

/// What `info` says the pane's agent runs as, and the conversation to
/// continue. Claude's flags ride along, as the model the terminal agent was
/// started with; the other agents run a server with flags of its own.
pub fn resume(pane: &Pane, info: &ProcessInfo, agent: Agent) -> Result<Resume, Error> {
    let session = session(pane, agent)?;
    let processes = agent_processes(agent, info).unwrap_or_default();
    let process = processes.first().ok_or(Error::NotRunning(agent))?;
    let cwd = process
        .cwd
        .clone()
        .or_else(|| pane.foreground_cwd.clone())
        .or_else(|| pane.cwd.clone())
        .ok_or(Error::NotRunning(agent))?;
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

/// Stops the agent: Ctrl+C until the shell is back, at most [`INTERRUPTS`]
/// of them, then waits out the remaining rounds. It never presses Enter, so
/// nothing in the agent's input can be answered or sent. Once a key is sent
/// the agent may be leaving, so a question that fails, such as one dropped
/// SSH round trip, is asked again rather than abandoning the agent; a lost
/// connection ends the move through `cancelled`.
pub fn stop<E: From<Error>>(
    agent: Agent,
    mut returned: impl FnMut() -> Result<bool, E>,
    mut interrupt: impl FnMut() -> Result<(), E>,
    mut wait: impl FnMut(),
    cancelled: &AtomicBool,
) -> Result<(), E> {
    for round in 0..ROUNDS {
        if cancelled.load(Ordering::Acquire) {
            return Err(Error::Stopped.into());
        }
        match returned() {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(error) if round == 0 => return Err(error),
            Err(_) => {}
        }
        if round < INTERRUPTS {
            interrupt()?;
        }
        wait();
    }
    Err(Error::StillRunning(agent).into())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
