//! The composer's text shortcuts button and its popover: a search field,
//! the shortcuts as two lines each (label, then the start of the text), and
//! a way to the page that edits them. It grows in and fades out through
//! `crate::motion`, reversing from where it is when toggled midway.

use super::super::{
    chats::{ChatPane, Menu},
    shortcuts::{Picked, found},
};
use crate::i18n::fl;
use crate::{SheprdWindow, config::corners, notifications::safe_text, text_shortcuts::Store};
use gpui::{prelude::*, *};

/// How far the popover rises into place as it opens.
const RISE: f32 = 6.;
/// The popover's width; narrower panes clip it as they clip the menus.
const WIDTH: f32 = 360.;

/// A shortcut's text on one line, for a row too short to wrap it.
pub(in crate::chat) fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl SheprdWindow {
    /// The button beside the composer's + that opens the popover.
    pub(super) fn render_chat_shortcuts_button(
        &self,
        chat: &ChatPane,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let open = chat.menu == Some(Menu::Shortcuts);
        let pane_id = chat.target.pane_id.clone();
        let hint = SharedString::from(fl!("chat-shortcuts-button"));
        let (foreground, surface) = (theme.foreground, theme.surface);
        div()
            .id("chat-shortcuts")
            .debug_selector(|| "chat-shortcuts".into())
            .flex_none()
            .size(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .cursor_pointer()
            .when(open, |button| button.bg(rgb(theme.active)))
            .hover(|button| button.bg(rgb(theme.active)))
            .child(
                svg()
                    .path("icons/note.svg")
                    .size(px(13.))
                    .text_color(rgb(if open {
                        theme.foreground
                    } else {
                        theme.subtext()
                    })),
            )
            .tooltip(move |_, cx| {
                cx.new(|_| crate::menu::SettingsHint {
                    text: hint.clone(),
                    foreground,
                    surface,
                })
                .into()
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                cx.stop_propagation();
                this.toggle_chat_shortcuts(&pane_id, window, cx);
            }))
    }

    /// The popover over the composer while it is open or still closing.
    pub(super) fn render_chat_shortcuts(
        &self,
        chat: &ChatPane,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let now = crate::motion::now();
        // Unit tests see it open or closed at once, as `motion::pop_in` does,
        // so geometry checks need not wait out the motion.
        let shown = if cfg!(test) {
            f32::from(u8::from(chat.shortcuts.is_open()))
        } else {
            chat.shortcuts.shown(now)
        };
        if shown <= 0. {
            return None;
        }
        let theme = &self.theme;
        let open = chat.menu == Some(Menu::Shortcuts);
        let pane_id = chat.target.pane_id.clone();
        // While another menu is open its own field is drawn there, so a
        // popover still fading out leaves its place empty.
        let search = chat
            .menu_search
            .as_ref()
            .filter(|_| chat.menu.is_none() || open)
            .map(|(search, _)| search.clone());
        let query = search
            .as_ref()
            .map(|search| search.read(cx).text().to_owned())
            .unwrap_or_default();
        let list = Store::current(cx);
        let small = self.chat_small_text();
        let mut rows: Vec<AnyElement> = Vec::new();
        rows.push(
            div()
                .debug_selector(|| "chat-shortcuts-search".into())
                .mx(px(4.))
                .mb(px(4.))
                .px(px(6.))
                .h(px(30.))
                .flex()
                .items_center()
                .rounded(px(corners::SMALL))
                .border_1()
                .border_color(rgb(theme.active))
                .bg(rgb(theme.background))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .children(search)
                .into_any_element(),
        );
        let mut any = false;
        for (index, shortcut) in found(&list.shortcuts, &query).enumerate() {
            any = true;
            let pane = pane_id.clone();
            let text = shortcut.text.clone();
            rows.push(
                div()
                    .id(("chat-shortcut", index))
                    .debug_selector(move || format!("chat-shortcut-{index}"))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(corners::SMALL))
                    .cursor_pointer()
                    .hover(|row| row.bg(rgb(theme.active)))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .child(safe_text(&shortcut.label(), 80)),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(small)
                            .text_color(rgb(theme.subtext()))
                            .child(safe_text(&one_line(&shortcut.text), 200)),
                    )
                    .when(open, |row| {
                        row.on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.toggle_chat_shortcuts(&pane, window, cx);
                            this.use_chat_shortcut(&pane, &text, Picked::Button, window, cx);
                        }))
                    })
                    .into_any_element(),
            );
        }
        if !any {
            let note = if list.shortcuts.is_empty() {
                fl!("chat-shortcuts-empty")
            } else {
                fl!("chat-shortcuts-no-match")
            };
            rows.push(
                div()
                    .debug_selector(|| "chat-shortcuts-none".into())
                    .px(px(8.))
                    .py(px(4.))
                    .text_color(rgb(theme.muted))
                    .child(note)
                    .into_any_element(),
            );
        }
        let edit = {
            let pane = pane_id.clone();
            div()
                .id("chat-shortcuts-edit")
                .debug_selector(|| "chat-shortcuts-edit".into())
                .flex_none()
                .flex()
                .items_center()
                .gap(px(6.))
                .mt(px(4.))
                .px(px(8.))
                .py(px(5.))
                .border_t_1()
                .border_color(rgb(theme.active))
                .text_color(rgb(theme.subtext()))
                .cursor_pointer()
                .hover(|row| row.bg(rgb(theme.active)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(svg().path("icons/pencil.svg").size(px(11.)))
                .child(fl!("chat-shortcuts-edit"))
                .when(open, |row| {
                    row.on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.edit_chat_shortcuts(&pane, window, cx);
                    }))
                })
        };
        let moving = !cfg!(test) && chat.shortcuts.moving(now);
        Some(
            div()
                .absolute()
                .bottom(relative(1.))
                .left(px(12.))
                .w(px(WIDTH))
                .child(
                    div()
                        .id("chat-shortcuts-popover")
                        .debug_selector(|| "chat-shortcuts-popover".into())
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .relative()
                        .top(px((1. - shown) * RISE))
                        .opacity(shown)
                        .flex()
                        .flex_col()
                        .p(px(4.))
                        .rounded(px(corners::CONTROL))
                        .border_1()
                        .border_color(rgb(theme.active))
                        .bg(rgb(theme.surface))
                        .shadow_md()
                        .child(
                            div()
                                .id("chat-shortcuts-rows")
                                .flex()
                                .flex_col()
                                .max_h(px(300.))
                                .overflow_y_scroll()
                                .children(rows),
                        )
                        .child(edit),
                )
                .when(moving, |frame| {
                    // Draws the next frame while it is still moving.
                    frame.child(
                        canvas(
                            |_, window, _| window.request_animation_frame(),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_0(),
                    )
                })
                .into_any_element(),
        )
    }
}
