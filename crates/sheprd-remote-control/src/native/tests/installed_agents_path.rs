//! The search for installed agents runs with the login shell's `PATH`, not
//! the user service's bare one, so agents a shell profile puts on it (mise,
//! asdf, nvm, ...) are offered.

use crate::native::create::list_agents;
use sheprd_chat_protocol::Agent;
use std::{fs, os::unix::fs::PermissionsExt, time::Duration};

/// Far longer than a script takes, and than the one login shell probe.
const PATIENT: Duration = Duration::from_secs(30);

#[test]
fn the_listing_sees_the_login_shells_path() {
    let login = sheprd_client::login_env::path().expect("a login PATH");
    let dir = tempfile::tempdir().unwrap();
    let expected = dir.path().join("expected-path");
    fs::write(&expected, login.as_encoded_bytes()).unwrap();
    let program = dir.path().join("sheprd-acp");
    let body = format!(
        "#!/bin/sh\nprintf %s \"$PATH\" | cmp -s - '{}' && echo opencode\n",
        expected.display()
    );
    fs::write(&program, body).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(list_agents(&program, PATIENT), Some(vec![Agent::Opencode]));
}
