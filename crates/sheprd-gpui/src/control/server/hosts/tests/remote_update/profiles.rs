//! Endpoint aliases resolve before looking up the machine-wide update state.
use super::*;

#[gpui::test]
fn a_second_session_profile_opens_the_shared_update_by_id_or_label(cx: &mut TestAppContext) {
    for name in ["ssh:studio-work", "Studio Work", "studio work"] {
        let (view, cx) = window(cx);
        install(&view, cx, Ok(read(Phase::Offered)));
        view.update_in(cx, |view, _, _| {
            profile(view, "ssh:studio-work", "Studio Work", "studio", "work");
            let wanted = view.automation_hosts();
            let shared = wanted
                .iter()
                .find(|host| host.machine == Machine::Ssh("studio".into()))
                .unwrap();
            assert_eq!(shared.endpoint.as_deref(), Some("ssh:studio"));
            assert!(
                wanted
                    .iter()
                    .all(|host| host.endpoint.as_deref() != Some("ssh:studio-work"))
            );
        });
        let response = run(&view, cx, Some(name));
        assert!(
            matches!(&response, Response::Host {
                reply: Reply::Update { latest: Some(latest), note: None, .. }
            } if latest == "20261109.2"),
            "{name}: {response:?}"
        );
        view.read_with(cx, |view, _| {
            assert_eq!(view.menu.page, Some(Page::RemoteUpdate));
            let machine = Machine::Ssh("studio".into());
            assert_eq!(view.remote_updates.dialog.as_ref(), Some(&machine));
            assert!(matches!(
                view.remote_updates.host(&machine).unwrap().pill(),
                Some(crate::remote_update::Pill::Offered { .. })
            ));
        });
    }
}

#[gpui::test]
fn an_endpoint_id_wins_over_another_machines_label(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    install(&view, cx, Ok(read(Phase::Offered)));
    view.update_in(cx, |view, _, _| {
        // The shared machine's retained label collides with another host's ID.
        view.endpoints[1].label = "ssh:other".into();
        let mut wanted = studio();
        wanted.label = "ssh:other".into();
        view.remote_updates = Default::default();
        view.remote_updates
            .install_for_test(wanted, Ok(read(Phase::Offered)));
        profile(view, "ssh:other", "Other", "other", "default");
    });
    // Other has no update state. Never open Studio's confirmation instead.
    let response = run(&view, cx, Some("ssh:other"));
    assert_eq!(error_code(&response), Some(ErrorCode::NotFound));
    view.read_with(cx, |view, _| {
        assert!(view.menu.page.is_none());
        assert!(view.remote_updates.dialog.is_none());
    });
}

#[gpui::test]
fn a_disabled_alias_cannot_open_another_profiles_shared_update(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    install(&view, cx, Ok(read(Phase::Offered)));
    view.update_in(cx, |view, _, _| {
        profile(view, "ssh:studio-work", "Studio Work", "studio", "work");
        view.endpoints.last_mut().unwrap().enabled = false;
    });
    let response = run(&view, cx, Some("ssh:studio-work"));
    assert_eq!(error_code(&response), Some(ErrorCode::InvalidRequest));
    view.read_with(cx, |view, _| assert!(view.menu.page.is_none()));
}
