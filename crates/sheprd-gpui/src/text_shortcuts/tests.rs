use super::*;

/// A clock well past every edit the tests make.
const NOW: u64 = 1_000_000;

fn shortcut(id: &str, title: &str, text: &str) -> TextShortcut {
    TextShortcut {
        id: id.into(),
        title: title.into(),
        text: text.into(),
    }
}

fn list(texts: &[&str], updated_ms: u64) -> TextShortcuts {
    TextShortcuts::new(
        texts
            .iter()
            .enumerate()
            .map(|(index, text)| shortcut(&format!("s{index}"), "", text)),
        updated_ms,
    )
}

/// once each has met the app: the newest is taken, then offered to the rest.
#[test]
fn an_edit_here_is_later_than_the_copy_it_replaces() {
    let current = list(&["carry on"], 500);
    let edit = edited(&current, vec![shortcut("a", "", "explain")], 100);
    assert_eq!(edit.updated_ms, 501);
    let edit = edited(&current, vec![shortcut("a", "", "explain")], 900);
    assert_eq!(edit.updated_ms, 900);
    // Bounded as every daemon bounds it.
    let blank = edited(&current, vec![shortcut("a", "", "  ")], 900);
    assert!(blank.shortcuts.is_empty());
}

#[test]
fn a_query_matches_the_label_or_the_text() {
    let named = shortcut("a", "Carry on", "Please continue where you left off");
    assert!(matches(&named, ""));
    assert!(matches(&named, "carry"));
    assert!(matches(&named, "LEFT OFF"));
    assert!(!matches(&named, "review"));
    let untitled = shortcut("b", "", "Review the diff");
    assert!(matches(&untitled, "review"));
}

#[test]
fn a_shortcut_joins_the_draft_a_space_apart() {
    assert_eq!(drafted("", "carry on"), "carry on");
    assert_eq!(drafted("look at", "carry on"), "look at carry on");
    assert_eq!(drafted("look at ", "carry on"), "look at carry on");
    assert_eq!(drafted("first\n", "carry on"), "first\ncarry on");
}

#[test]
fn new_ids_are_ones_every_daemon_keeps() {
    let id = new_id();
    let kept = TextShortcuts::new([shortcut(&id, "", "text")], 1);
    assert_eq!(kept.shortcuts.len(), 1);
    assert_ne!(new_id(), id);
}

#[test]
fn a_hosts_answer_is_bounded_or_nothing() {
    assert_eq!(sync::parse(b"").unwrap(), None);
    assert_eq!(sync::parse(b" \n").unwrap(), None);
    let answer = br#"{"shortcuts":[{"id":"a","title":"","text":"go"},{"id":"bad id","text":"x"}],"updated_ms":7}"#;
    let parsed = sync::parse(answer).unwrap().unwrap();
    assert_eq!(parsed, TextShortcuts::new([shortcut("a", "", "go")], 7));
    assert!(matches!(sync::parse(b"nope"), Err(Error::Answer(_))));
}

#[test]
fn the_host_script_runs_the_subcommand_without_installing() {
    assert!(sync::SCRIPT.contains("shortcuts --offer"));
    assert!(!sync::SCRIPT.contains("install"));
    assert!(sync::SCRIPT.contains("cat >/dev/null"));
}

#[test]
fn the_editor_saves_deletes_and_reorders_in_place() {
    let list = vec![
        shortcut("a", "", "one"),
        shortcut("b", "", "two"),
        shortcut("c", "", "three"),
    ];
    let ids = |list: &[TextShortcut]| list.iter().map(|own| own.id.clone()).collect::<Vec<_>>();
    let saved = with_saved(&list, shortcut("b", "Two", "two!"));
    assert_eq!(ids(&saved), ["a", "b", "c"]);
    assert_eq!(saved[1].title, "Two");
    assert_eq!(
        ids(&with_saved(&list, shortcut("d", "", "four"))),
        ["a", "b", "c", "d"]
    );
    assert_eq!(ids(&without(&list, "b")), ["a", "c"]);
    assert_eq!(ids(&without(&list, "z")), ["a", "b", "c"]);
    assert_eq!(ids(&moved(&list, "b", true)), ["b", "a", "c"]);
    assert_eq!(ids(&moved(&list, "b", false)), ["a", "c", "b"]);
    assert_eq!(ids(&moved(&list, "a", true)), ["a", "b", "c"]);
    assert_eq!(ids(&moved(&list, "c", false)), ["a", "b", "c"]);
    assert_eq!(ids(&moved(&list, "z", true)), ["a", "b", "c"]);
}

#[test]
fn an_edit_after_the_largest_stamp_does_not_overflow() {
    let current = list(&["carry on"], u64::MAX);
    assert_eq!(edited(&current, Vec::new(), NOW).updated_ms, u64::MAX);
}

#[test]
fn a_newer_copy_on_a_computer_is_taken() {
    let current = list(&["carry on"], 10);
    let stood = list(&["carry on", "explain"], 20);
    assert_eq!(
        settle(&current, 10, stood.clone(), None, NOW),
        Settled::Take(stood)
    );
}

#[test]
fn a_computer_holding_these_shortcuts_changes_nothing() {
    let current = list(&["carry on"], 10);
    assert_eq!(
        settle(&current, 10, current.clone(), None, NOW),
        Settled::Keep
    );
    // A daemon whose clock runs behind stamps them by its own: the stamp
    // here is never lowered to match.
    assert_eq!(
        settle(&current, 10, list(&["carry on"], 8), None, NOW),
        Settled::Keep
    );
}

