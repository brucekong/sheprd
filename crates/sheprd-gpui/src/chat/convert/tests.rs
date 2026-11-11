//! The line for an SSH host; the shared policy is tested in `sheprd-client`.

use super::*;

fn resume(cwd: &str) -> Resume {
    Resume {
        agent: Agent::Claude,
        cwd: cwd.to_owned(),
        flags: Vec::new(),
        session: "0c6f0c39-3c1e-4b7e-9a51-6d1b8e2d7f10".to_owned(),
    }
}

#[test]
fn the_remote_line_finds_the_hosts_program() {
    let line = line(&resume("/my work"), None).unwrap();
    assert!(line.starts_with(" sh -c 'for p in "));
    assert!(line.contains(
        "exec \"$p\" \"--resume\" \"0c6f0c39-3c1e-4b7e-9a51-6d1b8e2d7f10\" \"--cwd\" \"/my work\" \"claude\""
    ));
}

#[test]
fn the_local_line_runs_this_apps_program() {
    // The folder must exist here: a local line refuses one that is gone.
    let folder = std::env::temp_dir();
    let line = line(
        &resume(folder.to_str().unwrap()),
        Some(Path::new("/app/sheprd-acp")),
    )
    .unwrap();
    assert!(line.contains("'/app/sheprd-acp' '--resume'"), "{line}");
}

#[test]
fn a_local_line_refuses_a_removed_worktree_before_the_agent_stops() {
    let gone = std::env::temp_dir().join("sheprd-no-such-worktree-9df4");
    assert!(matches!(
        line(
            &resume(gone.to_str().unwrap()),
            Some(Path::new("/app/sheprd-acp"))
        ),
        Err(convert::Error::FolderGone(_))
    ));
}

#[test]
fn an_ssh_hosts_folder_is_not_checked_here() {
    // The folder is on the host, so this machine cannot say it is gone.
    assert!(line(&resume("/sheprd-no-such-worktree-9df4"), None).is_ok());
}

#[test]
fn a_folder_no_remote_line_can_carry_is_refused() {
    for cwd in ["/$HOME", "/`id`", "/a\"b", "/a\\b", "/it's"] {
        assert!(matches!(
            line(&resume(cwd), None),
            Err(convert::Error::Line)
        ));
    }
}
