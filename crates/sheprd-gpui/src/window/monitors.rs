//! The background monitors the window polls as it ticks: plan usage, system
//! load, listening ports, this computer's ports, agent checkpoints, and port
//! forwards, each on the hosts it watches.
// i18n: migrated

use super::{Flash, SheprdWindow};
use crate::checkpoint;
use crate::i18n::{Localize, fl};
use gpui::Context;

impl SheprdWindow {
    /// Plan usage follows the selected host: a remote host reports its own
    /// agents' sign-ins, never this machine's.
    pub(super) fn update_usage(&mut self, cx: &mut Context<Self>) -> bool {
        let has_antigravity = self.chats.panes().any(|pane| {
            pane.available()
                && matches!(
                    pane.session
                        .transcript()
                        .agent
                        .to_ascii_lowercase()
                        .as_str(),
                    "antigravity" | "agy"
                )
        }) || self.endpoints.iter().any(|endpoint| {
            endpoint.live.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot.agents.iter().any(|agent| {
                    agent
                        .agent
                        .as_deref()
                        .is_some_and(|a| matches!(a, "antigravity" | "agy" | "antigravity-cli"))
                        || agent.display_agent.as_deref().is_some_and(|d| {
                            d.eq_ignore_ascii_case("antigravity") || d.eq_ignore_ascii_case("agy")
                        })
                        || agent.name.as_deref().is_some_and(|n| {
                            n.eq_ignore_ascii_case("antigravity") || n.eq_ignore_ascii_case("agy")
                        })
                })
            })
        });
        if has_antigravity
            && let Some(provider) = crate::usage::registry::find("antigravity")
            && !self.config.usage.shown(provider)
            && !self.config.usage.hidden(provider)
        {
            self.config
                .usage
                .show_providers
                .push(provider.id().to_owned());
            self.config_load_revision = self.config_load_revision.wrapping_add(1);
        }
        let host = self
            .config
            .usage
            .show
            .then(|| self.endpoints.get(self.selected_endpoint))
            .flatten()
            .and_then(|endpoint| crate::usage::Host::of(&endpoint.connection.target));
        let granted = crate::usage::KeychainGrants::granted(cx);
        let changed = self.usage.poll(
            host,
            &self.config.usage,
            &granted,
            self.config_load_revision,
            self.active,
            std::time::Instant::now(),
        );
        // A denied prompt takes the grant back, so a later launch does not
        // ask again in the background.
        for provider in self.usage.take_denied() {
            crate::usage::KeychainGrants::revoke(provider, cx);
        }
        changed
    }

    /// CPU, memory and GPU are sampled for every enabled host, while they
    /// are shown, a Devices overview is open, or a host picker ranks hosts by
    /// them.
    pub(super) fn update_system_load(&mut self) -> bool {
        if !self.config.show_system_load
            && !self.config.gpu_sampled()
            && !self.devices_overview_open()
            && !self.overview_shown()
            && !self.dispatch_sampling()
            && !self.orchestrator_sampling
        {
            return self.system_load.poll(Vec::new(), false);
        }
        let hosts = self.connected_hosts(crate::usage::Host::load_of);
        self.system_load.poll(hosts, self.config.gpu_sampled())
    }

    /// Listening ports are scanned on the same hosts as CPU and memory.
    pub(super) fn update_listening_ports(&mut self) -> bool {
        if !self.config.show_listening_ports {
            self.tunnels.close_all();
            return self.listening_ports.poll(Vec::new());
        }
        let hosts = self.watched_hosts();
        let mut changed = self.listening_ports.poll(hosts);
        // The shell shown in place of Herdr 1 is in the shown host's
        // stand-in workspace, when that host is this computer.
        let fallback = (!self.selected_is_remote())
            .then(|| crate::browser::scope(&self.endpoints[self.selected_endpoint]));
        let places = self
            .core_terminals
            .terminal_workspaces(fallback)
            .into_iter()
            .filter_map(|(host, terminal, place)| {
                Some((self.scanned_host(&host)?, terminal, place))
            })
            .collect();
        changed |= self.listening_ports.place_terminals(places);
        // A tunnel lives as long as its remote port is listed at the address
        // it dials and its SSH child runs; either ending removes its close
        // button.
        let ports = &self.listening_ports;
        let closed = self.tunnels.retain(|key| ports.tunnels_to(key));
        changed || closed
    }

    /// The host whose port scan finds the listeners of `host`'s core: this
    /// computer, or the machine the saved device reaches, while that one is
    /// a device. A cloud machine is not scanned.
    fn scanned_host(&self, host: &crate::core_terminals::Host) -> Option<crate::usage::Host> {
        match host {
            crate::core_terminals::Host::Local => Some(crate::usage::Host::Local),
            crate::core_terminals::Host::Remote(scope) => self
                .endpoints
                .iter()
                .find(|endpoint| crate::browser::scope(endpoint) == *scope)
                .and_then(|endpoint| crate::usage::Host::of(&endpoint.connection.target)),
        }
    }

    /// This computer's ports are listed while the Ports section is shown,
    /// or a command line request asked for them recently.
    pub(super) fn update_local_ports(&mut self) -> bool {
        let shown = self.activity_section() == Some(crate::sidebar::ActivityPanel::Ports);
        // The bar's count needs a scan only while the bar shows Ports.
        self.local_ports.counted = self.config.sidebar_style.ports_count
            && self.activity_bar_shown()
            && self.activity_panel_listed(crate::sidebar::ActivityPanel::Ports);
        self.local_ports.poll(shown, std::time::Instant::now())
    }

    /// The machines background monitors watch: this one always, a remote
    /// host while its Herdr 1 or its herdr-core is connected, so a dropped
    /// host is not dialled every few seconds.
    pub(super) fn watched_hosts(&self) -> Vec<crate::usage::Host> {
        self.connected_hosts(crate::usage::Host::of)
    }

    /// The hosts `host` finds for the enabled endpoints `watched_hosts` would
    /// watch. A cloud machine is only reached while it is connected, so a
    /// stopped one is never woken by a monitor.
    fn connected_hosts(
        &self,
        host: fn(&sheprd_client::ConnectTarget) -> Option<crate::usage::Host>,
    ) -> Vec<crate::usage::Host> {
        self.endpoints
            .iter()
            .enumerate()
            .filter(|(index, endpoint)| {
                let live = if *index == self.selected_endpoint {
                    &self.live
                } else {
                    &endpoint.live
                };
                // A host whose core answers while its Herdr 1 does not
                // keeps workspaces too, with ports and load of its own.
                let core = crate::core_terminals::Host::of(&crate::browser::scope(endpoint));
                endpoint.enabled
                    && (live.status.is_connected()
                        || !endpoint.connection.target.is_remote()
                        || self.core_terminals.answers(&core))
            })
            .filter_map(|(_, endpoint)| host(&endpoint.connection.target))
            .collect()
    }

    /// Agent turns are watched on every enabled, connected host this client
    /// may run Git on, and finished restores are reported wherever the user is.
    pub(super) fn update_checkpoints(&mut self, cx: &mut Context<Self>) {
        self.close_stale_checkpoints();
        let enabled = self.config.agent_checkpoints;
        let mut watched = Vec::new();
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            let live = if index == self.selected_endpoint {
                &self.live
            } else {
                &endpoint.live
            };
            let (true, true, Some(host), Some((boot, session))) = (
                enabled,
                endpoint.enabled && live.status.is_connected(),
                checkpoint::host_for(&endpoint.connection.target, live.local_daemon_peer),
                live.latest_session(),
            ) else {
                continue;
            };
            self.checkpoints
                .observe(&endpoint.id, &host, boot, &session);
            watched.push(endpoint.id.clone());
        }
        // Herdr 2's agents on this computer, whichever host is shown, watched
        // as an endpoint of their own so a turn is told apart from Herdr 1's.
        if let (true, Some(host), Some(index)) = (
            enabled,
            checkpoint::host_for(&sheprd_client::ConnectTarget::Local, true),
            self.core_host(),
        ) {
            let mut sessions = self.sessions(index);
            // A Herdr 2 tab in a Herdr 1 workspace borrows that workspace's
            // checkout, which Git then reads here: only a daemon proven to
            // run on this computer may name one.
            let live = if index == self.selected_endpoint {
                &self.live
            } else {
                &self.endpoints[index].live
            };
            if !names_local_checkouts(&self.endpoints[index].connection.target, live) {
                sessions.herdr_1 = None;
            }
            if let Some(session) = crate::session_model::core_with_checkouts(&sessions) {
                let watch = format!("{}/herdr-2", self.endpoints[index].id);
                self.checkpoints.observe(&watch, &host, "herdr-2", &session);
                watched.push(watch);
            }
        }
        self.checkpoints
            .retain_endpoints(|endpoint| watched.iter().any(|watched| watched == endpoint));
        let (changed, restored) = self.checkpoints.poll();
        for restored in restored {
            let flash = match restored.result {
                Ok(()) => Flash::success(fl!(
                    "window-notices-checkpoint-restored",
                    branch = restored.checkout.branch
                )),
                Err(error) => Flash::warning(not_restored(&error)),
            };
            self.show_flash(flash, cx);
        }
        if changed {
            cx.notify();
        }
    }

    /// Forwards outlive a dropped connection, since SSH may still reach the
    /// host, but end once their host is removed or disabled. A report the
    /// user did not just ask for is flashed.
    pub(crate) fn update_port_forwards(&mut self, cx: &mut Context<Self>) {
        let endpoints = &self.endpoints;
        let mut changed = self.port_forwards.retain_hosts(|target| {
            endpoints.iter().any(|endpoint| {
                endpoint.enabled
                    && endpoint
                        .saved_ssh()
                        .is_some_and(|(saved, _)| saved == target)
            })
        });
        let notices = self.port_forwards.poll();
        if let Some(notice) = notices.last() {
            let flash = match notice {
                crate::port_forward::Notice::Listening { .. } => Flash::success(notice.text()),
                crate::port_forward::Notice::Ended { .. } => Flash::warning(notice.text()),
            };
            self.show_flash(flash, cx);
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }

    /// The machine the selected endpoint runs on, for its load.
    pub(crate) fn selected_host(&self) -> Option<crate::usage::Host> {
        self.endpoints
            .get(self.selected_endpoint)
            .and_then(|endpoint| crate::usage::Host::load_of(&endpoint.connection.target))
    }

    /// Update cloud activity timestamps for active cloud device connections.
    #[cfg(feature = "azure")]
    pub(super) fn update_cloud_activity(&mut self) {
        for endpoint in &self.endpoints {
            if let sheprd_client::ConnectTarget::Cloud {
                provider: sheprd_client::CloudProvider::Azure,
                id,
                ..
            } = &endpoint.connection.target
                && endpoint.live.status.is_connected()
            {
                crate::azure::record_activity(id);
            }
        }
    }
}

/// Whether the checkouts `target`'s Herdr 1 names are on this computer, so
/// Git may run on them here: a socket may forward another machine's daemon.
pub(super) fn names_local_checkouts(
    target: &sheprd_client::ConnectTarget,
    live: &crate::state::LiveState,
) -> bool {
    checkpoint::host_for(target, live.local_daemon_peer) == Some(crate::usage::Host::Local)
}

/// Why a restore failed: the error's message, then the cause a failed Git
/// script or checkpoint worker carries, so Git's own output or the SSH
/// failure stays in view. Git's and GitHub's own errors already keep their
/// output in their messages.
pub(super) fn not_restored(error: &sheprd_vcs::Error) -> String {
    let cause = match error {
        sheprd_vcs::Error::Checkpoint(checkpoint::Error::Script(cause)) => cause.localize(),
        // The system's own words, kept as they are in every language.
        sheprd_vcs::Error::Checkpoint(checkpoint::Error::Thread(cause)) => cause.to_string(),
        _ => {
            return fl!(
                "window-notices-checkpoint-not-restored",
                error = error.localize()
            );
        }
    };
    fl!(
        "window-notices-checkpoint-not-restored-cause",
        error = error.localize(),
        cause = cause
    )
}

#[cfg(test)]
mod tests;
