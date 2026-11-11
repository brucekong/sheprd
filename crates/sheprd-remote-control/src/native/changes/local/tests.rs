use super::*;
use std::path::PathBuf;

fn run(repository: &Path, args: &[&str]) {
    let status = sheprd_client::new_command("git")
        .current_dir(repository)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args([
            "-c",
            "init.defaultBranch=main",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// A repository on `main` with `a.txt` and `old.txt` committed.
fn repository(parent: &Path) -> PathBuf {
    let repository = parent.join("project");
    std::fs::create_dir(&repository).unwrap();
    run(&repository, &["init", "--quiet"]);
    std::fs::write(repository.join("a.txt"), "one\ntwo\nthree\n").unwrap();
    std::fs::write(repository.join("old.txt"), "kept\n".repeat(20)).unwrap();
    run(&repository, &["add", "."]);
    run(&repository, &["commit", "--quiet", "-m", "start"]);
    repository
}

fn listed(answer: Answer) -> FileList {
    match answer {
        Answer::Files(list) => list,
        other => panic!("{other:?}"),
    }
}

fn file<'a>(list: &'a FileList, path: &str) -> &'a ChangedFile {
    list.files.iter().find(|file| file.path == path).unwrap()
}

#[test]
fn uncommitted_changes_list_edits_renames_and_untracked_files() {
    let dir = tempfile::tempdir().unwrap();
    let repository = repository(dir.path());
    std::fs::write(repository.join("a.txt"), "one\n2\nthree\nfour\n").unwrap();
    run(&repository, &["mv", "old.txt", "new.txt"]);
    std::fs::write(repository.join("notes.md"), "hello\nworld\n").unwrap();
    std::fs::write(repository.join("blob.bin"), [0_u8, 1, 2]).unwrap();

    let wanted = Wanted::Files {
        scope: Scope::Uncommitted,
    };
    let list = listed(answer(&repository, &wanted));
    assert_eq!(list.total, 4, "{list:?}");
    assert_eq!(list.base, None);
    let edited = file(&list, "a.txt");
    assert_eq!(edited.status, FileStatus::Modified);
    assert_eq!((edited.added, edited.removed), (Some(2), Some(1)));
    let renamed = file(&list, "new.txt");
    assert_eq!(renamed.status, FileStatus::Renamed);
    assert_eq!(renamed.old_path.as_deref(), Some("old.txt"));
    let notes = file(&list, "notes.md");
    assert_eq!(
        (notes.status, notes.added),
        (FileStatus::Untracked, Some(2))
    );
    assert_eq!(file(&list, "blob.bin").shown, Shown::Binary);
}

#[test]
fn a_listed_files_diff_is_numbered_and_another_path_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let repository = repository(dir.path());
    std::fs::write(repository.join("a.txt"), "one\n2\nthree\n").unwrap();
    std::fs::write(repository.join("notes.md"), "hello\n").unwrap();
    let scope = Scope::Uncommitted;
    let Answer::Diff(diff) = answer(
        &repository,
        &Wanted::Diff {
            scope,
            path: "a.txt".into(),
        },
    ) else {
        panic!()
    };
    let kinds: Vec<LineKind> = diff.lines.iter().map(|line| line.kind).collect();
    assert_eq!(
        kinds,
        [
            LineKind::Hunk,
            LineKind::Context,
            LineKind::Removed,
            LineKind::Added,
            LineKind::Context
        ]
    );
    assert_eq!(diff.lines[2].text, "two");
    assert_eq!((diff.lines[2].old, diff.lines[2].new), (Some(2), None));
    assert_eq!((diff.lines[3].old, diff.lines[3].new), (None, Some(2)));
    assert_eq!((diff.lines[4].old, diff.lines[4].new), (Some(3), Some(3)));

    let Answer::Diff(untracked) = answer(
        &repository,
        &Wanted::Diff {
            scope,
            path: "notes.md".into(),
        },
    ) else {
        panic!()
    };
    assert_eq!(untracked.lines[1].text, "hello");
    assert_eq!(untracked.lines[1].kind, LineKind::Added);

    for path in ["old.txt", "../secret", "/etc/hosts"] {
        let wanted = Wanted::Diff {
            scope,
            path: path.into(),
        };
        assert_eq!(
            answer(&repository, &wanted),
            Answer::Failed(Failure::NoFile)
        );
    }
}

