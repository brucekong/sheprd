//! The window's side: which machines are read and when, the pill a header
//! shows, and the confirmed install.

use super::{Error, HostUpdate, Install, Pill, fetch};
use crate::{
    SheprdWindow,
    automations::{Machine, Wanted},
    endpoint::Endpoint,
    menu::Page,
    teleport::Host,
};
use gpui::{Context, Window};
use std::time::Instant;

#[cfg(test)]
mod tests;

impl SheprdWindow {
    /// Called on each tick: follows the machines the window reaches, and
    /// reads those that are due.
    pub(crate) fn poll_remote_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let wanted = self.remote_update_hosts();
        if self.remote_updates.sync(wanted) {
            if self.remote_updates.dialog.is_none() && self.menu.page == Some(Page::RemoteUpdate) {
                self.dismiss_menu(window, cx);
            }
            cx.notify();
        }
        let now = Instant::now();
        let due: Vec<Machine> = self
            .remote_updates
            .hosts
            .iter()
            .filter(|host| host.due(now))
            .map(|host| host.machine.clone())
            .collect();
        for machine in due {
            self.read_remote_update(&machine, cx);
        }
    }

    fn remote_update_hosts(&self) -> Vec<Wanted> {
        let mut wanted: Vec<Wanted> = self
            .automation_hosts()
            .into_iter()
            .filter(|wanted| !wanted.machine.is_local())
            .collect();
        // Terminal connectivity does not own an already confirmed update.
        // Keep its reads and result through reconnects, but stop tracking a
        // machine once all its endpoints are removed or disabled.
        for endpoint in self.endpoints.iter().filter(|endpoint| endpoint.enabled) {
            let Some(machine) = Machine::of(&endpoint.connection.target) else {
                continue;
            };
            if wanted.iter().any(|host| host.machine == machine)
                || !self
                    .remote_updates
                    .host(&machine)
                    .is_some_and(|host| host.install.is_some())
            {
                continue;
            }
            let Ok(host) = Host::new(&endpoint.connection.target) else {
                continue;
            };
            wanted.push(Wanted {
                machine,
                label: endpoint.label.clone(),
                endpoint: Some(endpoint.id.clone()),
                host,
            });
        }
        wanted
    }

    fn read_remote_update(&mut self, machine: &Machine, cx: &mut Context<Self>) {
        let cancelled = self.remote_updates.cancelled.clone();
        let Some(host) = self.remote_updates.host_mut(machine) else {
            return;
        };
        host.last = Some(Instant::now());
        let script_host = host.host.clone();
        let work = cx
            .background_executor()
            .spawn(async move { fetch::run(&script_host, fetch::STATUS, &cancelled) });
        let machine = machine.clone();
        host.task = Some(cx.spawn(async move |this, cx| {
            let read = work.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(host) = this.remote_updates.host_mut(&machine) {
                    host.take(read, Instant::now());
                    cx.notify();
                }
            });
        }));
    }

    /// The pill for `endpoint`'s header, when its machine has something to
    /// say about updating.
    pub(crate) fn remote_update_pill(&self, endpoint: &Endpoint) -> Option<(Machine, Pill)> {
        let machine = Machine::of(&endpoint.connection.target).filter(|m| !m.is_local())?;
        let pill = self.remote_updates.host(&machine)?.pill()?;
        Some((machine, pill))
    }

    /// The release `endpoint`'s machine offers, for `host list`.
    pub(crate) fn remote_update_offer(&self, endpoint: &Endpoint) -> Option<String> {
        match self.remote_update_pill(endpoint)? {
            (_, Pill::Offered { latest, .. }) => Some(latest.unwrap_or_else(|| "newer".into())),
            _ => None,
        }
    }

    /// Opens the confirmation for `machine`, unless another page holds
    /// the window. Whether it opened.
    pub(crate) fn ask_remote_update(
        &mut self,
        machine: &Machine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.remote_updates.host(machine).is_none()
            || self.menu.page.is_some()
            || !self.open_menu(window, cx)
        {
            return false;
        }
        self.remote_updates.dialog = Some(machine.clone());
        self.menu.page = Some(Page::RemoteUpdate);
        true
    }

    /// The user confirmed: asks the machine's Sheprd to update, then
    /// follows it until it runs the new release.
    pub(in crate::remote_update) fn confirm_remote_update(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let machine = self.remote_updates.dialog.take();
        self.dismiss_menu(window, cx);
        let Some(machine) = machine else {
            return;
        };
        let cancelled = self.remote_updates.cancelled.clone();
        let Some(host) = self.remote_updates.host_mut(&machine) else {
            return;
        };
        let from = host.remote().and_then(|remote| remote.current.clone());
        let script_host = host.host.clone();
        let work = cx
            .background_executor()
            .spawn(async move { fetch::run(&script_host, fetch::INSTALL, &cancelled) });
        let mut install = Install::new(from, Instant::now());
        let followed = machine.clone();
        install.task = Some(cx.spawn(async move |this, cx| {
            let answer = work.await;
            let _ = this.update(cx, |this, cx| {
                this.remote_update_requested(&followed, answer);
                cx.notify();
            });
        }));
        host.install = Some(install);
        cx.notify();
    }

    /// What the install request answered: the updater's state, which the
    /// following reads take on from, or why the machine refused.
    fn remote_update_requested(&mut self, machine: &Machine, answer: Result<super::Remote, Error>) {
        let Some(host) = self.remote_updates.host_mut(machine) else {
            return;
        };
        let Some(install) = host.install.as_mut() else {
            return;
        };
        install.task = None;
        match answer {
            Ok(remote) => {
                host.read = Some(Ok(remote));
                // Read again soon, not a whole interval after the last read.
                host.last = Some(Instant::now());
            }
            Err(error) => install.failed = Some(std::sync::Arc::new(error)),
        }
    }

    /// Forgets a failed update, so the pill offers the release again.
    pub(in crate::remote_update) fn dismiss_remote_update_failure(&mut self, machine: &Machine) {
        if let Some(host) = self.remote_updates.host_mut(machine)
            && host
                .install
                .as_ref()
                .is_some_and(|install| install.failed.is_some())
        {
            host.install = None;
            host.last = None;
        }
    }
}

impl HostUpdate {
    /// Whether the dialog may start an update: nothing is running.
    pub(in crate::remote_update) fn startable(&self) -> bool {
        self.install
            .as_ref()
            .is_none_or(|install| install.failed.is_some())
    }
}
