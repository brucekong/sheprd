use super::*;
use crate::control::shortcuts::TextShortcut;

fn one(id: &str, text: &str, updated_ms: u64) -> TextShortcuts {
    TextShortcuts::new(
        [TextShortcut {
            id: id.into(),
            title: String::new(),
            text: text.into(),
        }],
        updated_ms,
    )
}

#[test]
fn with_no_daemon_the_file_takes_the_latest_offer() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");

    assert_eq!(
        exchange(&state_dir, None).unwrap(),
        TextShortcuts::default()
    );
    assert!(!state_dir.exists(), "reading leaves nothing behind");

    let offer = one("a", "Carry on.", 10);
    assert_eq!(exchange(&state_dir, Some(offer.clone())).unwrap(), offer);
    assert_eq!(
        crate::app::edited::load::<TextShortcuts>(&file(&state_dir)),
        offer
    );

    assert_eq!(
        exchange(&state_dir, Some(one("b", "older", 5))).unwrap(),
        offer
    );
    assert_eq!(exchange(&state_dir, None).unwrap(), offer);
}

#[test]
fn a_damaged_file_costs_only_the_shortcuts() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(file(dir.path()), b"not json").unwrap();
    assert_eq!(
        exchange(dir.path(), None).unwrap(),
        TextShortcuts::default()
    );
}
