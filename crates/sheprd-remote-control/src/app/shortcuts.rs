//! This computer's text shortcuts on disk (`control::shortcuts`), and
//! `sheprd-remote-control shortcuts`, which Sheprd runs over SSH on another
//! computer. A host may have the program but no daemon running, so the file
//! stands in for one.

use crate::control::{self, Request, Response, shortcuts::TextShortcuts};
use std::path::{Path, PathBuf};

const FILE: &str = "shortcuts.json";

/// Where the shortcuts of the daemon whose state is `state_dir` are kept.
pub fn file(state_dir: &Path) -> PathBuf {
    state_dir.join(FILE)
}

/// Offers `offer` to the shortcuts in `state_dir` and returns those that
/// stand: through the daemon when one runs, so its phones hear of it at
/// once, else in the file it reads when it starts. Blocking.
pub fn exchange(state_dir: &Path, offer: Option<TextShortcuts>) -> super::Result<TextShortcuts> {
    let address = control::address(state_dir);
    let request = Request::Shortcuts {
        offer: offer.clone(),
    };
    match control::request(&address, &request) {
        Ok(Response::Shortcuts(shortcuts)) => Ok(shortcuts),
        Ok(_) => Err(super::Error::Unexpected),
        Err(control::Error::Connect(_)) => {
            super::edited::offer_file(state_dir, &file(state_dir), offer)
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