#[test]
fn an_edit_made_during_an_exchange_is_offered_again() {
    let current = list(&["explain"], 30);
    assert_eq!(
        settle(&current, 10, list(&["carry on"], 10), None, NOW),
        Settled::Again
    );
    // Unless the computer's copy is newer still, which wins whole.
    let later = list(&["review"], 40);
    assert_eq!(
        settle(&current, 10, later.clone(), None, NOW),
        Settled::Take(later)
    );
}

#[test]
fn a_delayed_answer_cannot_undo_a_save_made_since() {
    // X goes to two computers at 5000; the first takes it at 4000, by a
    // slower clock. The stamp here stays 5000, so a save comes after it.
    let x = list(&["carry on"], 5_000);
    assert_eq!(
        settle(&x, 5_000, list(&["carry on"], 4_000), None, NOW),
        Settled::Keep
    );
    let y = edited(&x, vec![shortcut("y", "", "review")], 4_500);
    assert_eq!(y.updated_ms, 5_001);
    // The second computer's answer, X at 5000, arrives late: Y stands.
    let known = list(&["old"], 1);
    assert_eq!(settle(&y, 5_000, x, Some(&known), NOW), Settled::Again);
}

#[test]
fn an_edit_on_a_computer_with_a_slower_clock_is_taken() {
    // This clock runs ahead; the host took the list at 4000.
    let current = list(&["carry on"], 5_000);
    let held = list(&["carry on"], 4_000);
    assert!(
        !offers(5_000, Some(5_000)),
        "an edit the host holds is not offered again"
    );
    // A phone edits there, stamped by the host's slower clock.
    let phone = list(&["review the diff"], 4_500);
    assert_eq!(
        settle(&current, 5_000, phone, Some(&held), NOW),
        Settled::Take(list(&["review the diff"], 5_001)),
        "taken after the copy it replaces, so it goes on to the others"
    );
    // The same answer heard twice is not an edit made there: the host
    // does not hold the copy after all, so it is offered again.
    let again = list(&["old"], 4_000);
    assert_eq!(
        settle(&current, 5_000, again.clone(), Some(&again), NOW),
        Settled::Again
    );
}

#[test]
fn an_answer_edited_far_in_the_future_is_not_taken() {
    let current = list(&["carry on"], 10);
    let ahead = list(&["from a fast clock"], NOW + FUTURE_TOLERANCE_MS + 1);
    assert_eq!(settle(&current, 10, ahead, None, NOW), Settled::Keep);
    let slightly = list(&["a minute ahead"], NOW + 60_000);
    assert_eq!(
        settle(&current, 10, slightly.clone(), None, NOW),
        Settled::Take(slightly)
    );
    assert_eq!(
        settle(&current, 10, list(&["max"], u64::MAX), None, NOW),
        Settled::Keep
    );
}

/// Three computers holding copies of different ages converge on the newest
/// once each has met the app: the newest is taken, then offered to the rest.
#[test]
fn computers_converge_on_the_latest_edit() {
    use sheprd_remote_control::control::edited::Edited;
    let mut computers = [
        list(&["old"], 10),
        list(&["newest"], 30),
        list(&["middle"], 20),
    ];
    let mut known: [Option<TextShortcuts>; 3] = Default::default();
    let mut held: [Option<u64>; 3] = Default::default();
    let mut copy = TextShortcuts::default();
    // Two rounds: the first finds the newest, the second hands it on.
    for _ in 0..2 {
        for ((computer, known), held) in computers.iter_mut().zip(&mut known).zip(&mut held) {
            let offered_ms = copy.updated_ms;
            if offers(copy.updated_ms, *held) {
                *computer = computer.clone().merged(copy.clone(), 100);
            }
            let before = known.replace(computer.clone());
            match settle(&copy, offered_ms, computer.clone(), before.as_ref(), NOW) {
                Settled::Take(taken) | Settled::Raise(taken) => copy = taken,
                Settled::Again => *held = None,
                Settled::Keep => {}
            }
            if computer.shortcuts == copy.shortcuts {
                *held = Some(copy.updated_ms);
            }
        }
    }
    assert_eq!(copy.shortcuts, list(&["newest"], 30).shortcuts);
    assert!(
        computers
            .iter()
            .all(|computer| computer.shortcuts == copy.shortcuts)
    );
}

#[test]
fn a_later_time_for_the_same_shortcuts_is_kept() {
    // A phone edited the list and changed it back, at 300 there.
    let current = list(&["carry on"], 100);
    let back = list(&["carry on"], 300);
    assert_eq!(
        settle(&current, 100, back.clone(), None, NOW),
        Settled::Raise(back.clone())
    );
    // So a save here, by a clock reading 250, still comes after it.
    let save = edited(&back, vec![shortcut("s", "", "review")], 250);
    assert_eq!(save.updated_ms, 301);
}

#[test]
fn an_edit_and_its_undo_made_here_still_reach_a_computer_that_was_away() {
    use sheprd_remote_control::control::edited::Edited;
    // The host held A, edited here at 100, then went away.
    let held = Some(100);
    // Here: B at 200, then back to A at 300. There: C at 250.
    let current = list(&["carry on"], 300);
    let host = list(&["from the host"], 250);
    assert!(
        offers(current.updated_ms, held),
        "matching text is no proof it was received"
    );
    // The host takes the later edit, stamped by its own clock.
    let stood = host.clone().merged(current.clone(), 260);
    assert_eq!(stood.shortcuts, current.shortcuts);
    assert_eq!(
        settle(&current, 300, stood, Some(&host), NOW),
        Settled::Keep
    );
}

#[test]
fn the_read_script_offers_nothing() {
    assert!(sync::READ_SCRIPT.contains("shortcuts 2>"));
    assert!(!sync::READ_SCRIPT.contains("--offer"));
    assert!(!sync::READ_SCRIPT.contains("install"));
}