#[test]
fn a_branch_is_compared_with_its_base_uncommitted_work_included() {
    let dir = tempfile::tempdir().unwrap();
    let repository = repository(dir.path());
    run(&repository, &["switch", "--quiet", "-c", "feature"]);
    std::fs::write(repository.join("b.txt"), "b\n").unwrap();
    run(&repository, &["add", "b.txt"]);
    run(&repository, &["commit", "--quiet", "-m", "b"]);
    std::fs::write(repository.join("a.txt"), "changed\n").unwrap();
    let list = listed(answer(
        &repository,
        &Wanted::Files {
            scope: Scope::Branch,
        },
    ));
    assert_eq!(list.base.as_deref(), Some("main"));
    assert_eq!(file(&list, "b.txt").status, FileStatus::Added);
    assert_eq!(file(&list, "a.txt").status, FileStatus::Modified);
}

#[test]
fn a_folder_outside_git_and_github_questions_say_so() {
    let dir = tempfile::tempdir().unwrap();
    let wanted = Wanted::Files {
        scope: Scope::Uncommitted,
    };
    assert_eq!(
        answer(dir.path(), &wanted),
        Answer::Failed(Failure::NoRepository)
    );
    assert_eq!(
        answer(dir.path(), &Wanted::Checks),
        Answer::Checks(Default::default())
    );
    assert_eq!(
        answer(dir.path(), &Wanted::CheckLog { job: 1 }),
        Answer::Failed(Failure::NoLog)
    );
}

#[test]
fn a_repository_without_a_commit_lists_its_files_and_has_no_base() {
    let dir = tempfile::tempdir().unwrap();
    let repository = dir.path().join("fresh");
    std::fs::create_dir(&repository).unwrap();
    run(&repository, &["init", "--quiet"]);
    std::fs::write(repository.join("first.txt"), "x\n").unwrap();
    let list = listed(answer(
        &repository,
        &Wanted::Files {
            scope: Scope::Uncommitted,
        },
    ));
    assert_eq!(list.total, 1);
    let wanted = Wanted::Files {
        scope: Scope::Branch,
    };
    assert_eq!(
        answer(&repository, &wanted),
        Answer::Failed(Failure::NoBase)
    );
}

#[test]
fn hunk_headers_give_each_sides_start() {
    assert_eq!(hunk_start("@@ -3,4 +5,6 @@ fn main"), Some((3, 5)));
    assert_eq!(hunk_start("@@ -0,0 +1 @@"), Some((0, 1)));
    assert_eq!(hunk_start("diff --git a/x b/x"), None);
}

