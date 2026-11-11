//! The native views of `herdr-acp` chats over their panes.
use super::app::Switch;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChatOp {
    Status,
    /// Opens a tab running a `herdr-acp` chat with a built-in agent, by name.
    New {
        agent: String,
        /// The pane whose workspace and folder it starts in, else the
        /// caller's own, else the focused one.
        #[serde(default)]
        pane: Option<String>,
    },
    /// Moves a pane's terminal agent into a chat in the same pane and
    /// conversation: stops its CLI, then continues its session in the chat.
    Convert {
        /// The pane, else the caller's own, else the focused one.
        #[serde(default)]
        pane: Option<String>,
    },
    /// The person's text shortcuts, which every chat's composer offers.
    Shortcuts,
    /// Shows the chat view over a pane (`on`), or its terminal (`off`).
    View {
        switch: Switch,
        /// The pane, else the caller's own, else the focused one.
        #[serde(default)]
        pane: Option<String>,
    },
}

/// Who owns input in the conversation pane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputMode {
    #[default]
    Chat,
    /// The running CLI's original editor is visible in the same pane.
    Terminal,
    ReadOnly,
}

/// One chat pane of the window's selected host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatInfo {
    pub pane_id: String,
    pub agent: String,
    /// "starting", "idle", "working", "blocked", "failed", or "exited".
    pub status: String,
    /// Connected to the program now, rather than reconnecting.
    pub attached: bool,
    /// The chat view stands over the pane; otherwise its terminal shows.
    pub shown: bool,
    /// Permission requests waiting for an answer.
    pub permissions: usize,
    /// Prompts waiting for the running turn.
    pub queued: usize,
    /// The model the agent says it uses, by name.
    #[serde(default)]
    pub model: Option<String>,
    /// How full the context window is, in percent, once the agent says.
    #[serde(default)]
    pub context: Option<u8>,
    #[serde(default)]
    pub input_mode: InputMode,
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reply {
    Chats {
        chats: Vec<ChatInfo>,
    },
    View {
        pane_id: String,
        shown: bool,
    },
    /// The chat's tab was asked for; it attaches once its program reports.
    Started {
        agent: String,
    },
    /// The agent is moving into a chat, which attaches once it has stopped
    /// and the program reports.
    Converting {
        agent: String,
        pane_id: String,
    },
    /// The text shortcuts, in the person's order.
    Shortcuts {
        shortcuts: Vec<ShortcutInfo>,
        /// When the list was last edited, in milliseconds since the Unix
        /// epoch; 0 for a list never edited.
        updated_ms: u64,
        /// Every shortcut is listed: a reply stops short of
        /// [`MAX_SHORTCUT_BYTES`] encoded rather than exceed the message bound.
        complete: bool,
    },
}

/// The most bytes a reply's shortcuts take encoded, escapes and all, well
/// within [`super::MAX_MESSAGE`].
pub const MAX_SHORTCUT_BYTES: usize = 160 * 1024;

/// One text shortcut.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutInfo {
    pub id: String,
    /// What a menu shows: its title, or the start of its text.
    pub label: String,
    /// Blank when the label is the start of the text.
    #[serde(default)]
    pub title: String,
    pub text: String,
}

/// `shortcuts` as a reply lists them: in order, until the next would take
/// them past [`MAX_SHORTCUT_BYTES`] as JSON, where a quote, backslash,
/// newline, or tab takes two bytes. Whether all of them fit.
pub fn bounded_shortcuts(
    shortcuts: impl IntoIterator<Item = ShortcutInfo>,
) -> (Vec<ShortcutInfo>, bool) {
    let mut listed = Vec::new();
    let mut bytes = 0;
    for shortcut in shortcuts {
        // One more for the comma between entries; one that cannot be
        // encoded is counted as too large to list.
        bytes += serde_json::to_string(&shortcut).map_or(usize::MAX, |encoded| encoded.len() + 1);
        if bytes > MAX_SHORTCUT_BYTES {
            return (listed, false);
        }
        listed.push(shortcut);
    }
    (listed, true)
}

#[cfg(test)]
mod tests;
