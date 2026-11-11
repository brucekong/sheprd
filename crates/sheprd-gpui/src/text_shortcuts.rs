//! Text shortcuts: prompts a person sends often, offered in a chat's
//! composer (`chat::shortcuts`) and edited in Settings.
//!
//! They are one list meant to be the same on every computer. Each
//! computer's own daemon keeps a copy for the phones that reach it
//! (`sheprd_remote_control::control::shortcuts`), and the latest edit wins,
//! whole. [`Store`] holds the newest copy this app has seen and meets this
//! computer's daemon and every connected SSH host's now and then: it always
//! offers its copy and takes a newer answer, then offers that to the rest,
//! so a phone's edit on one computer reaches all of them. A host without
//! `sheprd-remote-control` is skipped quietly.

mod error;
mod sync;

pub use error::Error;

use crate::device_tags::Daemon;
use gpui::{App, BorrowAppContext, Global};
use sheprd_remote_control::control::shortcuts::{TextShortcut, TextShortcuts};
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// How often a connected machine is asked over SSH. A phone's edit there
/// reaches this app within it; this app's own edits go at once.
const REMOTE_INTERVAL: Duration = Duration::from_secs(120);
/// This computer's daemon is a socket away.
const LOCAL_INTERVAL: Duration = Duration::from_secs(15);
/// How far past this clock an edit time may be and still count. A computer
/// whose clock runs further ahead, or a damaged copy, could otherwise keep
/// every later edit here from winning.
const FUTURE_TOLERANCE_MS: u64 = 24 * 60 * 60 * 1000;

/// What an answer does to this app's copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Settled {
    /// The computer had an edit this copy has not: it becomes the copy, and
    /// goes to the other computers.
    Take(TextShortcuts),
    /// The computer holds these shortcuts edited later, such as changed and
    /// changed back: the copy takes the later time, so its next edit is
    /// stamped after it, and needs to go nowhere else.
    Raise(TextShortcuts),
    /// The copy was edited while the exchange ran, or the computer does not
    /// hold what it was thought to: offer the copy again.
    Again,
    /// The computer holds this copy.
    Keep,
}

/// What `stood`, the computer's answer to an exchange made when the copy
/// was edited at `offered_ms`, does to `current`, the copy now, at
/// `now_ms`. `known` is what that computer answered before, whole.
///
/// Edit times come from different clocks, so they only order edits made
/// on one. An answer that changed since `known` without being newer than
/// the copy is still an edit made there, however its clock runs: it is
/// taken stamped after the copy it replaces. The copy's time is never
/// lowered, so an answer still on its way cannot undo a save made since.
/// An answer more than [`FUTURE_TOLERANCE_MS`] ahead is not taken.
pub(crate) fn settle(
    current: &TextShortcuts,
    offered_ms: u64,
    stood: TextShortcuts,
    known: Option<&TextShortcuts>,
    now_ms: u64,
) -> Settled {
    if stood.updated_ms > now_ms.saturating_add(FUTURE_TOLERANCE_MS) {
        Settled::Keep
    } else if stood.shortcuts == current.shortcuts {
        if stood.updated_ms > current.updated_ms {
            Settled::Raise(stood)
        } else {
            Settled::Keep
        }
    } else if stood.updated_ms > current.updated_ms {
        Settled::Take(stood)
    } else if current.updated_ms != offered_ms || known == Some(&stood) {
        // Edited here meanwhile; or nothing changed there, yet it holds
        // other shortcuts than it was thought to have received.
        Settled::Again
    } else {
        let updated_ms = stood.updated_ms.max(current.updated_ms.saturating_add(1));
        Settled::Take(TextShortcuts::new(stood.shortcuts, updated_ms))
    }
}

/// Whether an exchange offers the copy edited at `current_ms` to a
/// computer that last held this app's copy as edited at `held_ms`: only an
/// edit it has not received. Offering one it holds would stamp it again
/// there, by this clock, past an edit made on that computer since; matching
/// text alone is no proof, as an edit and its undo read the same.
pub(crate) fn offers(current_ms: u64, held_ms: Option<u64>) -> bool {
    held_ms != Some(current_ms)
}

