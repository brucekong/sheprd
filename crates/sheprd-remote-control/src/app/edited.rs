//! Settings kept whole with the time of their last edit (`control::edited`)
//! on disk, beside the daemon's other state. A host may have the program but
//! no daemon running, so the file stands in for one.

use crate::control::edited::Edited;
use serde::Serialize;
use std::{
    fs, io,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

/// The copy saved in `file`, bounded; the default when it is missing or
/// unreadable, as a damaged file costs only a setting. Blocking.
pub fn load<T: Edited>(file: &Path) -> T {
    fs::read(file)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<T>(&bytes).ok())
        .map(|saved| {
            let updated_ms = saved.updated_ms();
            saved.bounded(updated_ms)
        })
        .unwrap_or_default()
}

/// Replaces `file` whole, so a crash leaves the old copy or the new one.
/// Blocking.
pub fn save<T: Serialize>(file: &Path, value: &T) -> super::Result<()> {
    let failed = |operation, source| super::Error::Io {
        operation,
        path: file.to_owned(),
        source,
    };
    let bytes =
        serde_json::to_vec(value).map_err(|error| failed("encode", io::Error::from(error)))?;
    let partial = file.with_extension("json.partial");
    super::enroll::write_private(&partial, &bytes)?;
    fs::rename(&partial, file).map_err(|source| failed("replace", source))
}

/// Runs `work` as the only process changing `file`: with no daemon running,
/// each offer is its own process, and one that read an older copy could
/// otherwise finish last. The system frees the lock if its holder dies.
/// Blocking.
pub fn locked<T>(file: &Path, work: impl FnOnce() -> super::Result<T>) -> super::Result<T> {
    let path = file.with_extension("lock");
    let failed = |operation, source| super::Error::Io {
        operation,
        path: path.clone(),
        source,
    };
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|source| failed("open", source))?;
    lock.lock().map_err(|source| failed("lock", source))?;
    work()
}

/// With no daemon to ask: offers `offer`, if any, to `file` in `state_dir`
/// and returns the copy that stands, which the daemon reads when it starts.
/// Blocking.
pub(super) fn offer_file<T: Edited>(
    state_dir: &Path,
    file: &Path,
    offer: Option<T>,
) -> super::Result<T> {
    let Some(offer) = offer else {
        return Ok(load(file));
    };
    fs::create_dir_all(state_dir).map_err(|source| super::Error::Io {
        operation: "create",
        path: state_dir.to_owned(),
        source,
    })?;
    locked(file, || {
        let saved: T = load(file);
        let merged = saved.clone().merged(offer, now_ms());
        if merged != saved {
            save(file, &merged)?;
        }
        Ok(merged)
    })
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}
