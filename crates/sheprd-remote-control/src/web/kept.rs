//! A setting the daemon keeps for its phones and desktops (`control::edited`),
//! such as its tags or text shortcuts: the copy that stands, saved before
//! anyone hears of it, and watched by each phone's live stream.

use crate::{app, control::edited::Edited};
use std::{
    path::PathBuf,
    sync::{Mutex, PoisonError},
};
use tokio::sync::watch;

pub(crate) struct Kept<T> {
    file: PathBuf,
    current: watch::Sender<T>,
    /// Held across a whole offer: two read as one, the older could finish
    /// last, and both write the same partial file.
    offering: Mutex<()>,
}

impl<T: Edited> Kept<T> {
    /// The copy saved in `file`. Blocking.
    pub(crate) fn load(file: PathBuf) -> Self {
        let current = watch::Sender::new(app::edited::load(&file));
        Self {
            file,
            current,
            offering: Mutex::default(),
        }
    }

    pub(crate) fn current(&self) -> T {
        self.current.borrow().clone()
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<T> {
        self.current.subscribe()
    }

    /// Offers `offer` and returns what stands. Blocking: an edit is saved
    /// before anyone hears of it, and an edit that cannot be saved is refused
    /// so its sender keeps it and tries again. The file counts too, as an
    /// offer that found no socket while this daemon started wrote there
    /// after it was read.
    pub(crate) fn offer(&self, offer: T, now_ms: u64) -> app::Result<T> {
        let _offering = self.offering.lock().unwrap_or_else(PoisonError::into_inner);
        app::edited::locked(&self.file, || {
            let saved: T = app::edited::load(&self.file);
            let stored = self.current();
            let merged = stored
                .clone()
                .merged(saved.clone(), now_ms)
                .merged(offer, now_ms);
            if merged != saved {
                app::edited::save(&self.file, &merged)?;
            }
            if merged != stored {
                self.current.send_replace(merged.clone());
            }
            Ok(merged)
        })
    }
}
