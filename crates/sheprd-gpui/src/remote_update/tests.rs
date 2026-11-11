#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;

mod fetch;
mod follow;
mod recovery;

pub(super) fn offered(current: &str, latest: &str) -> Remote {
    Remote {
        phase: Phase::Offered,
        latest: Some(latest.into()),
        current: Some(current.into()),
        detail: None,
    }
}

pub(super) fn remote(phase: Phase, current: &str) -> Remote {
    Remote {
        phase,
        latest: None,
        current: Some(current.into()),
        detail: None,
    }
}

#[test]
fn states_the_command_line_names_map_to_phases() {
    for (state, phase) in [
        ("available", Phase::Offered),
        ("ready", Phase::Offered),
        ("homebrew", Phase::Offered),
        ("restart", Phase::Offered),
        ("checking", Phase::Working),
        ("downloading", Phase::Working),
        ("installing", Phase::Working),
        ("upgrading", Phase::Working),
        ("restarting", Phase::Working),
        ("cancelling", Phase::Working),
        ("current", Phase::Current),
        ("disabled", Phase::Off),
        ("error", Phase::Failed),
        ("idle", Phase::Idle),
        // A newer app's state is nothing to show, not an offer.
        ("something-new", Phase::Idle),
    ] {
        assert_eq!(Phase::parse(state), phase, "{state}");
    }
}
