//! The app's own confirmation before another machine's Sheprd updates:
//! which machine, from which version to which, and what happens there.
//! Nothing is asked of the machine until the user confirms here; `sheprd
//! host update` only opens this.

use super::Pill;
use crate::{
    SheprdWindow,
    i18n::{Localize, fl},
};
use gpui::{prelude::*, *};

impl SheprdWindow {
    pub(crate) fn render_remote_update(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let Some(machine) = self.remote_updates.dialog.clone() else {
            return div();
        };
        let Some(host) = self.remote_updates.host(&machine) else {
            return div();
        };
        let label = host.label.clone();
        let pill = host.pill();
        let startable = host.startable() && pill.is_some();
        let (title, body) = match &pill {
            Some(Pill::Offered { latest, current }) => (
                fl!("remote-update-title", host = label.as_str()),
                match (current, latest) {
                    (Some(current), Some(latest)) => fl!(
                        "remote-update-versions",
                        host = label.as_str(),
                        current = current.as_str(),
                        latest = latest.as_str()
                    ),
                    (None, Some(latest)) => fl!(
                        "remote-update-latest",
                        host = label.as_str(),
                        latest = latest.as_str()
                    ),
                    (_, None) => fl!("remote-update-newer", host = label.as_str()),
                },
            ),
            Some(Pill::Updating) => (
                fl!("remote-update-updating-title", host = label.as_str()),
                fl!("remote-update-updating-body"),
            ),
            Some(Pill::Failed(error)) => (
                fl!("remote-update-failed-title", host = label.as_str()),
                error.localize(),
            ),
            None => (
                fl!("remote-update-current-title", host = label.as_str()),
                fl!("remote-update-current-body"),
            ),
        };
        let button = |id: &'static str, label: String, primary: bool| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .p(px(8.))
                .rounded(px(crate::config::corners::CONTROL))
                .when(primary, |button| button.bg(rgb(theme.active)))
                .when(!primary, |button| button.hover(|s| s.bg(rgb(theme.active))))
                .cursor_pointer()
                .child(label)
        };
        let confirm_label = if matches!(pill, Some(Pill::Failed(_))) {
            fl!("remote-update-retry")
        } else {
            fl!("remote-update-confirm")
        };
        let failed = matches!(pill, Some(Pill::Failed(_)));
        div()
            .debug_selector(|| "remote-update".into())
            .child(
                div()
                    .p(px(8.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(
                div()
                    .debug_selector(|| "remote-update-body".into())
                    .p(px(8.))
                    .child(body),
            )
            .when(startable, |dialog| {
                dialog.child(
                    div()
                        .p(px(8.))
                        .text_color(rgb(theme.muted))
                        .child(fl!("remote-update-note")),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .p(px(8.))
                    .when(startable, |row| {
                        row.child(button("remote-update-yes", confirm_label, true).on_click(
                            cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.confirm_remote_update(window, cx);
                            }),
                        ))
                    })
                    .child(
                        button(
                            "remote-update-no",
                            if startable {
                                fl!("remote-update-cancel")
                            } else {
                                fl!("remote-update-close")
                            },
                            false,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                cx.stop_propagation();
                                if failed {
                                    this.dismiss_remote_update_failure(&machine);
                                }
                                this.remote_updates.dialog = None;
                                this.dismiss_menu(window, cx);
                            },
                        )),
                    ),
            )
    }

    /// Enter in the dialog: confirms when there is something to confirm.
    pub(crate) fn remote_update_enter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let startable = self
            .remote_updates
            .dialog
            .as_ref()
            .and_then(|machine| self.remote_updates.host(machine))
            .is_some_and(|host| host.startable() && host.pill().is_some());
        if startable {
            self.confirm_remote_update(window, cx);
        } else {
            self.remote_updates.dialog = None;
            self.dismiss_menu(window, cx);
        }
    }
}
