//! This computer's tags on disk (`control::tags`), and `sheprd-remote-control
//! tags`, which Sheprd runs over SSH on another computer. A host may have the
//! program but no daemon running, so the file stands in for one.

pub use super::edited::{locked, save};
use crate::control::{self, Request, Response, tags::ComputerTags};
use std::path::{Path, PathBuf};

const FILE: &str = "tags.json";

/// Where the tags of the daemon whose state is `state_dir` are kept.
pub fn file(state_dir: &Path) -> PathBuf {
    state_dir.join(FILE)
}

/// The tags saved in `file`, bounded; none when it is missing or unreadable,
/// as a damaged file costs only labels. Blocking.
pub fn load(file: &Path) -> ComputerTags {
    super::edited::load(file)
}

/// Offers `offer` to the tags in `state_dir` and returns those that stand:
/// through the daemon when one runs, so its phones hear of it at once, else
/// in the file it reads when it starts. Blocking.
pub fn exchange(state_dir: &Path, offer: Option<ComputerTags>) -> super::Result<ComputerTags> {
    let address = control::address(state_dir);
    let request = Request::Tags {
        offer: offer.clone(),
    };
    match control::request(&address, &request) {
        Ok(Response::Tags(tags)) => Ok(tags),
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
