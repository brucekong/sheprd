//! The composer's text shortcuts: prompts a person sends often, such as
//! asking the agent to carry on. Each computer's own daemon keeps them
//! (`edited`), so the phone, the iPad, and Sheprd on the desktop share
//! one list, and an edit on any of them reaches the rest.

use super::edited::Edited;
use serde::{Deserialize, Serialize};

/// The most shortcuts one computer keeps.
pub const MAX_SHORTCUTS: usize = 64;
/// The longest title, in characters.
pub const MAX_TITLE_CHARS: usize = 60;
/// The longest text, in characters.
pub const MAX_TEXT_CHARS: usize = 4_000;
/// The longest id, in characters: the phone's are UUIDs.
pub const MAX_ID_CHARS: usize = 64;

/// The most bytes a whole list takes as JSON, with room to spare: every
/// bound at once, in four-byte characters. Requests that carry shortcuts
/// allow this much.
pub const MAX_WIRE_BYTES: usize = 2 * 1024 * 1024;

/// One shortcut: what the menu shows, and what the chat gets.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TextShortcut {
    /// Stable across edits, so a rename keeps the shortcut's place.
    pub id: String,
    /// Blank shows the start of the text instead.
    #[serde(default)]
    pub title: String,
    pub text: String,
}

/// A computer's shortcuts, in the person's order, and when they were last
/// edited.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TextShortcuts {
    pub shortcuts: Vec<TextShortcut>,
    /// Milliseconds since the Unix epoch; 0 for shortcuts never edited.
    pub updated_ms: u64,
}

impl TextShortcuts {
    /// `shortcuts` within bounds: a one-line title, text cut to
    /// [`MAX_TEXT_CHARS`], and none without text, without a plain id, or
    /// repeating an id, at most [`MAX_SHORTCUTS`] of them.
    pub fn new(shortcuts: impl IntoIterator<Item = TextShortcut>, updated_ms: u64) -> Self {
        let mut kept: Vec<TextShortcut> = Vec::new();
        for shortcut in shortcuts {
            if kept.len() == MAX_SHORTCUTS {
                break;
            }
            let Some(shortcut) = tidied(shortcut) else {
                continue;
            };
            if !kept.iter().any(|other| other.id == shortcut.id) {
                kept.push(shortcut);
            }
        }
        Self {
            shortcuts: kept,
            updated_ms,
        }
    }
}

impl Edited for TextShortcuts {
    fn updated_ms(&self) -> u64 {
        self.updated_ms
    }

    fn bounded(self, updated_ms: u64) -> Self {
        Self::new(self.shortcuts, updated_ms)
    }
}

impl TextShortcut {
    /// The title, or the start of the text when the title is blank, as the
    /// phone shows it.
    pub fn label(&self) -> String {
        if !self.title.is_empty() {
            return self.title.clone();
        }
        let line = self
            .text
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default();
        let words = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if words.chars().count() > MAX_TITLE_CHARS {
            let cut: String = words.chars().take(MAX_TITLE_CHARS - 1).collect();
            format!("{}…", cut.trim_end())
        } else {
            words
        }
    }
}

fn tidied(shortcut: TextShortcut) -> Option<TextShortcut> {
    let id_ok = !shortcut.id.is_empty()
        && shortcut.id.chars().count() <= MAX_ID_CHARS
        && shortcut
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !id_ok {
        return None;
    }
    // Newlines and tabs belong in a prompt; other control characters do not.
    let text: String = shortcut
        .text
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .take(MAX_TEXT_CHARS)
        .collect();
    if text.trim().is_empty() {
        return None;
    }
    let title: String = shortcut
        .title
        .split_whitespace()
        .filter(|word| !word.chars().any(char::is_control))
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_TITLE_CHARS)
        .collect();
    Some(TextShortcut {
        id: shortcut.id,
        title: title.trim_end().to_owned(),
        text,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