/// `shortcuts` as edited here at `now_ms`: always later than `current`, so
/// an edit stands even when this clock lags the one that made the last.
pub(crate) fn edited(
    current: &TextShortcuts,
    shortcuts: Vec<TextShortcut>,
    now_ms: u64,
) -> TextShortcuts {
    TextShortcuts::new(shortcuts, now_ms.max(current.updated_ms.saturating_add(1)))
}

/// Whether `shortcut` matches what was typed to find it: its label or its
/// text holding `query`, whatever the case. An empty query matches all.
pub(crate) fn matches(shortcut: &TextShortcut, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    let query = query.to_lowercase();
    shortcut.label().to_lowercase().contains(&query)
        || shortcut.text.to_lowercase().contains(&query)
}

/// The draft once `text` is added to it, as the phone adds a shortcut: at
/// its end, a space apart from a word before it.
pub(crate) fn drafted(draft: &str, text: &str) -> String {
    if draft.is_empty() || draft.ends_with(char::is_whitespace) {
        format!("{draft}{text}")
    } else {
        format!("{draft} {text}")
    }
}

/// `list` with `shortcut` saved: in its place when one has its id, else
/// last.
pub(crate) fn with_saved(list: &[TextShortcut], shortcut: TextShortcut) -> Vec<TextShortcut> {
    let mut list = list.to_vec();
    match list.iter_mut().find(|own| own.id == shortcut.id) {
        Some(own) => *own = shortcut,
        None => list.push(shortcut),
    }
    list
}

/// `list` without the shortcut whose id is `id`.
pub(crate) fn without(list: &[TextShortcut], id: &str) -> Vec<TextShortcut> {
    list.iter().filter(|own| own.id != id).cloned().collect()
}

/// `list` with the shortcut whose id is `id` one place earlier (`up`) or
/// later; unchanged at either end.
pub(crate) fn moved(list: &[TextShortcut], id: &str, up: bool) -> Vec<TextShortcut> {
    let mut list = list.to_vec();
    let Some(index) = list.iter().position(|own| own.id == id) else {
        return list;
    };
    let other = if up {
        index.checked_sub(1)
    } else {
        Some(index + 1).filter(|other| *other < list.len())
    };
    if let Some(other) = other {
        list.swap(index, other);
    }
    list
}

/// A new shortcut's id, as the phone makes them.
pub(crate) fn new_id() -> String {
    uuid::Uuid::new_v4().to_string().to_uppercase()
}

#[derive(Debug, Default)]
struct Link {
    last: Option<Instant>,
    /// An edit asks for an exchange without waiting for the interval.
    due: bool,
    running: bool,
    /// What the computer answered last, to tell an edit made there from
    /// one this copy already holds.
    known: Option<TextShortcuts>,
    /// The edit of this app's copy, by its time here, that the computer
    /// holds: an exchange offers only a later one.
    held_ms: Option<u64>,
}

/// The newest copy of the shortcuts and each computer's link to its daemon.
#[derive(Default)]
pub(crate) struct Store {
    /// The computers windows can reach now, by endpoint ID.
    daemons: BTreeMap<String, Daemon>,
    links: HashMap<String, Link>,
    list: Arc<TextShortcuts>,
    /// Whether it meets daemons at all: the app's own store does, and one
    /// held for a test never touches this computer's.
    sharing: bool,
}

impl Global for Store {}

impl Store {
    pub(crate) fn install(cx: &mut App) {
        cx.set_global(Self {
            sharing: true,
            ..Self::default()
        });
    }

    /// Holds `list` without any computer to share it with, for tests of
    /// what draws it.
    #[cfg(test)]
    pub(crate) fn hold(cx: &mut App, list: TextShortcuts) {
        cx.set_global(Self {
            list: Arc::new(list),
            ..Self::default()
        });
    }

    /// The copy to draw from; empty before [`Self::install`].
    pub(crate) fn current(cx: &App) -> Arc<TextShortcuts> {
        cx.try_global::<Self>()
            .map(|this| this.list.clone())
            .unwrap_or_default()
    }

