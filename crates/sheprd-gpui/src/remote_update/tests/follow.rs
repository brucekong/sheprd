//! Following a confirmed update until the machine runs the new release.
use super::*;

fn install(from: &str, started: Instant) -> Install {
    Install::new(Some(from.into()), started)
}

#[test]
fn a_new_version_ends_the_update_at_once() {
    let start = Instant::now();
    let mut update = install("20261101.1", start);
    let read = Ok(remote(Phase::Offered, "20261109.2"));
    assert!(matches!(settle(&mut update, &read, start), Settle::Done));
}

#[test]
fn an_old_state_right_after_the_request_does_not_end_it() {
    let start = Instant::now();
    let mut update = install("20261101.1", start);
    let mut failed = remote(Phase::Failed, "20261101.1");
    failed.detail = Some("offline".into());
    // The machine still reports the failed check from before the request.
    assert!(matches!(
        settle(
            &mut update,
            &Ok(failed.clone()),
            start + Duration::from_secs(5)
        ),
        Settle::Running
    ));
    assert!(matches!(
        settle(
            &mut update,
            &Ok(remote(Phase::Current, "20261101.1")),
            start
        ),
        Settle::Running
    ));
    // Once it was seen working, a failure is the update's own.
    assert!(matches!(
        settle(
            &mut update,
            &Ok(remote(Phase::Working, "20261101.1")),
            start
        ),
        Settle::Running
    ));
    assert!(matches!(
        settle(&mut update, &Ok(failed), start + Duration::from_secs(6)),
        Settle::Failed(Error::Update(detail)) if detail == "offline"
    ));
}

#[test]
fn after_the_grace_a_failure_or_current_ends_it() {
    let start = Instant::now();
    let later = start + GRACE;
    let mut update = install("20261101.1", start);
    assert!(matches!(
        settle(&mut update, &Ok(remote(Phase::Failed, "20261101.1")), later),
        Settle::Failed(Error::Update(_))
    ));
    let mut update = install("20261101.1", start);
    assert!(matches!(
        settle(
            &mut update,
            &Ok(remote(Phase::Current, "20261101.1")),
            later
        ),
        Settle::Done
    ));
}

#[test]
fn a_silent_machine_is_given_until_the_limit() {
    let start = Instant::now();
    let mut update = install("20261101.1", start);
    let gone = Err(Arc::new(Error::NotRunning));
    assert!(matches!(
        settle(&mut update, &gone, start + Duration::from_secs(120)),
        Settle::Running
    ));
    assert!(matches!(
        settle(&mut update, &gone, start + INSTALL_LIMIT),
        Settle::Failed(Error::Timeout)
    ));
}

fn host() -> HostUpdate {
    let target = sheprd_client::ConnectTarget::Ssh {
        target: "studio".into(),
        session: "default".into(),
    };
    HostUpdate::new(crate::automations::Wanted {
        machine: Machine::Ssh("studio".into()),
        label: "Studio".into(),
        endpoint: Some("ssh:studio".into()),
        host: Host::new(&target).unwrap(),
    })
}

#[test]
fn the_pill_offers_follows_and_clears() {
    let start = Instant::now();
    let mut host = host();
    assert!(host.pill().is_none(), "nothing before a read");
    host.take(Ok(offered("20261101.1", "20261109.2")), start);
    assert!(matches!(
        host.pill(),
        Some(Pill::Offered { latest: Some(latest), .. }) if latest == "20261109.2"
    ));
    assert!(host.startable());
    host.install = Some(Install::new(Some("20261101.1".into()), start));
    assert!(matches!(host.pill(), Some(Pill::Updating)));
    assert!(!host.startable());
    // Restarting: the machine stops answering, and the last answer stays.
    host.take(Err(Error::NotRunning), start + Duration::from_secs(60));
    assert!(matches!(host.pill(), Some(Pill::Updating)));
    assert!(host.remote().is_some());
    host.take(
        Ok(remote(Phase::Current, "20261109.2")),
        start + Duration::from_secs(90),
    );
    assert!(host.pill().is_none(), "updated: nothing left to say");
    assert!(host.install.is_none());
}

#[test]
fn a_failed_update_keeps_its_pill_and_may_be_tried_again() {
    let start = Instant::now();
    let mut host = host();
    host.take(Ok(offered("20261101.1", "20261109.2")), start);
    host.install = Some(Install::new(Some("20261101.1".into()), start));
    host.take(Err(Error::NotRunning), start + INSTALL_LIMIT);
    assert!(matches!(host.pill(), Some(Pill::Failed(error)) if matches!(*error, Error::Timeout)));
    assert!(host.startable());
    // Reads go back to their usual pace while it shows the failure.
    host.last = Some(start + INSTALL_LIMIT);
    assert!(!host.due(start + INSTALL_LIMIT + FOLLOW_EVERY));
}

#[test]
fn reads_follow_an_update_closely_and_a_missing_app_rarely() {
    let start = Instant::now();
    let mut host = host();
    assert!(host.due(start), "read at once the first time");
    host.last = Some(start);
    host.take(Err(Error::Missing), start);
    assert!(!host.due(start + READ_EVERY));
    assert!(host.due(start + MISSING_EVERY));
    host.take(Ok(offered("20261101.1", "20261109.2")), start);
    assert!(!host.due(start + FOLLOW_EVERY));
    assert!(host.due(start + READ_EVERY));
    host.install = Some(Install::new(None, start));
    assert!(host.due(start + FOLLOW_EVERY));
}

#[test]
fn a_note_says_why_nothing_is_offered() {
    let start = Instant::now();
    let mut host = host();
    assert!(host.note().is_some(), "unread");
    host.take(Err(Error::NotRunning), start);
    assert_eq!(
        host.note().as_deref(),
        Some("Sheprd is not running on that machine")
    );
    host.take(Ok(remote(Phase::Current, "20261109.2")), start);
    assert_eq!(host.note(), None);
}
