//! Why Sheprd on another machine could not be read or updated.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Sheprd's answer on that machine is not one this app can read")]
    Answer(#[source] serde_json::Error),
    #[error("Sheprd is not installed on that machine")]
    Missing,
    #[error("Sheprd is not running on that machine")]
    NotRunning,
    #[error("Sheprd on that machine refused: {0}")]
    Refused(String),
    #[error("Could not run Sheprd on that machine")]
    Run(#[source] sheprd_client::Error),
    #[error("Sheprd on that machine did not finish updating in time")]
    Timeout,
    #[error("Sheprd on that machine is too old to be updated from here; update it there once")]
    TooOld,
    #[error("Sheprd on that machine could not update: {0}")]
    Update(String),
    // keep-sorted end
}
