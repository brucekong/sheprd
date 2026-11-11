//! Why a computer's text shortcuts could not be read or offered.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("The host's text shortcuts are not ones this app can read")]
    Answer(#[source] serde_json::Error),
    #[error(transparent)]
    Daemon(#[from] sheprd_remote_control::app::Error),
    #[error("Could not run sheprd-remote-control on the host")]
    Run(#[source] sheprd_client::Error),
    #[error("Text shortcuts cannot be shared with this kind of device")]
    Unsupported,
    // keep-sorted end
}
