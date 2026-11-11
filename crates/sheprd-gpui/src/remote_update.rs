//! Sheprd on the other machines the window reaches, kept up to date from
//! here: a machine whose own Sheprd offers a newer release gets a pill on
//! its sidebar header, and the user who clicks it and confirms in this app's
//! dialog has that machine update itself, without a screen there.
//!
//! This app never copies a release onto the machine. It runs `sheprd update
//! status --json` there over SSH (or the cloud provider's shell) on the
//! background executor, at most every [`READ_EVERY`], and after the user
//! confirms, `sheprd update install --confirmed --json`, which hands the
//! update to that app's own updater: it downloads the release, checks it
//! against the key it embeds, installs it, and restarts, as a paired phone's
//! request does. The machine's terminals and agents keep running.
// i18n: migrated

mod dialog;
mod error;
mod fetch;
#[cfg(test)]
mod tests;
mod window;

pub use error::Error;

use crate::{automations::Machine, teleport::Host};
use gpui::Task;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// How often a machine's Sheprd is read. Its own updater checks for
/// releases on its schedule; this only follows what it found.
const READ_EVERY: Duration = Duration::from_secs(30 * 60);
/// How often a machine without Sheprd is looked at again.
const MISSING_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// How often a machine is read while it updates.
const FOLLOW_EVERY: Duration = Duration::from_secs(5);
/// Downloading, installing, and restarting take longer than this only when
/// something went wrong.
const INSTALL_LIMIT: Duration = Duration::from_secs(20 * 60);
/// Right after the request the machine may still report the state from
/// before it: an old failure, or `current` before it checked again. Those
/// end the update only once it was seen working, or after this long.
const GRACE: Duration = Duration::from_secs(30);

/// What the machine's updater is doing, from the state `update status`
/// names (`docs/cli.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    /// A newer release waits: to download, install, or restart into.
    Offered,
    /// Checking, downloading, installing, or restarting.
    Working,
    Current,
    /// Updates are off in that build.
    Off,
    /// No check has finished yet.
    Idle,
    /// The last check or update failed.
    Failed,
}

impl Phase {
    /// The state's name as the command line prints it; an unknown name,
    /// from a newer app, is treated as nothing to show.
    pub(crate) fn parse(state: &str) -> Self {
        match state {
            "available" | "ready" | "homebrew" | "restart" => Self::Offered,
            "checking" | "downloading" | "installing" | "upgrading" | "restarting"
            | "cancelling" => Self::Working,
            "current" => Self::Current,
            "disabled" => Self::Off,
            "error" => Self::Failed,
            _ => Self::Idle,
        }
    }
}

/// One answer from the machine's Sheprd, its text already cleaned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Remote {
    pub(crate) phase: Phase,
    pub(crate) latest: Option<String>,
    /// The version it runs; absent from apps that predate reporting it.
    pub(crate) current: Option<String>,
    pub(crate) detail: Option<String>,
}

/// An update the user confirmed, followed until it ends.
pub(crate) struct Install {
    started: Instant,
    /// The version the machine ran when it was asked.
    from: Option<String>,
    /// The request itself, while it runs.
    task: Option<Task<()>>,
    /// Whether a read showed the updater working since.
    seen_working: bool,
    failed: Option<Arc<Error>>,
}

impl Install {
    fn new(from: Option<String>, started: Instant) -> Self {
        Self {
            started,
            from,
            task: None,
            seen_working: false,
            failed: None,
        }
    }
}

/// Where an update stands after a read.
#[derive(Debug)]
enum Settle {
    Running,
    Done,
    Failed(Error),
}

/// What `read` at `now` says about `install`.
fn settle(install: &mut Install, read: &Result<Remote, Arc<Error>>, now: Instant) -> Settle {
    let elapsed = now.saturating_duration_since(install.started);
    if let Ok(remote) = read {
        if remote.phase == Phase::Working {
            install.seen_working = true;
        }
        // The app restarted as another version: whatever it says now is
        // about the release it runs.
        let moved = install
            .from
            .as_ref()
            .zip(remote.current.as_ref())
            .is_some_and(|(from, current)| from != current);
        if moved {
            return Settle::Done;
        }
        // A failed request may still have reached the updater. Only a new
        // running version clears that failure; an old `current` does not.
        if install.failed.is_some() {
            return Settle::Running;
        }
        let settled = install.seen_working || elapsed >= GRACE;
        match remote.phase {
            Phase::Current if settled => return Settle::Done,
            Phase::Failed if settled => {
                return Settle::Failed(Error::Update(remote.detail.clone().unwrap_or_default()));
            }
            Phase::Off => {
                return Settle::Failed(Error::Refused(remote.detail.clone().unwrap_or_default()));
            }
            _ => {}
        }
    }
    // A machine not answering is most likely restarting into the release.
    if elapsed >= INSTALL_LIMIT {
        return Settle::Failed(Error::Timeout);
    }
    Settle::Running
}

/// What the machine's sidebar header shows.
#[derive(Clone, Debug)]
pub(crate) enum Pill {
    /// A newer release waits; a click asks to install it.
    Offered {
        latest: Option<String>,
        current: Option<String>,
    },
    Updating,
    Failed(Arc<Error>),
}

/// One machine's Sheprd, as last read.
pub(crate) struct HostUpdate {
    pub(crate) machine: Machine,
    pub(crate) label: String,
    pub(crate) endpoint: Option<String>,
    host: Host,
    read: Option<Result<Remote, Arc<Error>>>,
    /// When the last read started, which the next one is timed from.
    last: Option<Instant>,
    task: Option<Task<()>>,
    install: Option<Install>,
}

