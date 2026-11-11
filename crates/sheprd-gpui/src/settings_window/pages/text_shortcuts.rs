//! Text Shortcuts: add, edit, reorder, and delete the prompts a chat's
//! composer offers (`crate::text_shortcuts`). An edit saves at once to the
//! shared list, which goes to this computer's daemon and every connected
//! computer that keeps one, and from there to the phones.
// i18n: migrated
use super::{SettingsWindow, ui};
use crate::{
    config::corners,
    i18n::fl,
    multiline_input::MultilineInput,
    notifications::safe_text,
    search_input::SearchInput,
    text_shortcuts::{self, Store},
};
use gpui::{prelude::*, *};
use sheprd_remote_control::control::shortcuts::{MAX_SHORTCUTS, MAX_TEXT_CHARS, TextShortcut};

/// The page's state: the shortcut being written, if any.
#[derive(Default)]
pub(in crate::settings_window) struct TextShortcutsPage {
    editing: Option<Editing>,
    /// Redraws the page when the list changes, here or on another device.
    watch: Option<Subscription>,
}

impl TextShortcutsPage {
    /// The text field of the shortcut being written, for tests to type in.
    #[cfg(test)]
    pub(in crate::settings_window) fn editing(&self) -> Option<&Entity<MultilineInput>> {
        self.editing.as_ref().map(|editing| &editing.text)
    }
}

/// One shortcut's title and text, while they are written.
struct Editing {
    id: String,
    title: Entity<SearchInput>,
    text: Entity<MultilineInput>,
    _typing: Subscription,
}

/// Opens Settings at Text Shortcuts, as the chat's popover does.
pub(crate) fn open(source: WeakEntity<crate::SheprdWindow>, cx: &mut App) {
    crate::settings_window::open(source, cx);
    // Queued after `open`'s own deferred work, so the window exists by now.
    cx.defer(|cx| {
        let Some(handle) = cx
            .default_global::<crate::settings_window::SettingsWindowHandle>()
            .window
        else {
            return;
        };
        let _ = handle.update(cx, |view, window, cx| {
            view.select_section(crate::settings_window::Section::TextShortcuts, window, cx);
        });
    });
}

impl SettingsWindow {
    pub(in crate::settings_window) fn open_text_shortcuts_page(&mut self, cx: &mut Context<Self>) {
        if self.text_shortcuts.watch.is_none() {
            self.text_shortcuts.watch = Some(cx.observe_global::<Store>(|_, cx| cx.notify()));
        }
    }

    /// Starts writing `shortcut`, or a new one.
    fn edit_text_shortcut(
        &mut self,
        shortcut: Option<TextShortcut>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let shortcut = shortcut.unwrap_or_else(|| TextShortcut {
            id: text_shortcuts::new_id(),
            title: String::new(),
            text: String::new(),
        });
        let (font, theme) = (self.config.ui.clone(), self.theme.clone());
        let title = cx.new(|cx| {
            let mut input = SearchInput::new(cx);
            input.set_appearance(font.clone(), theme.clone(), cx);
            input.set_placeholder_with(|| fl!("settings-text-shortcuts-title-placeholder"), cx);
            input.set_text_at(&shortcut.title, shortcut.title.len(), cx);
            input
        });
        let text = cx.new(|cx| {
            let mut input = MultilineInput::new(cx);
            input.set_appearance(font, theme, cx);
            input.set_placeholder_with(|| fl!("settings-text-shortcuts-text-placeholder"), cx);
            input.set_text_at(&shortcut.text, shortcut.text.len(), cx);
            input
        });
        let focus = if shortcut.title.is_empty() && shortcut.text.is_empty() {
            title.read(cx).focus.clone()
        } else {
            text.read(cx).focus.clone()
        };
        // Typing redraws the page, which enables Save once there is text.
        let typing = cx.observe(&text, |_, _, cx| cx.notify());
        self.text_shortcuts.editing = Some(Editing {
            id: shortcut.id,
            title,
            text,
            _typing: typing,
        });
        window.focus(&focus, cx);
        cx.notify();
    }

