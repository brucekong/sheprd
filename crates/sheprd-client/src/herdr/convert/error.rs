use sheprd_chat_protocol::Agent;

/// Why an agent did not move into a chat. Each one before the agent is told
/// to stop leaves it running; [`Self::Stopped`] and [`Self::StillRunning`]
/// come after, and leave its session to continue by hand.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("the pane already runs a chat")]
    AlreadyChat,
    #[error("{} is busy; wait until it is idle", .0.display_name())]
    Busy(Agent),
    #[error("the session's folder {0} is gone, as when its worktree was removed")]
    FolderGone(String),
    #[error("the session's folder or flags cannot be typed safely")]
    Line,
    #[error("the pane changed before its agent could stop")]
    Moved,
    #[error("the pane runs no agent")]
    NoAgent,
    #[error(
        "herdr-core reports no session for {}; only Claude Code's own session record can be read",
        .0.display_name()
    )]
    NoCoreSession(Agent),
    #[error(
        "Herdr reports no session for {} in this pane; run `herdr integration install {}`, then restart the agent",
        .0.display_name(),
        .0.name()
    )]
    NoSession(Agent),
    #[error(
        "{} is the terminal's own program, with no shell to start its chat in",
        .0.display_name()
    )]
    NoShell(Agent),
    #[error("{} is not the program running in this pane", .0.display_name())]
    NotRunning(Agent),
    #[error("{} did not stop in time; it is still in its terminal", .0.display_name())]
    StillRunning(Agent),
    #[error("the move was stopped before the agent moved")]
    Stopped,
    #[error("`{0}` is not an agent Sheprd can chat with")]
    Unknown(String),
    #[error("{} cannot move into a chat yet", .0.display_name())]
    Unsupported(Agent),
    // keep-sorted end
}
