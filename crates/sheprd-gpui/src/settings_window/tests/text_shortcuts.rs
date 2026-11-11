//! Settings > Text Shortcuts: the list in order, and adding, editing,
//! reordering, and deleting a shortcut through the shared store.
use super::*;
use crate::text_shortcuts::Store;
use gpui::{px, size};
use sheprd_remote_control::control::shortcuts::{TextShortcut, TextShortcuts};

fn shortcut(id: &str, title: &str, text: &str) -> TextShortcut {
    TextShortcut {
        id: id.into(),
        title: title.into(),
        text: text.into(),
    }
}

fn page(cx: &mut TestAppContext) -> (Entity<SettingsWindow>, &mut VisualTestContext) {
    cx.update(|cx| {
        Store::hold(
            cx,
            TextShortcuts::new(
                [
                    shortcut("a", "Carry on", "Please continue"),
                    shortcut("b", "", "Review the diff"),
                ],
                10,
            ),
        );
    });
    let (view, cx) = cx.add_window_view(skill_fixture);
    view.update_in(cx, |view, window, cx| {
        view.select_section(Section::TextShortcuts, window, cx);
    });
    cx.simulate_resize(size(px(960.), px(1200.)));
    draw(cx);
    (view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let point = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("missing {selector}"))
        .center();
    cx.simulate_click(point, Default::default());
    cx.run_until_parked();
    draw(cx);
}

fn ids(cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|_, cx| {
        Store::current(cx)
            .shortcuts
            .iter()
            .map(|shortcut| shortcut.id.clone())
            .collect()
    })
}

#[gpui::test]
fn the_page_lists_shortcuts_in_order(cx: &mut TestAppContext) {
    let (_view, cx) = page(cx);
    let first = cx.debug_bounds("text-shortcut-0").unwrap();
    let second = cx.debug_bounds("text-shortcut-1").unwrap();
    assert!(first.bottom() <= second.top());
    assert!(cx.debug_bounds("text-shortcuts-add").is_some());
    assert!(cx.debug_bounds("text-shortcuts-empty").is_none());
}

#[gpui::test]
fn moving_and_deleting_edit_the_shared_list(cx: &mut TestAppContext) {
    let (_view, cx) = page(cx);
    click(cx, "text-shortcut-down-0");
    assert_eq!(ids(cx), ["b", "a"]);
    let stamped = cx.update(|_, cx| Store::current(cx).updated_ms);
    assert!(stamped > 10, "an edit here is stamped now");
    click(cx, "text-shortcut-delete-1");
    assert_eq!(ids(cx), ["b"]);
}

#[gpui::test]
fn a_new_shortcut_saves_once_it_has_text(cx: &mut TestAppContext) {
    let (view, cx) = page(cx);
    click(cx, "text-shortcuts-add-button");
    assert!(cx.debug_bounds("text-shortcut-form").is_some());
    // The title holds the keyboard first.
    cx.simulate_input("Explain");
    draw(cx);
    // Nothing to send yet: Save does nothing.
    click(cx, "text-shortcut-save");
    assert!(cx.debug_bounds("text-shortcut-form").is_some());
    view.update_in(cx, |view, window, cx| {
        let editing = view.text_shortcuts.editing().unwrap();
        let focus = editing.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    cx.simulate_input("Explain what you changed");
    draw(cx);
    click(cx, "text-shortcut-save");
    assert!(cx.debug_bounds("text-shortcut-form").is_none());
    let list = cx.update(|_, cx| Store::current(cx));
    assert_eq!(list.shortcuts.len(), 3);
    assert_eq!(list.shortcuts[2].title, "Explain");
    assert_eq!(list.shortcuts[2].text, "Explain what you changed");
}

#[gpui::test]
fn editing_keeps_the_shortcuts_place(cx: &mut TestAppContext) {
    let (_view, cx) = page(cx);
    click(cx, "text-shortcut-edit-0");
    assert!(cx.debug_bounds("text-shortcut-form").is_some());
    cx.simulate_input(" now");
    draw(cx);
    click(cx, "text-shortcut-save");
    let list = cx.update(|_, cx| Store::current(cx));
    assert_eq!(list.shortcuts[0].id, "a");
    assert_eq!(list.shortcuts[0].text, "Please continue now");
}

#[gpui::test]
fn a_prompt_past_the_limit_shows_its_length_and_cannot_be_saved(cx: &mut TestAppContext) {
    let (view, cx) = page(cx);
    click(cx, "text-shortcuts-add-button");
    let long = "z".repeat(sheprd_remote_control::control::shortcuts::MAX_TEXT_CHARS + 1);
    view.update_in(cx, |view, window, cx| {
        let editing = view.text_shortcuts.editing().unwrap().clone();
        editing.update(cx, |input, cx| {
            input.replace_text_in_range(None, &long, window, cx);
        });
    });
    draw(cx);
    assert!(cx.debug_bounds("text-shortcut-length").is_some());
    click(cx, "text-shortcut-save");
    assert!(
        cx.debug_bounds("text-shortcut-form").is_some(),
        "nothing is cut unseen: the form stays open"
    );
    let list = cx.update(|_, cx| Store::current(cx));
    assert_eq!(list.shortcuts.len(), 2);
}

#[gpui::test]
fn a_new_shortcut_is_kept_when_another_device_fills_the_list(cx: &mut TestAppContext) {
    let (view, cx) = page(cx);
    click(cx, "text-shortcuts-add-button");
    view.update_in(cx, |view, window, cx| {
        let editing = view.text_shortcuts.editing().unwrap().clone();
        editing.update(cx, |input, cx| {
            input.replace_text_in_range(None, "Carry on", window, cx);
        });
    });
    // A phone fills the list while the form is open.
    let max = sheprd_remote_control::control::shortcuts::MAX_SHORTCUTS;
    cx.update(|_, cx| {
        let full: Vec<_> = (0..max)
            .map(|index| shortcut(&format!("p{index}"), "", "go"))
            .collect();
        Store::set(cx, full);
    });
    draw(cx);
    assert!(cx.debug_bounds("text-shortcut-full").is_some());
    click(cx, "text-shortcut-save");
    assert!(
        cx.debug_bounds("text-shortcut-form").is_some(),
        "the draft stays"
    );
    let list = cx.update(|_, cx| Store::current(cx));
    assert_eq!(list.shortcuts.len(), max);
    assert!(list.shortcuts.iter().all(|own| own.text == "go"));
}