impl HostUpdate {
    fn new(wanted: crate::automations::Wanted) -> Self {
        Self {
            machine: wanted.machine,
            label: wanted.label,
            endpoint: wanted.endpoint,
            host: wanted.host,
            read: None,
            last: None,
            task: None,
            install: None,
        }
    }

    /// The last good answer.
    pub(crate) fn remote(&self) -> Option<&Remote> {
        self.read.as_ref()?.as_ref().ok()
    }

    pub(crate) fn pill(&self) -> Option<Pill> {
        if let Some(install) = &self.install {
            return Some(match &install.failed {
                Some(error) => Pill::Failed(error.clone()),
                None => Pill::Updating,
            });
        }
        let remote = self.remote()?;
        (remote.phase == Phase::Offered).then(|| Pill::Offered {
            latest: remote.latest.clone(),
            current: remote.current.clone(),
        })
    }

    /// Why nothing is offered, when it is not simply that the machine runs
    /// the latest release: for `host update`, whose words are English.
    pub(crate) fn note(&self) -> Option<String> {
        match &self.read {
            None => Some("not read yet; try again shortly".into()), // i18n: data
            Some(Err(error)) => Some(error.to_string()),
            Some(Ok(remote)) => match remote.phase {
                Phase::Off => Some(match &remote.detail {
                    Some(detail) => format!("updates are off in that build: {detail}"), // i18n: data
                    None => "updates are off in that build".into(), // i18n: data
                }),
                Phase::Failed => Some(match &remote.detail {
                    Some(detail) => format!("its last update check failed: {detail}"), // i18n: data
                    None => "its last update check failed".into(),                     // i18n: data
                }),
                Phase::Idle | Phase::Working => Some("it has not finished checking".into()), // i18n: data
                Phase::Current | Phase::Offered => None,
            },
        }
    }

    /// Whether the machine is being actively followed, before a failure.
    fn following(&self) -> bool {
        self.install
            .as_ref()
            .is_some_and(|install| install.failed.is_none())
    }

    fn due(&self, now: Instant) -> bool {
        if self.task.is_some()
            || self
                .install
                .as_ref()
                .is_some_and(|install| install.task.is_some())
        {
            return false;
        }
        let every = if self.following() {
            FOLLOW_EVERY
        } else if matches!(&self.read, Some(Err(error)) if matches!(**error, Error::Missing)) {
            MISSING_EVERY
        } else {
            READ_EVERY
        };
        self.last.is_none_or(|last| now >= last + every)
    }

    /// Keeps a read, and moves a followed update on.
    fn take(&mut self, read: Result<Remote, Error>, now: Instant) {
        self.task = None;
        let read = read.map_err(Arc::new);
        if let Some(install) = self.install.as_mut() {
            match settle(install, &read, now) {
                Settle::Running => {}
                Settle::Done => self.install = None,
                Settle::Failed(error) => {
                    install.failed.get_or_insert_with(|| Arc::new(error));
                }
            }
        }
        // A machine restarting keeps showing what it said before.
        let restarting =
            self.following() && matches!(&read, Err(error) if matches!(**error, Error::NotRunning));
        if !restarting {
            self.read = Some(read);
        }
    }
}

/// Every machine's Sheprd in a window.
#[derive(Default)]
pub(crate) struct RemoteUpdates {
    hosts: Vec<HostUpdate>,
    /// The machine the open confirmation is about.
    pub(crate) dialog: Option<Machine>,
    /// Tells reads in flight to stop when the window goes.
    cancelled: Arc<AtomicBool>,
}

impl Drop for RemoteUpdates {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl RemoteUpdates {
    pub(crate) fn host(&self, machine: &Machine) -> Option<&HostUpdate> {
        self.hosts.iter().find(|host| host.machine == *machine)
    }

    fn host_mut(&mut self, machine: &Machine) -> Option<&mut HostUpdate> {
        self.hosts.iter_mut().find(|host| host.machine == *machine)
    }

    /// Keeps the machines the window reaches now, and what was read of
    /// those it already had. Whether anything changed.
    fn sync(&mut self, wanted: Vec<crate::automations::Wanted>) -> bool {
        let same = wanted.len() == self.hosts.len()
            && wanted.iter().zip(&self.hosts).all(|(wanted, host)| {
                wanted.machine == host.machine
                    && wanted.label == host.label
                    && wanted.endpoint == host.endpoint
            });
        if same {
            return false;
        }
        let mut old = std::mem::take(&mut self.hosts);
        self.hosts = wanted
            .into_iter()
            .map(
                |wanted| match old.iter().position(|host| host.machine == wanted.machine) {
                    Some(index) => {
                        let mut host = old.swap_remove(index);
                        host.label = wanted.label;
                        host.endpoint = wanted.endpoint;
                        host.host = wanted.host;
                        host
                    }
                    None => HostUpdate::new(wanted),
                },
            )
            .collect();
        if self
            .dialog
            .as_ref()
            .is_some_and(|machine| self.host(machine).is_none())
        {
            self.dialog = None;
        }
        true
    }

    /// Installs a read for tests outside the module, as one would land.
    #[cfg(test)]
    pub(crate) fn install_for_test(
        &mut self,
        wanted: crate::automations::Wanted,
        read: Result<Remote, Error>,
    ) {
        let machine = wanted.machine.clone();
        if self.host(&machine).is_none() {
            self.hosts.push(HostUpdate::new(wanted));
        }
        if let Some(host) = self.host_mut(&machine) {
            host.last = Some(Instant::now());
            host.take(read, Instant::now());
        }
    }
}
