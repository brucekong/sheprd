//! Each terminal tab says what its menu may do, so a phone hides what the
//! daemon would refuse: a Herdr desktop space's tab keeps its own layout.

use super::*;
use model::{Home, Sources, TabAction};

fn home(layout: &Layout) -> Home {
    Home::project(Sources {
        computer: "Studio",
        fingerprint: "FP",
        layout,
        load: None,
        summaries: HashMap::new(),
        pull_requests: &Default::default(),
        shortcuts_ms: 0,
        tags: Default::default(),
        version: "20261109.1",
        waiting: 0,
    })
}

#[test]
fn a_desktop_space_tab_offers_no_split_zoom_or_rename() {
    let mut layout = layout(EndpointId::random());
    let mut space = layout.workspaces[0].terminals[0].clone();
    space.tab = "core:app-tab:6162".into();
    layout.workspaces[0].terminals.push(space);
    let home = home(&layout);
    let terminals = &home.workspaces[0].terminals;
    assert_eq!(terminals[0].actions, TabAction::all());
    assert!(terminals[1].actions.is_empty());
    let wire = serde_json::to_value(&terminals[0]).unwrap();
    assert_eq!(
        wire["actions"],
        serde_json::json!(["rename", "split", "zoom"])
    );
}

#[test]
fn an_older_server_without_actions_allows_them_all() {
    let home = home(&layout(EndpointId::random()));
    let mut wire = serde_json::to_value(&home.workspaces[0].terminals[0]).unwrap();
    wire.as_object_mut().unwrap().remove("actions");
    let read: model::Terminal = serde_json::from_value(wire).unwrap();
    assert_eq!(read.actions, TabAction::all());
}