    fn save_text_shortcut(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editing) = &self.text_shortcuts.editing else {
            return;
        };
        let shortcut = TextShortcut {
            id: editing.id.clone(),
            title: editing.title.read(cx).text().to_owned(),
            text: editing.text.read(cx).text().to_owned(),
        };
        // Text past the limit would be cut by every computer: refused here,
        // so a long prompt is never shortened unseen.
        if shortcut.text.trim().is_empty() || shortcut.text.chars().count() > MAX_TEXT_CHARS {
            return;
        }
        // Another device may have filled the list while this was written:
        // the draft stays, rather than being saved past the limit and lost.
        let list = Store::current(cx);
        if no_room_for(&list.shortcuts, &shortcut.id) {
            cx.notify();
            return;
        }
        self.text_shortcuts.editing = None;
        Store::set(cx, text_shortcuts::with_saved(&list.shortcuts, shortcut));
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn cancel_text_shortcut(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.text_shortcuts.editing = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(in crate::settings_window) fn render_text_shortcuts(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let list = Store::current(cx);
        let count = list.shortcuts.len();
        let editing = self.text_shortcuts.editing.as_ref();
        let full = count >= MAX_SHORTCUTS;
        let add = ui::row(
            theme,
            "text-shortcuts-add",
            fl!("settings-text-shortcuts-add"),
        )
        .description(if full {
            fl!("settings-text-shortcuts-full", count = MAX_SHORTCUTS)
        } else {
            fl!("settings-text-shortcuts-add-description")
        })
        .control(
            ui::button(
                theme,
                "text-shortcuts-add-button",
                fl!("settings-text-shortcuts-add-button"),
                !full && editing.is_none(),
            )
            .when(!full && editing.is_none(), |button| {
                button.on_click(cx.listener(|this, _, window, cx| {
                    this.edit_text_shortcut(None, window, cx);
                }))
            }),
        );
        let new = editing.filter(|editing| list.shortcuts.iter().all(|own| own.id != editing.id));
        let form = new.map(|editing| self.text_shortcut_form(editing, cx));
        let rows: Vec<AnyElement> = list
            .shortcuts
            .iter()
            .enumerate()
            .map(
                |(index, shortcut)| match editing.filter(|editing| editing.id == shortcut.id) {
                    Some(editing) => self.text_shortcut_form(editing, cx).into_any_element(),
                    None => self
                        .text_shortcut_row(index, count, shortcut, editing.is_none(), cx)
                        .into_any_element(),
                },
            )
            .collect();
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .child(add)
            .children(form)
            .child(ui::section(
                theme,
                &fl!("settings-text-shortcuts-list"),
                None,
            ))
            .when(count == 0 && new.is_none(), |page| {
                page.child(
                    div()
                        .debug_selector(|| "text-shortcuts-empty".into())
                        .py(px(10.))
                        .child(self.control_note(fl!("settings-text-shortcuts-empty"))),
                )
            })
            .children(rows)
    }

    /// One shortcut: its label and the start of its text, and the buttons
    /// that move, edit, and delete it.
    fn text_shortcut_row(
        &self,
        index: usize,
        count: usize,
        shortcut: &TextShortcut,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let id = shortcut.id.clone();
        let button = |name: &str, label: String, on: bool| {
            ui::button(theme, format!("text-shortcut-{name}-{index}"), label, on)
        };
        let up = {
            let id = id.clone();
            let on = enabled && index > 0;
            button("up", fl!("settings-text-shortcuts-up"), on).when(on, |button| {
                button.on_click(cx.listener(move |_, _, _, cx| {
                    let list = Store::current(cx);
                    Store::set(cx, text_shortcuts::moved(&list.shortcuts, &id, true));
                }))
            })
        };
        let down = {
            let id = id.clone();
            let on = enabled && index + 1 < count;
            button("down", fl!("settings-text-shortcuts-down"), on).when(on, |button| {
                button.on_click(cx.listener(move |_, _, _, cx| {
                    let list = Store::current(cx);
                    Store::set(cx, text_shortcuts::moved(&list.shortcuts, &id, false));
                }))
            })
        };
        let edit = {
            let shortcut = shortcut.clone();
            button("edit", fl!("settings-text-shortcuts-edit"), enabled).when(enabled, |button| {
                button.on_click(cx.listener(move |this, _, window, cx| {
                    this.edit_text_shortcut(Some(shortcut.clone()), window, cx);
                }))
            })
        };
        let delete = button("delete", fl!("settings-text-shortcuts-delete"), enabled).when(
            enabled,
            |button| {
                button.on_click(cx.listener(move |_, _, _, cx| {
                    let list = Store::current(cx);
                    Store::set(cx, text_shortcuts::without(&list.shortcuts, &id));
                }))
            },
        );
        div()
            .debug_selector(move || format!("text-shortcut-{index}"))
            .flex()
            .items_center()
            .gap(px(12.))
            .py(px(8.))
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(safe_text(&shortcut.label(), 80)),
                    )
                    .child(
                        div()
                            .line_clamp(2)
                            .text_size(px(11.))
                            .line_height(px(15.))
                            .text_color(rgb(theme.subtext()))
                            .child(safe_text(&shortcut.text, 400)),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .gap(px(6.))
                    .child(up)
                    .child(down)
                    .child(edit)
                    .child(delete),
            )
    }

