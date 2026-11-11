//! An agent whose folder is gone, as a removed worktree's, keeps running:
//! `sheprd-acp` could not start there, so no Ctrl+C is sent.

use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_in_a_removed_worktree_is_left_running() {
    let dir = tempfile::tempdir().unwrap();
    let requests = serve_in(dir.path(), "idle", "/sheprd-no-such-worktree-9df4");
    let hub = hub(
        layout(Agent::Claude),
        Some(dir.path().join("h-client.sock")),
    );
    let error = hub
        .convert_to_chat("t2-p1", Agent::Claude)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, Error::Refused(convert::Error::FolderGone(path)) if path == "/sheprd-no-such-worktree-9df4"),
        "{error:?}"
    );
    let methods: Vec<_> = requests.try_iter().map(|(method, _)| method).collect();
    assert!(
        !methods.iter().any(|method| method == "pane.send_keys"),
        "{methods:?}"
    );
    assert!(hub.moving.claim("t2-p1").is_ok(), "the claim is released");
}