    /// Replaces the list with an edit made here, and offers it to every
    /// computer at once.
    pub(crate) fn set(cx: &mut App, shortcuts: Vec<TextShortcut>) {
        if !cx.has_global::<Self>() {
            return;
        }
        let changed = cx.update_global::<Self, _>(|this, _| {
            if this.list.shortcuts == shortcuts {
                return false;
            }
            this.list = Arc::new(edited(&this.list, shortcuts, now_ms()));
            for id in this.daemons.keys() {
                this.links.entry(id.clone()).or_default().due = true;
            }
            true
        });
        if changed {
            cx.refresh_windows();
            Self::tick(cx);
        }
    }

    /// Tells which computers a window can reach. Windows report every poll,
    /// so this only touches the global when the set changed.
    pub(crate) fn reach(cx: &mut App, reachable: &BTreeMap<String, Daemon>) {
        let Some(this) = cx.try_global::<Self>() else {
            return;
        };
        if this.daemons != *reachable {
            cx.global_mut::<Self>().daemons = reachable.clone();
        }
        Self::tick(cx);
    }

    /// Starts the exchanges that are due.
    fn tick(cx: &mut App) {
        let Some(this) = cx.try_global::<Self>().filter(|this| this.sharing) else {
            return;
        };
        let due: Vec<(String, Daemon)> = this
            .daemons
            .iter()
            .filter(|(id, daemon)| {
                let link = this.links.get(*id);
                let interval = match daemon {
                    Daemon::Local => LOCAL_INTERVAL,
                    Daemon::Remote(_) => REMOTE_INTERVAL,
                };
                !link.is_some_and(|link| link.running)
                    && link.is_none_or(|link| {
                        link.due || link.last.is_none_or(|last| last.elapsed() >= interval)
                    })
            })
            .map(|(id, daemon)| (id.clone(), daemon.clone()))
            .collect();
        let current = this.list.clone();
        for (id, daemon) in due {
            let link = cx.global_mut::<Self>().links.entry(id.clone()).or_default();
            link.running = true;
            link.due = false;
            link.last = Some(Instant::now());
            let offer = offers(current.updated_ms, link.held_ms).then(|| (*current).clone());
            let offered_ms = current.updated_ms;
            let task = cx
                .background_executor()
                .spawn(async move { sync::exchange(&daemon, offer, &AtomicBool::new(false)) });
            cx.spawn(async move |cx| {
                let answer = task.await;
                cx.update(|cx| Self::answered(cx, &id, offered_ms, answer));
            })
            .detach();
        }
    }

    fn answered(
        cx: &mut App,
        id: &str,
        offered_ms: u64,
        answer: Result<Option<TextShortcuts>, Error>,
    ) {
        let stood = match answer {
            Ok(stood) => stood,
            Err(error) => {
                tracing::debug!(id, %error, "Could not exchange text shortcuts");
                None
            }
        };
        let changed = cx.update_global::<Self, _>(|this, _| {
            let link = this.links.entry(id.to_owned()).or_default();
            link.running = false;
            let Some(stood) = stood else {
                return false;
            };
            let known = link.known.replace(stood.clone());
            let held = stood.shortcuts.clone();
            let changed = match settle(&this.list, offered_ms, stood, known.as_ref(), now_ms()) {
                Settled::Take(taken) => {
                    this.list = Arc::new(taken);
                    // Every other computer still holds the older copy.
                    for other in this.daemons.keys().filter(|other| *other != id) {
                        this.links.entry(other.clone()).or_default().due = true;
                    }
                    true
                }
                Settled::Raise(raised) => {
                    this.list = Arc::new(raised);
                    false
                }
                Settled::Again => {
                    let link = this.links.entry(id.to_owned()).or_default();
                    link.due = true;
                    link.held_ms = None;
                    return false;
                }
                Settled::Keep => false,
            };
            // The computer holds the copy as it stands here now.
            if held == this.list.shortcuts {
                let updated_ms = this.list.updated_ms;
                this.links.entry(id.to_owned()).or_default().held_ms = Some(updated_ms);
            }
            changed
        });
        if changed {
            cx.refresh_windows();
        }
        Self::tick(cx);
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
