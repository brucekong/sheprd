#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    remote_update::{FOLLOW_EVERY, INSTALL_LIMIT, tests::offered},
    sidebar::layout_tests::fixture_window,
};
use sheprd_client::ConnectTarget;

fn seed(view: &mut SheprdWindow, start: Instant) -> Machine {
    let target = ConnectTarget::Ssh {
        target: "studio".into(),
        session: "default".into(),
    };
    let machine = Machine::of(&target).unwrap();
    let mut endpoint = Endpoint::new("ssh:studio".into(), "Studio".into(), target, true);
    endpoint.never_connect();
    endpoint.live.status = crate::state::ConnectionStatus::Connected;
    endpoint.live.snapshot = view.live.snapshot.clone();
    view.endpoints.push(endpoint);
    let wanted = view.remote_update_hosts();
    view.remote_updates.sync(wanted);
    let host = view.remote_updates.host_mut(&machine).unwrap();
    host.last = Some(start);
    host.take(Ok(offered("20261101.1", "20261109.2")), start);
    machine
}

#[gpui::test]
fn disconnect_and_reconnect_keep_the_install_and_its_deadline(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update_in(cx, |view, window, cx| {
        let start = Instant::now();
        let machine = seed(view, start);
        let host = view.remote_updates.host_mut(&machine).unwrap();
        let mut install = Install::new(Some("20261101.1".into()), start);
        install.seen_working = true;
        // Keep the actual request task, not just the displayed state.
        install.task = Some(cx.spawn(async |_, _| std::future::pending::<()>().await));
        host.install = Some(install);
        view.endpoints[1].live.snapshot = None;
        view.poll_remote_updates(window, cx);
        view.poll_remote_updates(window, cx);
        let host = view.remote_updates.host_mut(&machine).unwrap();
        let install = host.install.as_mut().unwrap();
        assert_eq!(install.started, start);
        assert!(install.seen_working);
        assert!(install.task.is_some());
        install.task = None;
        assert!(host.due(start + FOLLOW_EVERY));

        // The deadline still expires while the terminal is disconnected.
        host.take(Err(Error::NotRunning), start + INSTALL_LIMIT);
        host.last = Some(start + INSTALL_LIMIT);
        view.poll_remote_updates(window, cx);
        assert!(matches!(
            view.remote_updates.host(&machine).unwrap().pill(),
            Some(Pill::Failed(error)) if matches!(*error, Error::Timeout)
        ));
        view.endpoints[1].live.snapshot = view.live.snapshot.clone();
        view.poll_remote_updates(window, cx);
        let host = view.remote_updates.host(&machine).unwrap();
        assert_eq!(host.install.as_ref().unwrap().started, start);
        assert!(matches!(host.pill(), Some(Pill::Failed(_))));
        assert_eq!(
            view.remote_updates
                .hosts
                .iter()
                .filter(|host| host.machine == machine)
                .count(),
            1,
            "reconnecting reuses the same machine"
        );
    });
}

#[gpui::test]
fn disconnect_closes_an_unconfirmed_dialog_and_restores_focus(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update_in(cx, |view, window, cx| {
        let machine = seed(view, Instant::now());
        assert!(view.ask_remote_update(&machine, window, cx));
        assert!(view.menu.focus.is_focused(window));
        view.endpoints[1].live.snapshot = None;
        view.poll_remote_updates(window, cx);
        assert!(view.remote_updates.host(&machine).is_none());
        assert!(view.remote_updates.dialog.is_none());
        assert!(view.menu.page.is_none());
        assert!(view.focus.is_focused(window));
    });
}

#[gpui::test]
fn removed_or_disabled_hosts_drop_tracking_and_close_their_dialog(cx: &mut gpui::TestAppContext) {
    for remove in [false, true] {
        let (view, cx) = cx.add_window_view(fixture_window);
        view.update_in(cx, |view, window, cx| {
            let start = Instant::now();
            let machine = seed(view, start);
            view.remote_updates.host_mut(&machine).unwrap().install =
                Some(Install::new(Some("20261101.1".into()), start));
            assert!(view.ask_remote_update(&machine, window, cx));
            if remove {
                view.endpoints.pop();
            } else {
                view.endpoints[1].enabled = false;
            }
            view.poll_remote_updates(window, cx);
            assert!(view.remote_updates.host(&machine).is_none());
            assert!(view.remote_updates.dialog.is_none());
            assert!(view.menu.page.is_none());
            assert!(view.focus.is_focused(window));
        });
    }
}
