//! A herdr-core terminal drawn by the app: its own focus, keys, and IME
//! text, painted with the window's terminal painter. One per tab showing it;
//! any number may show the same terminal, as observers.

mod accessibility;
mod copy_mode;
mod find;
mod history;
pub(super) mod keys;
mod links;
pub(super) mod paste;
mod pointer;
mod quote;
mod slide;

pub(crate) use links::LinkClick;

use super::{
    effects::Effects,
    screen::Screen,
    selection::{Cell, Selection},
    worker::{Command, Link},
};
use crate::{
    config::{FontConfig, OptionAsAlt, Padding, Theme},
    herdr_settings::RightClickPassthrough,
    i18n::fl,
    terminal::{HeldKeys, key_input, viewport},
    terminal_painter::{Ground, ImageTarget, PlacedImages, TerminalPainter},
};
use gpui::{prelude::*, *};
use sheprd_backend::{ClientPaneInputEvent, FrameData, Size, TerminalId};
use std::{cell::RefCell, ops::Range, rc::Rc, sync::Arc};

/// How the window draws terminals and handles the pointer over them,
/// copied into each view and refreshed when fonts, theme, or options change.
/// Herdr's shared `ui` settings apply to herdr-core panes as to Herdr panes.
#[derive(Clone, PartialEq)]
pub(crate) struct Look {
    pub(crate) background: Hsla,
    pub(crate) cell_height: f32,
    pub(crate) cell_width: f32,
    /// The window's own look, for what the view draws over its cells.
    pub(crate) chrome: Rc<Chrome>,
    /// Herdr's `ui.copy_on_select`: releasing a selection copies it.
    pub(crate) copy_on_select: bool,
    pub(crate) font: Font,
    pub(crate) ground: Ground,
    /// The GUI's `keep_selection_after_copy`: a copied selection stays
    /// highlighted.
    pub(crate) keep_selection: bool,
    pub(crate) option_as_alt: OptionAsAlt,
    pub(crate) padding: Padding,
    /// Herdr's `ui.right_click_passthrough_modifier`.
    pub(crate) right_click_passthrough: RightClickPassthrough,
    /// Herdr's `ui.mouse_scroll_lines`: lines one wheel notch scrolls.
    pub(crate) scroll_lines: std::num::NonZeroU16,
    /// Whether a scroll through the history slides into place.
    pub(crate) smooth_scroll: bool,
}

/// The window's colors and interface font, for the find bar a view draws.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Chrome {
    pub(crate) theme: Theme,
    pub(crate) ui: FontConfig,
}

/// How much of the background covers a stale frame, as Herdr 1's
/// reconnecting overlay dims its last picture.
const STALE_DIM: f32 = 0.63;

/// What a view asks of the window it is in.
#[derive(Debug)]
pub(crate) enum ToWindow {
    /// Open a link clicked with the link modifier held.
    OpenLink(LinkClick),
    /// Say that a selection went to the clipboard.
    Copied,
    /// Say why something the user did was refused.
    Refused(crate::Error),
}

/// How a view reaches the window it is in.
pub(crate) type WindowCall = Rc<dyn Fn(ToWindow, &mut Window, &mut App)>;

/// The least room a split's cells keep from their pane's edges, across
/// and down.
const PANE_PADDING: (f32, f32) = (6., 3.);

/// How a pane of a split draws its cells, from the tab layout's chrome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PaneLook {
    /// The cells' top and bottom corners, matching a rounded pane.
    pub(crate) corners: (f32, f32),
    /// The veil over a pane not in use, out of 255. Its cursor hides too, so
    /// only the pane keys go to shows one.
    pub(crate) veil: Option<u8>,
}

