//! Moving a terminal agent into a chat: same pane, same conversation. The
//! agent's own CLI stops, and `herdr-acp --resume` continues its session in
//! the shell it leaves behind; discovery then attaches the chat view as for
//! any chat. The session's history stays with the agent, which the program
//! reads back (`sheprd-acp`'s `history`, or an ACP agent's `session/load`).
//!
//! What may move, and how it stops, is [`sheprd_client::herdr::convert`],
//! shared with the remote control daemon. This module asks the window's
//! host through `herdr_cli`, on this machine or over SSH, and types the line
//! once the window still shows that host.

mod core;
mod error;

pub(crate) use core::Job as CoreJob;

pub use error::Error;

use super::{Route, existing};
use crate::{
    SheprdWindow,
    i18n::{Localize, fl},
    teleport::{self, Envelope, Host, ProcessInfo, ProcessInfoResult},
    window::Flash,
};
use gpui::{Context, Window};
use sheprd_chat_protocol::Agent;
use sheprd_client::{
    herdr::convert::{self, Resume},
    shell_quote,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

/// The line typed into the pane's shell: this machine's `program`, or for
/// `None` the one an SSH host finds for itself. Refusing happens here,
/// before the agent stops, so a folder or flag no line can carry costs
/// nothing.
fn line(resume: &Resume, program: Option<&Path>) -> Result<String, convert::Error> {
    match program {
        Some(program) => {
            resume.folder_here()?;
            resume.local_line(program)
        }
        None => remote_line(&resume.words()).ok_or(convert::Error::Line),
    }
}

/// The line for an SSH host, which finds its own program. The words ride
/// double-quoted inside a single-quoted script, so anything those read
/// specially makes no line.
fn remote_line(words: &[&str]) -> Option<String> {
    let special = |c: char| "'\"$`\\".contains(c) || c.is_control();
    if words.iter().any(|word| word.contains(special)) {
        return None;
    }
    let args: Vec<String> = words.iter().map(|word| format!("\"{word}\"")).collect();
    Some(format!(
        " sh -c '{}'",
        super::remote_program_script(&args.join(" "))
    ))
}

fn process_info(host: &Host, pane: &str, cancelled: &AtomicBool) -> Result<ProcessInfo, Error> {
    let pane = shell_quote(pane);
    // i18n: data
    let body = format!("herdr_cli pane process-info --pane {pane}\n");
    let bytes = existing::query(host, &body, cancelled)?;
    serde_json::from_slice::<Envelope<ProcessInfoResult>>(&bytes)
        .map(|answer| answer.result.process_info)
        .map_err(Error::Answer)
}

/// What the window asks of its worker.
struct Request {
    agent: Agent,
    pane: String,
    /// This machine's program, or `None` for the one on an SSH host.
    program: Option<PathBuf>,
    scope: existing::Scope,
}

/// The worker's whole job: the line to type once the agent has stopped.
fn run(request: &Request, cancelled: &AtomicBool) -> Result<String, Error> {
    let Request {
        agent,
        pane,
        program,
        scope,
    } = request;
    let host = teleport::host_for(&scope.target).map_err(existing::Error::from)?;
    let snapshot = existing::snapshot(&host, cancelled)?.snapshot;
    let found = snapshot.pane(pane).ok_or(convert::Error::Moved)?;
    convert::settled(found, *agent)?;
    let resume = convert::resume(found, &process_info(&host, pane, cancelled)?, *agent)?;
    let line = line(&resume, program.as_deref())?;
    if program.is_none() {
        // i18n: data
        let body = format!(" sh -c '{}'\n", super::remote_program_script("--version"));
        existing::query(&host, &body, cancelled).map_err(Error::HostProgram)?;
    }
    let keys = {
        let pane = shell_quote(pane);
        // i18n: data
        format!("herdr_cli pane send-keys {pane} ctrl+c\n")
    };
    convert::stop(
        *agent,
        || process_info(&host, pane, cancelled).map(|info| convert::shell_in_front(&info)),
        || Ok(existing::query(&host, &keys, cancelled).map(drop)?),
        || std::thread::sleep(convert::POLL),
        cancelled,
    )?;
    Ok(line)
}

/// An agent on its way into a chat. Dropping it stops the worker from going
/// further, as a window that closes or a pane that moves on must.
#[derive(Debug)]
pub(crate) struct Job {
    cancelled: Arc<AtomicBool>,
    pane: String,
    result: mpsc::Receiver<Result<String, Error>>,
    scope: existing::Scope,
}

#[cfg(test)]
impl Job {
    /// A job whose worker is `result`'s sender, and the flag dropping it sets.
    pub(in crate::chat) fn waiting(
        pane: &str,
        scope: existing::Scope,
    ) -> (
        Self,
        mpsc::SyncSender<Result<String, Error>>,
        Arc<AtomicBool>,
    ) {
        let (sender, result) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let job = Self {
            cancelled: cancelled.clone(),
            pane: pane.to_owned(),
            result,
            scope,
        };
        (job, sender, cancelled)
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

fn spawn(request: Request) -> Result<Job, Error> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    let (sender, result) = mpsc::sync_channel(1);
    let pane = request.pane.clone();
    let scope = request.scope.clone();
    std::thread::Builder::new()
        .name("herdr-chat-convert".into())
        .spawn(move || {
            let _ = sender.send(run(&request, &flag));
        })
        .map_err(Error::Worker)?;
    Ok(Job {
        cancelled,
        pane,
        result,
        scope,
    })
}

/// The chat key of the herdr-core pane with the keyboard, to move into a
/// chat in place of Herdr's focused pane; none when no herdr-core pane has
/// it. A pane on another host's core, or a mirror, is refused rather than
/// passed over: its terminal ID names another terminal on this computer's
/// core, and Herdr's focused pane is not the one the user is in.
pub(crate) fn focused_core_key(
    focused: Option<(crate::browser::TabId, sheprd_backend::TerminalId)>,
    drives: impl Fn(crate::browser::TabId) -> bool,
) -> Result<Option<String>, Error> {
    match focused {
        None => Ok(None),
        Some((tab, _)) if !drives(tab) => Err(Error::NotReachable),
        Some((_, terminal)) => Ok(Some(super::core_pane::key(&terminal))),
    }
}

impl SheprdWindow {
    /// Starts moving the agent in `pane`, else the focused pane, into a
    /// chat, and says which pane and agent. It finishes in the background.
    pub(crate) fn convert_to_chat(&mut self, pane: Option<&str>) -> Result<(String, Agent), Error> {
        if !self.config.features.acp_chats {
            return Err(Error::Disabled);
        }
        if self.chats.converting.is_some() || self.chats.converting_core.is_some() {
            return Err(Error::AlreadyConverting);
        }
        if let Some(terminal) = pane.and_then(core::terminal) {
            return self.convert_core_to_chat(terminal);
        }
        let program = match self.chat_route().ok_or(Error::NotReachable)? {
            // `herdr pane process-info` and the typed `sh` line are Unix.
            Route::Local if cfg!(windows) => return Err(Error::NotReachable),
            Route::Local => Some(self.chats.program.found()?.to_owned()),
            Route::Ssh(_) => None,
        };
        let snapshot = self
            .live
            .snapshot
            .as_deref()
            .filter(|_| self.live.status.is_connected())
            .ok_or(Error::NotConnected)?;
        let pane = pane
            .or(snapshot.focused_pane_id.as_deref())
            .ok_or(convert::Error::NoAgent)?
            .to_owned();
        let reported = snapshot
            .agents
            .iter()
            .find(|agent| agent.pane_id == pane)
            .ok_or(convert::Error::NoAgent)?;
        let agent = convert::check(reported)?;
        let endpoint = &self.endpoints[self.selected_endpoint];
        let scope = existing::Scope {
            target: endpoint.connection.target.clone(),
            boot: snapshot.boot_id.clone(),
            selection: self.selection_epoch,
            connection: endpoint.generation,
        };
        self.chats.converting = Some(spawn(Request {
            agent,
            pane: pane.clone(),
            program,
            scope,
        })?);
        Ok((pane, agent))
    }

    /// The pane a tab's menu offers to move into a chat, if this window could
    /// move one there. What only waiting fixes is left to the refusal.
    pub(crate) fn convertible_pane(&self, workspace: &str, tab: &str) -> Option<String> {
        if !self.config.features.acp_chats
            || matches!(self.chat_route()?, Route::Local if cfg!(windows))
        {
            return None;
        }
        let snapshot = self.live.snapshot.as_deref()?;
        convert::tab_pane(snapshot, workspace, tab).map(|(pane, _)| pane.to_owned())
    }

    /// [`Self::convert_to_chat`] for the pane with the keyboard, a herdr-core
    /// pane before Herdr's focused one, reporting a failure as a flash.
    pub(crate) fn convert_to_chat_or_flash(&mut self, window: &Window, cx: &mut Context<Self>) {
        let focused = self.core_terminals.focused_pane(window, cx);
        let core = focused_core_key(focused, |tab| self.core_terminals.drives(tab));
        if let Err(error) = core.and_then(|core| self.convert_to_chat(core.as_deref())) {
            self.flash_convert_error(&error, cx);
        }
        cx.notify();
    }

    fn flash_convert_error(&mut self, error: &Error, cx: &mut Context<Self>) {
        self.show_flash(
            Flash::warning(fl!("chat-convert-failed", error = error.localize())),
            cx,
        );
    }

    /// Why a move asked of `scope` must not go on, if it must not.
    fn convert_fence(&self, scope: &existing::Scope) -> Option<Error> {
        if !self.config.features.acp_chats {
            return Some(Error::Disabled);
        }
        (!self.convert_scope_current(scope)).then_some(Error::Stale)
    }

    /// Whether the window still shows the daemon `scope` was asked of.
    fn convert_scope_current(&self, scope: &existing::Scope) -> bool {
        self.live.status.is_connected()
            && self.selection_epoch == scope.selection
            && self
                .endpoints
                .get(self.selected_endpoint)
                .is_some_and(|endpoint| endpoint.generation == scope.connection)
            && self
                .live
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.boot_id == scope.boot)
    }

    /// Runs every tick: once the agent has stopped, types the line that
    /// continues it. The chat view attaches when the program reports.
    ///
    /// The fence comes before the worker's answer, not after it: a window
    /// that left the host could never type the line, so the worker must
    /// stop pressing keys there rather than finish stopping the agent.
    pub(super) fn poll_chat_convert(&mut self, cx: &mut Context<Self>) {
        if let Some(Err(error)) = self.poll_core_convert() {
            self.flash_convert_error(&error, cx);
            cx.notify();
        }
        let Some(job) = &self.chats.converting else {
            return;
        };
        let result = match self.convert_fence(&job.scope) {
            Some(error) => Err(error),
            None => match job.result.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => Err(convert::Error::Stopped.into()),
            },
        };
        let Some(job) = self.chats.converting.take() else {
            return;
        };
        let typed = result.and_then(|line| {
            self.type_script_line(&job.scope.boot, &job.pane, &line)
                .map_err(|error| Error::Type(Arc::new(error)))
        });
        if let Err(error) = typed {
            self.flash_convert_error(&error, cx);
        }
        cx.notify();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
