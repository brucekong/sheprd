//! Text shortcuts in a chat's composer (`crate::text_shortcuts`): listed
//! first when the draft is a slash, and in the popover of their own button.
//! As on the phone, a shortcut sends at once when nothing else is drafted,
//! and otherwise joins the draft.

use super::chats::{ChatInput, Menu};
use crate::{
    SheprdWindow,
    text_shortcuts::{self, Store},
};
use gpui::{prelude::*, *};
use sheprd_chat_protocol::{SlashCommand, Transcript, assist};
use sheprd_remote_control::control::shortcuts::TextShortcut;

/// Shortcuts the slash list shows at once, above the agent's commands.
pub(crate) const SLASH_SHORTCUTS: usize = 8;

/// One row of the slash list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Completion<'a> {
    Shortcut(&'a TextShortcut),
    Command(&'a SlashCommand),
}

/// What taking a row of the slash list uses, owned so the list's borrow
/// can end first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Chosen {
    /// A shortcut's text.
    Shortcut(String),
    /// A command's name.
    Command(String),
}

impl Completion<'_> {
    pub(crate) fn chosen(&self) -> Chosen {
        match self {
            Self::Shortcut(shortcut) => Chosen::Shortcut(shortcut.text.clone()),
            Self::Command(command) => Chosen::Command(command.name.clone()),
        }
    }
}

/// What the slash list offers for `text`: the person's shortcuts whose
/// label or text holds what follows the slash, then the agent's commands
/// that name completes to.
pub(crate) fn completions<'a>(
    shortcuts: &'a [TextShortcut],
    transcript: &'a Transcript,
    text: &str,
) -> Vec<Completion<'a>> {
    let typed = text
        .strip_prefix('/')
        .filter(|typed| !typed.contains(char::is_whitespace));
    let Some(typed) = typed else {
        return Vec::new();
    };
    shortcuts
        .iter()
        .filter(|shortcut| text_shortcuts::matches(shortcut, typed))
        .take(SLASH_SHORTCUTS)
        .map(Completion::Shortcut)
        .chain(
            assist::completions(transcript, text)
                .into_iter()
                .map(Completion::Command),
        )
        .collect()
}

/// The shortcuts the popover lists for `query`, in the person's order.
pub(crate) fn found<'a>(
    shortcuts: &'a [TextShortcut],
    query: &'a str,
) -> impl Iterator<Item = &'a TextShortcut> + 'a {
    shortcuts
        .iter()
        .filter(move |shortcut| text_shortcuts::matches(shortcut, query))
}

/// Where a shortcut was chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Picked {
    /// From the slash list: it takes the place of the `/query` typed.
    Slash,
    /// From its button's popover: the draft stays.
    Button,
}

/// What choosing a shortcut does to `draft`, the composer's text once a
/// slash query is taken out: the prompt to send at once when nothing else
/// was drafted (`alone`), else the draft it becomes.
pub(crate) fn applied(draft: &str, alone: bool, text: &str) -> (String, bool) {
    if alone && draft.trim().is_empty() {
        (text.to_owned(), true)
    } else {
        (text_shortcuts::drafted(draft, text), false)
    }
}

impl SheprdWindow {
    /// Uses `text`, a shortcut's, in the chat over `pane_id`.
    pub(super) fn use_chat_shortcut(
        &mut self,
        pane_id: &str,
        text: &str,
        picked: Picked,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        if pane.input() != ChatInput::Composer {
            return;
        }
        pane.completion = 0;
        let Some(input) = pane.composer.clone() else {
            return;
        };
        let draft = match picked {
            Picked::Slash => "",
            Picked::Button => input.read(cx).text(),
        };
        let nothing_else = pane.attachments.is_empty() && pane.uploading == 0;
        let (next, send) = applied(draft, nothing_else, text);
        input.update(cx, |input, cx| input.set_text_at(&next, next.len(), cx));
        if send {
            self.submit_chat_prompt(pane_id, &input, window, cx);
        }
        let focus = input.read(cx).focus.clone();
        window.focus(&focus, cx);
        self.redraw_terminal(cx);
        cx.notify();
    }

    /// Opens or closes the shortcuts' popover over the composer.
    pub(super) fn toggle_chat_shortcuts(
        &mut self,
        pane_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_chat_menu(pane_id, Menu::Shortcuts, window, cx);
    }

    /// Enter in the popover's search: the first shortcut it finds.
    pub(super) fn pick_first_chat_shortcut(
        &mut self,
        pane_id: &str,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let list = Store::current(cx);
        let text = found(&list.shortcuts, query)
            .next()
            .map(|shortcut| shortcut.text.clone());
        self.toggle_chat_shortcuts(pane_id, window, cx);
        if let Some(text) = text {
            self.use_chat_shortcut(pane_id, &text, Picked::Button, window, cx);
        }
    }

    /// Closes the popover and opens the shortcuts' page in Settings.
    pub(super) fn edit_chat_shortcuts(
        &mut self,
        pane_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .chats
            .pane(pane_id)
            .is_some_and(|pane| pane.menu == Some(Menu::Shortcuts))
        {
            self.toggle_chat_shortcuts(pane_id, window, cx);
        }
        crate::settings_window::open_text_shortcuts(cx.entity().downgrade(), cx);
    }
}

#[cfg(test)]
mod tests;
