//! A session whose folder is gone is refused before its agent stops.

use super::*;

fn in_folder(cwd: &str) -> Resume {
    Resume {
        agent: Agent::Claude,
        cwd: cwd.to_owned(),
        flags: Vec::new(),
        session: SESSION.to_owned(),
    }
}

#[test]
fn a_folder_that_exists_is_kept() {
    let here = std::env::temp_dir();
    let resume = in_folder(here.to_str().unwrap());
    assert!(resume.folder_here().is_ok());
}

#[test]
fn a_removed_worktree_is_refused_with_its_path() {
    let gone = std::env::temp_dir().join("sheprd-convert-no-such-worktree-9df4");
    let resume = in_folder(gone.to_str().unwrap());
    let error = resume.folder_here().unwrap_err();
    assert!(matches!(&error, Error::FolderGone(path) if *path == resume.cwd));
    assert!(
        error
            .to_string()
            .contains("sheprd-convert-no-such-worktree-9df4")
    );
}

#[test]
fn a_file_is_not_a_folder() {
    let file = std::env::current_exe().unwrap();
    assert!(matches!(
        in_folder(file.to_str().unwrap()).folder_here(),
        Err(Error::FolderGone(_))
    ));
}