pub(crate) struct CoreTerminalView {
    /// Where the cells were last painted, for the IME's candidate window.
    bounds: Option<Bounds<Pixels>>,
    copy_mode: Option<copy_mode::CopyModeState>,
    /// The history read whose text goes to the clipboard.
    copying: Option<history::Reading>,
    /// The selection whose copy landed, to be said on the next poll.
    copied: Option<Selection>,
    /// Whether a chat view covers this pane: the canvas still fits its
    /// bounds so the terminal and chat keep their size, without painting
    /// cells or overlays under the chat.
    covered: bool,
    /// Something typed while the connection was away, for the window to
    /// say it was not sent.
    dropped: std::cell::Cell<bool>,
    /// The terminal's working directory, as herdr-core last reported it.
    cwd: Option<String>,
    /// Bells and clipboard writes the window has not taken yet.
    effects: Effects,
    /// The history text asked for an editor, once it arrived.
    edit_ready: Option<Result<String, super::Error>>,
    /// The history request whose text goes to an editor.
    editing: Option<u64>,
    find: Option<find::Find>,
    /// The query the find bar last held, for the next one.
    find_query: String,
    focus: FocusHandle,
    /// Releases for keys pressed while the program asked for them.
    held_keys: HeldKeys,
    /// Where the pointer is while it drags a selection, which scrolls the
    /// history while it is held past the screen.
    dragged_to: Option<Point<Pixels>>,
    /// The cell a button-less pointer last reported, so movement inside
    /// one cell is not reported again.
    hovered: Option<Cell>,
    id: TerminalId,
    link: Link,
    /// The link under the pointer and the one a press went down on.
    links: links::Links,
    look: Look,
    /// The history read whose text a note quotes.
    quoting: Option<history::Reading>,
    /// The text a note quotes, once read from the history.
    quote_ready: Option<Result<String, super::Error>>,
    /// Text an IME is composing; committed text goes to the terminal.
    marked: String,
    /// The last history request's ID.
    next_request: u64,
    /// A mirror's view: it watches and never types.
    observe: bool,
    painter: Rc<RefCell<TerminalPainter>>,
    /// Set while the view is a pane of a split.
    pane: Option<PaneLook>,
    /// Where the button held down is going.
    press: Option<pointer::Press>,
    /// Whether the terminal runs in another host's core.
    remote: bool,
    /// When a drag past the screen last scrolled the history.
    scrolled_at: Option<std::time::Instant>,
    selection: Option<Selection>,
    /// A scroll sliding into place.
    slide: Option<slide::Slide>,
    /// The last frame painted with its cursor hidden, and the frame it came
    /// from, so a veiled pane copies a frame once rather than every paint.
    sleeping: Option<(Arc<FrameData>, Arc<FrameData>)>,
    screen: Screen,
    /// The cell size in whole pixels the terminal was last told.
    sent_cell: Option<(u16, u16)>,
    sent_focus: Option<bool>,
    sent_size: Option<Size>,
    /// The connection to herdr-core dropped: the last frame stays, dimmed,
    /// and nothing typed is sent until the terminal is attached again.
    stale: bool,
    /// The window's end; a view on its own, as in tests, has none.
    to_window: Option<WindowCall>,
    /// Wheel motion short of a whole row, kept for the next event.
    wheel_rest: f32,
}

impl CoreTerminalView {
    /// The terminal it shows.
    pub(crate) fn terminal(&self) -> &TerminalId {
        &self.id
    }

