//! Updating Sheprd on another machine: `error-remote-update.ftl`.

use super::Localize;
use crate::{i18n::fl, remote_update::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Answer(_) => fl!("error-remote-update-answer"),
            Self::Missing => fl!("error-remote-update-missing"),
            Self::NotRunning => fl!("error-remote-update-not-running"),
            Self::Refused(reason) => fl!("error-remote-update-refused", reason = reason.as_str()),
            Self::Run(source) => fl!("error-remote-update-run", reason = source.localize()),
            Self::Timeout => fl!("error-remote-update-timeout"),
            Self::TooOld => fl!("error-remote-update-too-old"),
            Self::Update(reason) => fl!("error-remote-update-update", reason = reason.as_str()),
            // keep-sorted end
        }
    }
}
