//! `host update`: opens the app's confirmation that updates Sheprd on the
//! host the caller names or runs on. It never updates: the user reads which
//! release goes where and confirms in the dialog.

use super::{Place, named};
use crate::{
    SheprdWindow,
    automations::Machine,
    control::protocol::{Caller, ErrorCode, Response, hosts::Reply},
    remote_update::Pill,
};
use gpui::{Context, Window};

pub(super) fn ask(
    view: &mut SheprdWindow,
    caller: &Caller,
    host: Option<&str>,
    window: &mut Window,
    cx: &mut Context<SheprdWindow>,
) -> Response {
    let machine = match host {
        Some(name) => {
            let endpoint = match named(view, name) {
                Ok(endpoint) => endpoint,
                Err(response) => return response,
            };
            if !endpoint.enabled {
                return Response::error(
                    ErrorCode::InvalidRequest,
                    format!(
                        "Host {} is turned off; turn it on in the sidebar first",
                        endpoint.label
                    ),
                );
            }
            // Profiles name endpoints, but sessions on one machine share
            // the same app and its update state.
            Machine::of(&endpoint.connection.target).filter(|machine| !machine.is_local())
        }
        None => callers(view, caller),
    };
    let Some(machine) = machine else {
        return Response::error(
            ErrorCode::Unsupported,
            "This computer's Sheprd updates from its own update dialog; \
             `sheprd update` checks it",
        );
    };
    let Some(host) = view.remote_updates.host(&machine) else {
        return Response::error(ErrorCode::NotFound, "That host is not connected");
    };
    let label = host.label.clone();
    let latest = match host.pill() {
        Some(Pill::Offered { latest, .. }) => latest,
        // The dialog says why it failed and offers to try again.
        Some(Pill::Failed(_)) => host.remote().and_then(|remote| remote.latest.clone()),
        Some(Pill::Updating) => {
            return nothing(label, Some("it is updating now".into()));
        }
        None => return nothing(label, host.note()),
    };
    if !view.ask_remote_update(&machine, window, cx) {
        return Response::error(
            ErrorCode::WindowBusy,
            "A dialog is open in the window; close it first",
        );
    }
    super::super::work::bring_forward(window, cx);
    Response::Host {
        reply: Reply::Update {
            host: label,
            latest: Some(latest.unwrap_or_else(|| "newer".into())),
            note: None,
        },
    }
}

fn nothing(host: String, note: Option<String>) -> Response {
    Response::Host {
        reply: Reply::Update {
            host,
            latest: None,
            note,
        },
    }
}

/// The other machine the caller's workspace runs on; none on this one.
fn callers(view: &SheprdWindow, caller: &Caller) -> Option<Machine> {
    Place::of(view, caller)
        .ok()
        .and_then(|place| Machine::of(&view.endpoints[place.index].connection.target))
        .filter(|machine| !machine.is_local())
}
