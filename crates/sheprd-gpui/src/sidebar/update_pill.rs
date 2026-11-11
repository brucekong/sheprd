//! The pill on a host header when Sheprd on that machine has a release to
//! take, is taking it, or failed to (`crate::remote_update`). A click opens
//! the app's confirmation; nothing reaches the machine before that.

use super::{label_text, metrics::glyph_width};
use crate::{SheprdWindow, config::FontConfig, endpoint::Endpoint, i18n::fl, remote_update::Pill};
use gpui::{prelude::*, *};

/// Space either side of the pill's text.
const PADDING: f32 = 5.;
/// The failure's red, adjusted to the theme as the status dots are.
const FAILED: u32 = 0xe06c75;

impl SheprdWindow {
    /// The pill for `endpoint`'s header and the width it takes, if its
    /// machine has something to say about updating.
    pub(super) fn update_pill(
        &self,
        endpoint: &Endpoint,
        prefix: &str,
        font: &FontConfig,
        cx: &mut Context<Self>,
    ) -> Option<(f32, Stateful<Div>)> {
        let (machine, pill) = self.remote_update_pill(endpoint)?;
        let theme = &self.theme;
        let host = endpoint.label.as_str();
        let (text, tooltip, color) = match &pill {
            Pill::Offered { latest, .. } => (
                fl!("remote-update-pill"),
                match latest {
                    Some(latest) => fl!(
                        "remote-update-pill-tooltip",
                        host = host,
                        latest = latest.as_str()
                    ),
                    None => fl!("remote-update-pill-tooltip-newer", host = host),
                },
                theme.primary(),
            ),
            Pill::Updating => (
                fl!("remote-update-pill-updating"),
                fl!("remote-update-pill-tooltip-updating", host = host),
                theme.muted,
            ),
            Pill::Failed(_) => (
                fl!("remote-update-pill-failed"),
                fl!("remote-update-pill-tooltip-failed", host = host),
                theme.ink(FAILED),
            ),
        };
        let size = (font.size * 0.8).round();
        // The pill's text is smaller than the label's.
        let glyph = glyph_width(font) * size / font.size;
        let width = (text.chars().count() as f32 * glyph + 2. * PADDING).ceil() + 2.;
        let id = format!("{prefix}host-update-{}", endpoint.id);
        let tip = SharedString::from(tooltip);
        let (foreground, surface) = (theme.foreground, theme.surface);
        let element = div()
            .id(SharedString::from(id.clone()))
            .debug_selector(move || id.clone())
            .flex_none()
            .px(px(PADDING))
            .rounded_full()
            .border_1()
            .border_color(rgb(color))
            .text_color(rgb(color))
            .text_size(px(size))
            .cursor_pointer()
            .child(label_text(&text))
            .tooltip(move |_, cx| {
                cx.new(|_| crate::usage::Hint {
                    text: tip.clone(),
                    foreground,
                    surface,
                })
                .into()
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.ask_remote_update(&machine, window, cx);
            }));
        Some((width, element))
    }
}
