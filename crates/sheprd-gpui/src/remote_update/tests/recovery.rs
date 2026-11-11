//! Late success must reconcile a failure without hiding an unchanged version.
use super::*;

#[test]
fn a_new_version_clears_a_timeout_or_a_lost_install_reply() {
    let start = Instant::now();
    for error in [Error::Timeout, Error::NotRunning] {
        let failure = Arc::new(error);
        for phase in [Phase::Current, Phase::Offered, Phase::Working] {
            let mut host = HostUpdate::new(crate::automations::Wanted {
                machine: Machine::Ssh("studio".into()),
                label: "Studio".into(),
                endpoint: Some("ssh:studio".into()),
                host: Host::new(&sheprd_client::ConnectTarget::Ssh {
                    target: "studio".into(),
                    session: "default".into(),
                })
                .unwrap(),
            });
            let mut install = Install::new(Some("20261101.1".into()), start);
            install.failed = Some(failure.clone());
            host.install = Some(install);
            let later = start + INSTALL_LIMIT + READ_EVERY;

            // Neither an unavailable app nor an old `current` erases the
            // original error, even after the install deadline.
            host.take(Err(Error::NotRunning), later);
            host.take(Ok(remote(Phase::Current, "20261101.1")), later);
            assert!(
                matches!(host.pill(), Some(Pill::Failed(error)) if Arc::ptr_eq(&error, &failure))
            );

            host.take(Ok(remote(phase, "20261109.2")), later);
            assert!(host.install.is_none(), "{phase:?}");
            assert!(!matches!(host.pill(), Some(Pill::Failed(_))));
            assert_eq!(
                host.remote().unwrap().current.as_deref(),
                Some("20261109.2")
            );
        }
    }
}