fn diff_of(repository: &Path, path: &str) -> FileDiff {
    let wanted = Wanted::Diff {
        scope: Scope::Uncommitted,
        path: path.into(),
    };
    match answer(repository, &wanted) {
        Answer::Diff(diff) => diff,
        other => panic!("{other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn an_untracked_link_is_never_followed_out_of_the_checkout() {
    let dir = tempfile::tempdir().unwrap();
    let repository = repository(dir.path());
    let secret = dir.path().join("secret.txt");
    std::fs::write(&secret, "token\n").unwrap();
    std::os::unix::fs::symlink(&secret, repository.join("link.txt")).unwrap();
    let list = listed(answer(
        &repository,
        &Wanted::Files {
            scope: Scope::Uncommitted,
        },
    ));
    assert_eq!(file(&list, "link.txt").shown, Shown::Unreadable);
    let diff = diff_of(&repository, "link.txt");
    assert!(diff.lines.is_empty());
    assert_eq!(diff.shown, Shown::Unreadable);
}

#[test]
fn a_workspace_in_a_subfolder_shows_the_checkouts_paths_and_their_diffs() {
    let dir = tempfile::tempdir().unwrap();
    let repository = repository(dir.path());
    let sub = repository.join("sub");
    std::fs::create_dir(&sub).unwrap();
    std::fs::write(sub.join("b.txt"), "b\n").unwrap();
    run(&repository, &["add", "."]);
    run(&repository, &["commit", "--quiet", "-m", "sub"]);
    std::fs::write(sub.join("b.txt"), "c\n").unwrap();
    std::fs::write(sub.join("new.txt"), "n\n").unwrap();
    let list = listed(answer(
        &sub,
        &Wanted::Files {
            scope: Scope::Uncommitted,
        },
    ));
    assert_eq!(file(&list, "sub/new.txt").added, Some(1));
    let diff = diff_of(&sub, "sub/b.txt");
    assert!(
        diff.lines
            .iter()
            .any(|line| line.kind == LineKind::Added && line.text == "c")
    );
    assert_eq!(diff_of(&sub, "sub/new.txt").lines[1].text, "n");
}

#[test]
fn colors_and_patterns_in_names_leave_a_diff_as_git_wrote_it() {
    // Both patterns match a.txt unless Git treats the path literally.
    // Brackets are valid Windows filename characters; a star is not.
    let names: &[&str] = &[
        "[a].txt",
        #[cfg(unix)]
        "a*.txt",
    ];
    for &name in names {
        let dir = tempfile::tempdir().unwrap();
        let repository = repository(dir.path());
        run(&repository, &["config", "color.diff", "always"]);
        run(&repository, &["config", "color.ui", "always"]);
        std::fs::write(repository.join(name), "x\n").unwrap();
        run(&repository, &["add", "."]);
        run(&repository, &["commit", "--quiet", "-m", "pattern"]);
        std::fs::write(repository.join(name), "y\n").unwrap();
        std::fs::write(repository.join("a.txt"), "other\n").unwrap();
        let diff = diff_of(&repository, name);
        let texts: Vec<&str> = diff.lines.iter().map(|line| line.text.as_str()).collect();
        assert_eq!(texts[1..], ["x", "y"], "{name}: {texts:?}");
    }
}

#[test]
fn an_untracked_file_listed_after_a_path_too_long_to_send_is_still_read() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("zz.bin"), [0_u8, 1]).unwrap();
    std::fs::write(dir.path().join("zz.txt"), "a\nb\n").unwrap();
    let untracked = |path: &str| ChangedFile {
        path: path.to_owned(),
        status: FileStatus::Untracked,
        ..ChangedFile::default()
    };
    // The list skips a path past the bound, so later files move up.
    let mut files = vec![untracked(&"a".repeat(MAX_PATH + 1))];
    files.extend(
        (0..crate::control::changes::MAX_FILES - 2).map(|index| ChangedFile {
            path: format!("m{index}"),
            ..ChangedFile::default()
        }),
    );
    files.extend([untracked("zz.bin"), untracked("zz.txt")]);
    let list = bounded(dir.path(), None, files);
    assert_eq!(file(&list, "zz.bin").shown, Shown::Binary);
    assert_eq!(file(&list, "zz.txt").added, Some(2));
}

#[test]
fn a_git_that_never_ends_is_ended_at_its_limit() {
    let dir = tempfile::tempdir().unwrap();
    let started = std::time::Instant::now();
    let slow = git_bounded(
        dir.path(),
        &["-c", "alias.slow=!sleep 60", "slow"],
        MAX_LISTING,
    );
    assert_eq!(slow.unwrap_err().kind(), std::io::ErrorKind::TimedOut);
    assert!(started.elapsed() < GIT_TIMEOUT * 2);
}

#[test]
fn a_sha256_repository_without_a_commit_lists_its_files() {
    let dir = tempfile::tempdir().unwrap();
    let repository = dir.path().join("fresh");
    std::fs::create_dir(&repository).unwrap();
    run(&repository, &["init", "--quiet", "--object-format=sha256"]);
    std::fs::write(repository.join("first.txt"), "x\n").unwrap();
    let list = listed(answer(
        &repository,
        &Wanted::Files {
            scope: Scope::Uncommitted,
        },
    ));
    assert_eq!(list.total, 1);
}

