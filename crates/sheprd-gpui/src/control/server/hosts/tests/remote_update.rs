//! `host update` opens the confirmation for a machine whose Sheprd offers a
//! release, and nothing more: the machine is asked only once the user
//! confirms in the dialog.
use super::*;
use crate::automations::{Machine, Wanted};
use crate::control::protocol::hosts::Reply;
use crate::control::server::hosts::update::ask;
use crate::menu::Page;
use crate::remote_update::{Error, Phase, Remote};
use crate::teleport::Host;

mod profiles;

fn profile(view: &mut SheprdWindow, id: &str, label: &str, target: &str, session: &str) {
    let mut endpoint = crate::endpoint::Endpoint::new(
        id.into(),
        label.into(),
        sheprd_client::ConnectTarget::Ssh {
            target: target.into(),
            session: session.into(),
        },
        true,
    );
    endpoint.never_connect();
    endpoint.live.status = crate::state::ConnectionStatus::Connected;
    endpoint.live.snapshot = view.live.snapshot.clone();
    view.endpoints.push(endpoint);
}

fn studio() -> Wanted {
    Wanted {
        machine: Machine::Ssh("studio".into()),
        label: "Studio".into(),
        endpoint: Some("ssh:studio".into()),
        host: Host::new(&sheprd_client::ConnectTarget::Ssh {
            target: "studio".into(),
            session: "default".into(),
        })
        .unwrap(),
    }
}

fn read(phase: Phase) -> Remote {
    Remote {
        phase,
        latest: Some("20261109.2".into()),
        current: Some("20261101.1".into()),
        detail: None,
    }
}

fn install(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, read: Result<Remote, Error>) {
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            if !view
                .endpoints
                .iter()
                .any(|endpoint| endpoint.id == "ssh:studio")
            {
                profile(view, "ssh:studio", "Studio", "studio", "default");
            }
            view.remote_updates.install_for_test(studio(), read)
        });
    });
}

fn run(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, host: Option<&str>) -> Response {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            ask(view, &caller(Some("w3")), host, window, cx)
        })
    })
}

#[gpui::test]
fn an_offered_release_opens_the_confirmation_and_nothing_runs(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    install(&view, cx, Ok(read(Phase::Offered)));
    let response = run(&view, cx, Some("Studio"));
    let Response::Host {
        reply: Reply::Update { host, latest, .. },
    } = &response
    else {
        panic!("{response:?}");
    };
    assert_eq!(
        (host.as_str(), latest.as_deref()),
        ("Studio", Some("20261109.2"))
    );
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::RemoteUpdate));
        let machine = Machine::Ssh("studio".into());
        assert_eq!(view.remote_updates.dialog, Some(machine.clone()));
        // Asking is all the command does: the machine is not updating.
        let pill = view.remote_updates.host(&machine).unwrap().pill();
        assert!(
            matches!(pill, Some(crate::remote_update::Pill::Offered { .. })),
            "{pill:?}"
        );
    });
    // A second request while the dialog is open is refused, not stacked.
    let response = run(&view, cx, Some("ssh:studio"));
    assert_eq!(error_code(&response), Some(ErrorCode::WindowBusy));
}

#[gpui::test]
fn nothing_to_install_says_why_and_opens_nothing(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    install(&view, cx, Err(Error::NotRunning));
    let response = run(&view, cx, Some("Studio"));
    let Response::Host {
        reply: Reply::Update { latest, note, .. },
    } = &response
    else {
        panic!("{response:?}");
    };
    assert_eq!(latest, &None);
    assert_eq!(
        note.as_deref(),
        Some("Sheprd is not running on that machine")
    );
    install(&view, cx, Ok(read(Phase::Current)));
    let response = run(&view, cx, Some("Studio"));
    assert!(
        matches!(
            &response,
            Response::Host {
                reply: Reply::Update {
                    latest: None,
                    note: None,
                    ..
                }
            }
        ),
        "{response:?}"
    );
    view.read_with(cx, |view, _| assert_eq!(view.menu.page, None));
}

#[gpui::test]
fn an_unknown_host_or_this_computer_is_refused(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    install(&view, cx, Ok(read(Phase::Offered)));
    let response = run(&view, cx, Some("buildbox"));
    assert_eq!(error_code(&response), Some(ErrorCode::NotFound));
    // The caller's workspace is on this computer, which updates itself.
    let response = run(&view, cx, None);
    assert_eq!(error_code(&response), Some(ErrorCode::Unsupported));
    view.read_with(cx, |view, _| assert_eq!(view.menu.page, None));
}
