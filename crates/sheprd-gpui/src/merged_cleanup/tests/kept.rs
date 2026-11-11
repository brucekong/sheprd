//! Kept workspaces' merged cleanup: their linked checkouts qualify as
//! Herdr 1's do, and a busy agent there keeps one as it does there.

use super::*;
use crate::merged_cleanup::kept::kept_candidates;
use sheprd_session::{Keeper, Workspace, Worktree};

/// A repository key absolute on this platform, as `key_input` requires.
const KEY: &str = if cfg!(windows) {
    r"C:\src\app\.git"
} else {
    "/src/app/.git"
};

pub(super) fn kept(id: &str, linked: bool, keeper: Keeper) -> Workspace {
    Workspace {
        ahead_behind: None,
        branch: Some("feature".into()),
        cwd: format!("/src/{id}"),
        custom_label: false,
        id: id.into(),
        keeper,
        label: id.into(),
        status: AgentStatus::Idle,
        tabs: Vec::new(),
        tokens: Vec::new(),
        worktree: Some(Worktree {
            key: KEY.into(),
            label: "app".into(),
            linked,
        }),
    }
}

#[test]
fn only_kept_linked_checkouts_with_a_merged_pull_request_qualify() {
    let workspaces = [
        kept("kept-main", false, Keeper::App),
        kept("kept-lane", true, Keeper::App),
        kept("w9", true, Keeper::Herdr1),
    ];
    let pr = merged(State::Merged);
    let found: Vec<_> = kept_candidates(&workspaces, |_| Some(&pr)).collect();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].workspace, "kept-lane");
    assert_eq!(found[0].number, pr.number);
    assert_eq!(found[0].input.repo_key.as_deref(), Some(KEY));
    let open = merged(State::Open);
    assert_eq!(kept_candidates(&workspaces, |_| Some(&open)).count(), 0);
}

#[test]
fn a_busy_kept_agent_keeps_its_checkout() {
    let workspaces = [kept("kept-lane", true, Keeper::App)];
    let pr = merged(State::Merged);
    let candidate = kept_candidates(&workspaces, |_| Some(&pr)).next().unwrap();
    let report = Report {
        head: candidate.head.clone(),
        ..Report::default()
    };
    for busy in [AgentStatus::Working, AgentStatus::Blocked] {
        assert_eq!(
            decide_status(&candidate, &report, busy, false),
            Err(Skip::AgentBusy)
        );
    }
    assert_eq!(
        decide_status(&candidate, &report, AgentStatus::Idle, false),
        Ok(Action::Remove)
    );
    assert_eq!(
        decide_status(&candidate, &report, AgentStatus::Idle, true),
        Ok(Action::Ask)
    );
}