#[cfg(target_os = "linux")]
#[test]
fn a_name_that_is_not_utf8_is_listed_but_not_read() {
    use std::os::unix::ffi::OsStrExt;
    let dir = tempfile::tempdir().unwrap();
    let repository = repository(dir.path());
    let name = std::ffi::OsStr::from_bytes(b"bad\xff.txt");
    std::fs::write(repository.join(name), "x\n").unwrap();
    let list = listed(answer(
        &repository,
        &Wanted::Files {
            scope: Scope::Uncommitted,
        },
    ));
    let bad = list
        .files
        .iter()
        .find(|file| file.path.contains(char::REPLACEMENT_CHARACTER))
        .unwrap();
    assert_eq!(bad.shown, Shown::Unreadable);
    let diff = diff_of(&repository, &bad.path.clone());
    assert_eq!(diff.shown, Shown::Unreadable);
}

#[test]
fn a_name_with_a_replacement_character_in_it_is_still_read() {
    let dir = tempfile::tempdir().unwrap();
    let repository = repository(dir.path());
    std::fs::write(repository.join("odd\u{FFFD}.txt"), "x\n").unwrap();
    let list = listed(answer(
        &repository,
        &Wanted::Files {
            scope: Scope::Uncommitted,
        },
    ));
    let odd = file(&list, "odd\u{FFFD}.txt");
    assert_eq!((odd.shown, odd.added), (Shown::Text, Some(1)));
    assert_eq!(diff_of(&repository, "odd\u{FFFD}.txt").lines[1].text, "x");
}

#[test]
fn a_rename_whose_old_name_was_reused_shows_its_own_diff() {
    let dir = tempfile::tempdir().unwrap();
    let repository = repository(dir.path());
    run(&repository, &["mv", "old.txt", "moved.txt"]);
    std::fs::write(repository.join("old.txt"), "replacement\n").unwrap();
    run(&repository, &["add", "old.txt"]);
    let moved = diff_of(&repository, "moved.txt");
    assert!(
        moved.lines.iter().all(|line| line.text != "replacement"),
        "{moved:?}"
    );
    let reused = diff_of(&repository, "old.txt");
    assert!(
        reused
            .lines
            .iter()
            .any(|line| line.text.contains("replacement")),
        "{reused:?}"
    );
}

#[test]
fn a_section_is_found_by_its_new_renamed_or_deleted_name() {
    let text = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n\
        diff --git a/y b/y\ndeleted file mode 100644\n--- a/y\n+++ /dev/null\n@@ -1 +0,0 @@\n-y\n\
        diff --git a/o b/n\nsimilarity index 100%\nrename from o\nrename to n\n";
    assert!(section(text, "x").contains("+b"));
    assert!(section(text, "y").contains("-y"));
    assert!(section(text, "n").contains("rename to n"));
    assert_eq!(section(text, "z"), "");
}

#[test]
fn a_quoted_name_in_a_header_is_read_as_the_file_it_names() {
    assert_eq!(unquoted(r#""b/say \"hi\".txt""#), r#"b/say "hi".txt"#);
    assert_eq!(unquoted(r#""b/a\tb\\c""#), "b/a\tb\\c");
    assert_eq!(unquoted(r#""b/\303\251""#), "b/é");
    assert_eq!(unquoted("b/plain"), "b/plain");
    let text = "diff --git a/o \"b/n\\\"q\"\n--- a/o\n+++ \"b/n\\\"q\"\n@@ -1 +1 @@\n-a\n+b\n\
        diff --git a/o b/o\nnew file mode 100644\n--- /dev/null\n+++ b/o\n@@ -0,0 +1 @@\n+x\n";
    assert!(section(text, "n\"q").contains("+b"));
    assert!(section(text, "o").contains("+x"));
}