    /// The title and text being written, with Save and Cancel.
    fn text_shortcut_form(&self, editing: &Editing, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let written = editing.text.read(cx).text();
        let length = written.chars().count();
        let too_long = length > MAX_TEXT_CHARS;
        let no_room = no_room_for(&Store::current(cx).shortcuts, &editing.id);
        let ready = !written.trim().is_empty() && !too_long && !no_room;
        let field = |selector: &'static str| {
            div()
                .debug_selector(move || selector.into())
                .px(px(8.))
                .py(px(6.))
                .rounded(px(corners::CONTROL))
                .border_1()
                .border_color(rgb(theme.active))
                .bg(rgb(theme.background))
        };
        let label = |text: String| {
            div()
                .text_size(px(11.))
                .text_color(rgb(theme.subtext()))
                .child(text)
        };
        let text = editing.text.clone();
        div()
            .debug_selector(|| "text-shortcut-form".into())
            .flex()
            .flex_col()
            .gap(px(6.))
            .my(px(8.))
            .p(px(12.))
            .rounded(px(corners::CONTROL))
            .border_1()
            .border_color(rgb(theme.active))
            .bg(rgb(theme.surface))
            .child(label(fl!("settings-text-shortcuts-title")))
            .child(field("text-shortcut-title").child(editing.title.clone()))
            .child(label(fl!("settings-text-shortcuts-text")))
            .child(
                field("text-shortcut-text")
                    // The field takes several lines: Enter starts a new one.
                    .on_key_down(move |event: &KeyDownEvent, window, cx| {
                        let modifiers = event.keystroke.modifiers;
                        if event.keystroke.key != "enter"
                            || modifiers.control
                            || modifiers.alt
                            || modifiers.platform
                            || text.read(cx).is_composing()
                        {
                            return;
                        }
                        text.update(cx, |input, cx| {
                            input.replace_text_in_range(None, "\n", window, cx);
                        });
                        cx.stop_propagation();
                    })
                    .child(editing.text.clone()),
            )
            .when(no_room, |form| {
                form.child(
                    div()
                        .debug_selector(|| "text-shortcut-full".into())
                        .text_size(px(11.))
                        .text_color(rgb(theme.ink(theme.palette[1])))
                        .child(fl!("settings-text-shortcuts-full", count = MAX_SHORTCUTS)),
                )
            })
            // Shown near the limit, and why Save is off past it.
            .when(length > MAX_TEXT_CHARS / 10 * 9, |form| {
                form.child(
                    div()
                        .debug_selector(|| "text-shortcut-length".into())
                        .text_size(px(11.))
                        .text_color(rgb(if too_long {
                            theme.ink(theme.palette[1])
                        } else {
                            theme.subtext()
                        }))
                        .child(fl!(
                            "settings-text-shortcuts-length",
                            count = length,
                            limit = MAX_TEXT_CHARS
                        )),
                )
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        ui::button(
                            theme,
                            "text-shortcut-cancel",
                            fl!("settings-text-shortcuts-cancel"),
                            true,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.cancel_text_shortcut(window, cx);
                        })),
                    )
                    .child(
                        ui::button(
                            theme,
                            "text-shortcut-save",
                            fl!("settings-text-shortcuts-save"),
                            ready,
                        )
                        .when(ready, |button| {
                            button.on_click(cx.listener(|this, _, window, cx| {
                                this.save_text_shortcut(window, cx);
                            }))
                        }),
                    ),
            )
    }
}

/// Whether `list` is full for a new shortcut `id`: one already listed is
/// edited in its place and always fits.
fn no_room_for(list: &[TextShortcut], id: &str) -> bool {
    list.len() >= MAX_SHORTCUTS && list.iter().all(|own| own.id != id)
}
