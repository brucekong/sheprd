use super::*;

#[test]
fn the_search_path_includes_version_manager_shims() {
    let home = Path::new("/home/me");
    let path = search_path(Some(OsStr::new("/usr/bin")), Some(home));
    let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    assert!(dirs.contains(&home.join(".local/share/mise/shims")));
    assert!(dirs.contains(&home.join(".asdf/shims")));
}

/// A user service's bare `PATH` still finds an agent mise installed, as
/// the remote control daemon's new workspace form needs.
#[cfg(unix)]
#[test]
fn an_agent_in_mise_shims_is_installed_under_a_bare_path() {
    let home = tempfile::tempdir().unwrap();
    let shims = home.path().join(".local/share/mise/shims");
    std::fs::create_dir_all(&shims).unwrap();
    install(&shims, "opencode");
    install(&shims, "pi");
    let path = search_path(Some(OsStr::new("/nonexistent")), Some(home.path()));
    let found = installed(Some(&path));
    assert!(found.contains(&Agent::Opencode));
    assert!(found.contains(&Agent::Pi));
}
