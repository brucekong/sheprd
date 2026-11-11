//! Moving a terminal agent into a chat for a phone, as Sheprd's Move Agent
//! into Chat does: the policy is `sheprd_client::herdr::convert`'s, and
//! this asks Herdr through its API socket. The agent stops on Ctrl+C, then
//! `sheprd-acp --resume` continues its session in the same pane, which the
//! hub then follows as any chat.
//!
//! A Herdr 2 terminal moves the same way through
//! `sheprd_core_client::convert`, on the daemon's own core connection.

mod error;

pub use error::Error;

use super::{ApiMethod, Hub, core};
use serde::Deserialize;
use serde_json::{Value, json};
use sheprd_chat_protocol::Agent;
use sheprd_client::herdr::{
    convert,
    snapshot::{Pane, ProcessInfo},
};
use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
};

pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;

/// The panes a request is moving now, so two cannot press keys in one.
#[derive(Debug, Default)]
pub(super) struct Moving(Mutex<HashSet<String>>);

/// Holds `pane` in [`Moving`] while its move runs, and cancels the stop loop
/// when dropped: a phone that gives up mid-move stops the presses too.
struct Claim<'a> {
    cancelled: Arc<AtomicBool>,
    moving: &'a Moving,
    pane: String,
}

impl Moving {
    fn claim(&self, pane: &str) -> Result<Claim<'_>> {
        let mut panes = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if !panes.insert(pane.to_owned()) {
            return Err(Error::AlreadyConverting);
        }
        Ok(Claim {
            cancelled: Arc::new(AtomicBool::new(false)),
            moving: self,
            pane: pane.to_owned(),
        })
    }
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        self.moving
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.pane);
    }
}

#[derive(Deserialize)]
struct PaneAnswer {
    pane: Pane,
}

#[derive(Deserialize)]
struct ProcessAnswer {
    process_info: ProcessInfo,
}

fn parse<T: for<'de> Deserialize<'de>>(answer: Value) -> Result<T> {
    serde_json::from_value(answer).map_err(Error::Answer)
}

impl Hub {
    /// Moves the agent in terminal `pane` into a chat in the same pane, and
    /// returns once the line that continues it is typed. Every refusal comes
    /// before the agent is touched. The pane must be the one its tab offers
    /// (`Terminal::convert`), and still with `expected`, the agent the phone
    /// was shown there: a pane whose agent was replaced since is refused
    /// rather than stopping an agent the person never chose.
    pub(crate) async fn convert_to_chat(
        self: &Arc<Self>,
        pane: &str,
        expected: Agent,
    ) -> Result<()> {
        // The typed line runs `sh`, as Sheprd's does.
        if cfg!(windows) {
            return Err(Error::Platform);
        }
        let params = json!({"pane_id": pane});
        let agent = self
            .layout()
            .terminal(pane)
            .and_then(|(_, terminal)| terminal.convert.clone())
            .filter(|(offered, _)| offered == pane)
            .map(|(_, agent)| agent)
            .ok_or(convert::Error::NoAgent)?;
        if agent != expected {
            return Err(convert::Error::Moved.into());
        }
        let program = self.program().ok_or(super::Error::ProgramMissing)?;
        let claim = self.moving.claim(pane)?;
        if core::owns(&params) {
            let core = self
                .core
                .clone()
                .ok_or(super::Error::from(core::Error::Disconnected))?;
            let home = self
                .home
                .clone()
                .or_else(sheprd_client::local_path::home_dir)
                .ok_or(Error::Home)?;
            let (owned, cancelled) = (pane.to_owned(), claim.cancelled.clone());
            tokio::task::spawn_blocking(move || {
                core.convert(&owned, agent, &program, &home, &cancelled)
            })
            .await
            .map_err(|_| Error::Worker)??;
            drop(claim);
            return Ok(());
        }
        let found: PaneAnswer = parse(self.call(ApiMethod::PaneGet, params.clone()).await?)?;
        convert::settled(&found.pane, agent)?;
        let info: ProcessAnswer = parse(
            self.call(ApiMethod::PaneProcessInfo, params.clone())
                .await?,
        )?;
        let resume = convert::resume(&found.pane, &info.process_info, agent)?;
        resume.folder_here()?;
        let line = resume.local_line(&program)?;
        let (hub, owned, cancelled) = (self.clone(), pane.to_owned(), claim.cancelled.clone());
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || hub.stop_agent(&runtime, &owned, agent, &cancelled))
            .await
            .map_err(|_| Error::Worker)??;
        self.run_line(pane, &line).await?;
        drop(claim);
        Ok(())
    }

    /// The shared stop loop, run on the blocking pool with each question
    /// and press sent through the runtime.
    fn stop_agent(
        &self,
        runtime: &tokio::runtime::Handle,
        pane: &str,
        agent: Agent,
        cancelled: &AtomicBool,
    ) -> Result<()> {
        let params = json!({"pane_id": pane});
        let keys = json!({"pane_id": pane, "keys": ["ctrl+c"]});
        convert::stop(
            agent,
            || {
                let answer =
                    runtime.block_on(self.call(ApiMethod::PaneProcessInfo, params.clone()))?;
                let info: ProcessAnswer = parse(answer)?;
                Ok(convert::shell_in_front(&info.process_info))
            },
            || {
                runtime.block_on(self.call(ApiMethod::PaneSendKeys, keys.clone()))?;
                Ok(())
            },
            || std::thread::sleep(convert::POLL),
            cancelled,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