    pub(crate) fn new(
        id: TerminalId,
        link: Link,
        look: Look,
        painter: Rc<RefCell<TerminalPainter>>,
        observe: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            bounds: None,
            copy_mode: None,
            copying: None,
            copied: None,
            covered: false,
            dropped: std::cell::Cell::new(false),
            cwd: None,
            effects: Effects::default(),
            edit_ready: None,
            editing: None,
            dragged_to: None,
            find: None,
            find_query: String::new(),
            focus: cx.focus_handle(),
            held_keys: HeldKeys::default(),
            hovered: None,
            id,
            link,
            links: links::Links::default(),
            look,
            quoting: None,
            quote_ready: None,
            marked: String::new(),
            next_request: 0,
            observe,
            painter,
            pane: None,
            press: None,
            remote: false,
            scrolled_at: None,
            selection: None,
            slide: None,
            sleeping: None,
            screen: Screen::default(),
            sent_cell: None,
            sent_focus: None,
            sent_size: None,
            stale: false,
            to_window: None,
            wheel_rest: 0.,
        }
    }

    /// Keeps the last frame, dimmed, while the connection is away.
    pub(crate) fn set_stale(&mut self, cx: &mut Context<Self>) {
        self.stale = true;
        cx.notify();
    }

    pub(crate) fn is_stale(&self) -> bool {
        self.stale
    }

    /// Attaches the terminal again through `link` after a dropped
    /// connection. The old frame shows until the new link draws one, and the
    /// terminal is told its size and focus again.
    pub(crate) fn relink(&mut self, link: Link, cx: &mut Context<Self>) {
        self.link = link;
        // The old link's answers never arrive: copy mode stops waiting on
        // its motion or search and drops the keys queued behind it, and a
        // copy or an editor's read asked of it is dropped.
        if let Some(state) = &mut self.copy_mode {
            state.relinked();
        }
        self.copying = None;
        self.editing = None;
        self.stale = false;
        self.screen.failure = None;
        self.sent_focus = None;
        self.sent_size = None;
        cx.notify();
    }

    /// Whether something was typed while the connection was away, since
    /// the last ask.
    pub(crate) fn take_dropped(&self) -> bool {
        self.dropped.take()
    }

    #[cfg(test)]
    pub(crate) fn has_frame(&self) -> bool {
        self.screen.frame.is_some()
    }

    /// The text revision of the screen shown, for tests that wait for one
    /// update rather than any.
    #[cfg(test)]
    pub(crate) fn screen_revision(&self) -> u64 {
        self.screen.revision
    }

    /// Lets the view reach the window it is in.
    pub(crate) fn with_window(mut self, to_window: WindowCall) -> Self {
        self.to_window = Some(to_window);
        self
    }

    /// Keeps the working directory a printed relative path is read
    /// against, and reads the link under the pointer again when it moved.
    pub(crate) fn set_cwd(&mut self, cwd: Option<&std::path::Path>, cx: &mut Context<Self>) {
        let cwd = cwd.and_then(std::path::Path::to_str);
        if self.cwd.as_deref() != cwd {
            self.cwd = cwd.map(str::to_owned);
            self.rehover_link(cx);
        }
    }

    /// Whether the pane's terminal runs in another host's core, whose
    /// printed paths are no link to a file on this computer.
    pub(crate) fn set_remote(&mut self, remote: bool, cx: &mut Context<Self>) {
        if self.remote != remote {
            self.remote = remote;
            self.rehover_link(cx);
        }
    }

    pub(crate) fn title(&self) -> Option<&str> {
        self.screen.title.as_deref()
    }

    pub(crate) fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    /// Whether the terminal's program ended.
    pub(crate) fn ended(&self) -> bool {
        self.screen.ended.is_some()
    }

    /// How the terminal's program ended, once it has.
    pub(crate) fn exit(&self) -> Option<sheprd_backend::Exit> {
        self.screen.ended
    }

    /// The size the terminal was last given, in cells.
    /// Where the terminal's cells were last laid out, in the window.
    pub(crate) fn bounds(&self) -> Option<Bounds<Pixels>> {
        self.bounds
    }

    pub(crate) fn size(&self) -> Option<Size> {
        self.sent_size
    }

    /// Draws the view as a pane of a split, or bare. Read by this frame's
    /// render, which follows the window's, so it needs no notify.
    pub(crate) fn set_pane(&mut self, pane: Option<PaneLook>) {
        self.pane = pane;
    }

    /// Marks whether a chat view covers this pane on this frame.
    pub(crate) fn set_covered(&mut self, covered: bool) {
        self.covered = covered;
    }

    /// The frame to paint: a veiled pane's with its cursor hidden.
    fn frame_to_paint(&mut self) -> Option<Arc<FrameData>> {
        let frame = self.screen.frame.clone()?;
        let veiled = self.pane.is_some_and(|pane| pane.veil.is_some());
        if !veiled || frame.cursor.is_none() {
            self.sleeping = None;
            return Some(frame);
        }
        if let Some((source, hidden)) = &self.sleeping
            && Arc::ptr_eq(source, &frame)
        {
            return Some(hidden.clone());
        }
        let mut hidden = (*frame).clone();
        hidden.cursor = None;
        let hidden = Arc::new(hidden);
        self.sleeping = Some((frame, hidden.clone()));
        Some(hidden)
    }

    pub(crate) fn changed(&self) -> bool {
        self.link.mailbox.changed()
    }

    /// Takes what the worker produced; true when there was a new screen.
    /// The answers come with the screen, never made from a newer one, so
    /// a copy checks its rows against text at least as new as it read.
    pub(crate) fn poll(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let (screen, answers) = self.link.mailbox.take_with_answers();
        let changed = screen.is_some();
        if let Some(mut screen) = screen {
            // Attached again, the last frame stays until a new one comes.
            if screen.frame.is_none() {
                screen.frame = self.screen.frame.take();
            }
            // A mirror only watches; the tab driving the terminal rings and
            // copies.
            if !self.observe {
                self.effects.merge(Effects {
                    bells: screen.bells,
                    clipboard: screen.clipboard.take(),
                });
            }
            let before = std::mem::replace(&mut self.screen, screen);
            self.slide_from(&before);
            // Output into a full history drops its oldest lines, a clear
            // drops them all, and a resize rewraps them, so the rows a
            // selection or copy mode's mark holds now hold other text.
            if self.screen.revision != before.revision
                && (self.screen.history.full
                    || self.screen.history.history < before.history.history
                    || self.screen.grid() != before.grid())
            {
                self.selection = None;
                if let Some(state) = &mut self.copy_mode {
                    state.forget_mark();
                }
            }
            self.rehover_link(cx);
            cx.notify();
        }
        self.take_answers(answers, cx);
        if let Some(copied) = self.copied.take() {
            self.finish_copy(copied, window, cx);
        }
        if self.find.is_some() {
            self.poll_find();
        }
        self.poll_copy_mode(window, cx);
        self.follow_drag(cx);
        changed
    }

    /// Bells and the clipboard write found since the last take.
    pub(crate) fn take_effects(&mut self) -> Effects {
        std::mem::take(&mut self.effects)
    }

    pub(crate) fn set_look(&mut self, look: Look, cx: &mut Context<Self>) {
        self.look = look;
        self.restyle_find(cx);
        cx.notify();
    }

    fn send(&self, input: ClientPaneInputEvent) {
        if self.stale {
            // The pointer moving is not typing; only what was typed is told.
            if !matches!(input, ClientPaneInputEvent::Mouse { .. }) {
                self.dropped.set(true);
            }
            return;
        }
        self.link.send(Command::Input(input));
    }

    /// Sizes the terminal to the cells that fit where it is laid out.
    fn fit(&mut self, bounds: Bounds<Pixels>) {
        self.bounds = Some(bounds);
        let fits = viewport(
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
            self.look.cell_width,
            self.look.cell_height,
        );
        // Clamped first, so the casts cannot truncate.
        let whole = |side: f32| side.round().clamp(1., f32::from(u16::MAX)) as u16;
        let cell = (whole(self.look.cell_width), whole(self.look.cell_height));
        if self.sent_cell != Some(cell) {
            self.sent_cell = Some(cell);
            self.link.send(Command::CellPixels(cell.0, cell.1));
        }
        let Ok(size) = Size::new(fits.cols, fits.rows) else {
            return;
        };
        if self.sent_size != Some(size) && !self.stale {
            self.sent_size = Some(size);
            self.link.send(Command::Resize(size));
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // Typing in the find bar or copy mode's prompt is the field's, not
        // the terminal's.
        if self.find_focused(window, cx) || self.copy_search_focused(window, cx) {
            return;
        }
        if self.copy_mode_active() {
            self.copy_mode_key(event, window, cx);
            return;
        }
        let modifiers = event.keystroke.modifiers;
        // Cmd-C copies a selection, as the window's copy does a Herdr pane's.
        // Ctrl-C does too only while `copy_on_select` is off, as in Herdr,
        // where the highlight waits for that copy; a selection the release
        // already copied must not stop Ctrl-C from interrupting the program.
        let copies = if modifiers.platform {
            !modifiers.control && !modifiers.shift
        } else {
            modifiers.control && (modifiers.shift || !self.look.copy_on_select)
        };
        if copies
            && !modifiers.alt
            && event.keystroke.key.eq_ignore_ascii_case("c")
            && self.copy_selection()
        {
            self.announce_copy();
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        // Pasting reads the clipboard here, as the window does for a Herdr
        // pane; the Herdr pane behind this one must not take it.
        let has_text = || {
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .is_some_and(|text| !text.is_empty())
        };
        if paste::pastes(
            &event.keystroke,
            cfg!(any(windows, target_os = "linux")),
            has_text,
        ) {
            self.paste_clipboard(window, cx);
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        // The window's own shortcuts keep working over the terminal.
        self.held_keys.forget(&event.keystroke.key);
        if modifiers.platform {
            return;
        }
        // Typing ends a selection.
        if self.selection.take().is_some() {
            cx.notify();
        }
        let option_keys = match event.keystroke.modifiers.alt {
            true => crate::input::held_option_keys(),
            false => crate::config::OptionKeys::LEFT,
        };
        let alt_keys = self
            .look
            .option_as_alt
            .sends_alt(cx.keyboard_layout().id(), option_keys);
        if !self.marked.is_empty() {
            return;
        }
        // Printable text must bubble on: macOS calls the input handler only
        // for a key nothing handled. The window ignores keys while this view
        // holds focus, so none reach a Herdr 1 pane behind it. A program that
        // asked for every key gets printable keys as key events instead.
        let keys = self.screen.keys;
        let input = key_input(event, alt_keys).or_else(|| {
            keys.every_key
                .then(|| keys::printable_key(&event.keystroke, event.is_held))
                .flatten()
        });
        if let Some(input) = input {
            let input = self
                .held_keys
                .press(&event.keystroke.key, input, keys.releases);
            self.send(input);
            window.prevent_default();
            cx.stop_propagation();
        }
    }

    fn report_focus(&mut self, window: &Window) {
        let focused = self.focus.is_focused(window);
        if self.sent_focus != Some(focused) && !self.stale {
            self.sent_focus = Some(focused);
            self.link.send(Command::Focus(focused));
        }
    }

    fn status(&self) -> Option<String> {
        if self.stale {
            return Some(fl!("core-terminals-reconnecting"));
        }
        if let Some(failure) = &self.screen.failure {
            return Some(failure.clone());
        }
        let ended = self.screen.ended.map(|exit| match exit.code {
            Some(code) => fl!("core-terminals-exited", code = code),
            None => fl!("core-terminals-ended"),
        });
        ended.or_else(|| self.observe.then(|| fl!("core-terminals-watching")))
    }
}

impl Focusable for CoreTerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for CoreTerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.report_focus(window);
        let entity = cx.entity();
        let fitting = entity.clone();
        let painter = self.painter.clone();
        let look = self.look.clone();
        let frame = self.frame_to_paint();
        let graphics = self.screen.graphics.clone();
        let images = self.screen.images.0.clone();
        let mut highlights = self.find_highlights();
        highlights.extend(self.copy_mode_highlights());
        highlights.extend(self.highlights());
        let find_bar = self.render_find(cx);
        let copy_badge = self.render_copy_mode(cx);
        let link_rows = self.link_rows();
        let on_link = !link_rows.is_empty();
        let covered = self.covered;
        let pane = self.pane;
        let background = self.look.background;
        let focus = self.focus.clone();
        let status = (!covered).then(|| self.status()).flatten();
        let selecting = self.press == Some(pointer::Press::Selecting);
        let sliding = self.sliding(crate::motion::now());
        if sliding.is_some() {
            window.request_animation_frame();
        }
        let body = div()
            .relative()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .child(
                canvas(
                    move |bounds, _, cx| fitting.update(cx, |view, _| view.fit(bounds)),
                    move |bounds, _, window, cx| {
                        if covered {
                            return;
                        }
                        // A selection dragged past the view's edges keeps
                        // following the pointer, which no longer hovers it.
                        if selecting {
                            let dragging = entity.clone();
                            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                                if phase == DispatchPhase::Bubble {
                                    dragging.update(cx, |view, cx| view.drag_outside(event, cx));
                                }
                            });
                        }
                        window.handle_input(
                            &focus,
                            crate::input::TerminalInputHandler::new(bounds, entity, false),
                            cx,
                        );
                        let Some(frame) = &frame else {
                            return;
                        };
                        window.with_content_mask(Some(ContentMask { bounds }), |window| {
                            let row = |rows: f32| point(px(0.), px(rows * look.cell_height));
                            let mut origin = bounds.origin;
                            if let Some(sliding) = &sliding {
                                origin -= row(sliding.lift);
                                painter.borrow_mut().paint_frame(
                                    &sliding.behind,
                                    origin + row(sliding.behind_at),
                                    Some(bounds.size),
                                    look.ground,
                                    look.cell_width,
                                    &look.font,
                                    &[],
                                    &[],
                                    None,
                                    None,
                                    window,
                                    cx,
                                );
                            }
                            painter.borrow_mut().paint_frame(
                                frame,
                                origin,
                                Some(bounds.size),
                                look.ground,
                                look.cell_width,
                                &look.font,
                                &highlights,
                                &[],
                                None,
                                (!graphics.is_empty()).then_some(PlacedImages {
                                    placements: &graphics,
                                    images: &images,
                                    target: ImageTarget::Main,
                                }),
                                window,
                                cx,
                            );
                            if !link_rows.is_empty() {
                                painter.borrow().paint_link(
                                    frame,
                                    bounds.origin,
                                    look.cell_width,
                                    &link_rows,
                                    window,
                                );
                            }
                        });
                    },
                )
                .size_full(),
            )
            .when(self.stale && !covered, |body| {
                body.child(div().absolute().inset_0().bg(background.opacity(STALE_DIM)))
            })
            .when_some(
                (!covered)
                    .then_some(pane)
                    .flatten()
                    .and_then(|pane| pane.veil),
                |body, veil| {
                    body.child(
                        div()
                            .absolute()
                            .inset_0()
                            .bg(background.opacity(f32::from(veil) / 255.)),
                    )
                },
            )
            .when_some((!covered).then_some(find_bar).flatten(), |body, bar| {
                body.child(bar)
            })
            .when_some((!covered).then_some(copy_badge).flatten(), |body, badge| {
                body.child(badge)
            })
            .when_some(status, |body, status| {
                body.child(
                    div()
                        .absolute()
                        .bottom_2()
                        .right_3()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(rgba(0x0000_0080))
                        .text_color(white())
                        .text_xs()
                        .child(status),
                )
            });
        self.look
            .padding
            .pad(div())
            .id(SharedString::from(format!("core-terminal-{}", self.id)))
            .key_context("CoreTerminal")
            .track_focus(&self.focus)
            .map(|root| self.accessible(root, window, cx))
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .size_full()
            .bg(self.look.background)
            // A hand marks the verified link a click opens.
            .when(on_link, |root| root.cursor_pointer())
            .when_some(pane, |root, pane| {
                // A split's cells never run into its gutter, whatever the
                // configured padding.
                let padding = self.look.padding;
                root.pl(px(padding.x.start().max(PANE_PADDING.0)))
                    .pr(px(padding.x.end().max(PANE_PADDING.0)))
                    .pt(px(padding.y.start().max(PANE_PADDING.1)))
                    .pb(px(padding.y.end().max(PANE_PADDING.1)))
                    .overflow_hidden()
                    .rounded_t(px(pane.corners.0))
                    .rounded_b(px(pane.corners.1))
            })
            .on_key_down(cx.listener(Self::key_down))
            .on_key_up(cx.listener(Self::key_up))
            .on_action(cx.listener(|this, _: &crate::actions::Copy, window, cx| {
                if !this.find_focused(window, cx)
                    && !this.copy_search_focused(window, cx)
                    && this.copy_selection()
                {
                    this.announce_copy();
                }
            }))
            .on_action(cx.listener(|this, _: &crate::actions::Paste, window, cx| {
                if !this.find_focused(window, cx)
                    && !this.copy_search_focused(window, cx)
                    && !this.copy_mode_active()
                {
                    this.paste_clipboard(window, cx);
                }
            }))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                this.drop_files(paths, window, cx);
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_modifiers_changed(
                cx.listener(|this, event: &ModifiersChangedEvent, window, cx| {
                    this.hover_link(window.mouse_position(), event.modifiers, cx);
                }),
            )
            // Moves off the pane reach it no more, so leaving drops the hover.
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if !*hovered {
                    this.leave_link(cx);
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up(MouseButton::Right, cx.listener(Self::mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_scroll(cx.listener(Self::scroll_wheel))
            .child(body)
    }
}

impl EntityInputHandler for CoreTerminalView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let text: Vec<u16> = self.marked.encode_utf16().collect();
        let range = range.start.min(text.len())..range.end.min(text.len());
        *adjusted = Some(range.clone());
        Some(String::from_utf16_lossy(&text[range]))
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let end = self.marked.encode_utf16().count();
        Some(UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        (!self.marked.is_empty()).then(|| 0..self.marked.encode_utf16().count())
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked.clear();
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked.clear();
        if self.copy_mode_active() {
            self.copy_mode_text(text, window, cx);
            cx.notify();
            return;
        }
        if !text.is_empty() {
            self.send(ClientPaneInputEvent::TextCommit(text.into()));
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked = text.into();
        cx.notify();
    }

    /// The cursor's cell, so the IME's candidate window opens beside it.
    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let bounds = self.bounds?;
        let cursor = self.screen.frame.as_ref()?.cursor.as_ref()?;
        let origin = bounds.origin
            + point(
                px(f32::from(cursor.x) * self.look.cell_width),
                px(f32::from(cursor.y) * self.look.cell_height),
            );
        Some(Bounds::new(
            origin,
            size(px(self.look.cell_width), px(self.look.cell_height)),
        ))
    }

    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}
