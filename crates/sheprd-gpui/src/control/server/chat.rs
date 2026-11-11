//! `chat status`, `chat shortcuts`, `chat show`, `chat hide`, and `chat toggle`.
use crate::control::protocol::{Call, ErrorCode, Response, chat::ChatOp};
use gpui::App;

pub(super) fn serve(call: Call<ChatOp>, cx: &mut App) -> Response {
    use super::{in_window, not_found};
    use crate::control::protocol::chat::Reply;

    let Call { caller, op } = call;
    match op {
        ChatOp::Status => in_window(&caller, cx, |view, _, _| Response::Chat {
            reply: Reply::Chats {
                chats: info::infos(view),
            },
        }),
        ChatOp::New { agent, pane } => {
            let Ok(agent) = agent.parse::<sheprd_chat_protocol::Agent>() else {
                return Response::error(
                    ErrorCode::InvalidRequest,
                    format!(
                        "Unknown agent {agent}; known agents are {}",
                        sheprd_chat_protocol::Agent::names()
                    ),
                );
            };
            let beside = pane.or_else(|| caller.pane_id.clone());
            in_window(&caller, cx, move |view, _, cx| {
                // With no Herdr pane named, as from an agent in a herdr-core
                // pane, a herdr-core tab in the active group splits instead.
                let core = beside
                    .is_none()
                    .then(|| view.core_chat_group_tab(None, cx))
                    .flatten();
                let started = match core {
                    Some(tab) => view.start_core_chat(agent, tab, cx),
                    None => view.start_chat(agent, beside.as_deref()),
                };
                match started {
                    Ok(()) => {
                        cx.notify();
                        Response::Chat {
                            reply: Reply::Started {
                                agent: agent.name().into(),
                            },
                        }
                    }
                    Err(error) => Response::error(start_code(&error), error.to_string()),
                }
            })
        }
        ChatOp::Convert { pane } => {
            let pane = pane.or_else(|| callers_pane(&caller));
            in_window(&caller, cx, move |view, _, cx| {
                match view.convert_to_chat(pane.as_deref()) {
                    Ok((pane_id, agent)) => {
                        cx.notify();
                        Response::Chat {
                            reply: Reply::Converting {
                                agent: agent.name().into(),
                                pane_id,
                            },
                        }
                    }
                    Err(error) => Response::error(convert_code(&error), error.to_string()),
                }
            })
        }
        ChatOp::Shortcuts => in_window(&caller, cx, |_, _, cx| {
            let list = crate::text_shortcuts::Store::current(cx);
            let (shortcuts, complete) =
                crate::control::protocol::chat::bounded_shortcuts(list.shortcuts.iter().map(
                    |shortcut| crate::control::protocol::chat::ShortcutInfo {
                        id: shortcut.id.clone(),
                        label: shortcut.label(),
                        title: shortcut.title.clone(),
                        text: shortcut.text.clone(),
                    },
                ));
            Response::Chat {
                reply: Reply::Shortcuts {
                    shortcuts,
                    updated_ms: list.updated_ms,
                    complete,
                },
            }
        }),
        ChatOp::View { switch, pane } => in_window(&caller, cx, |view, window, cx| {
            // A herdr-core pane is nobody's Herdr pane: the one holding the
            // keyboard comes before Herdr's focused pane.
            let pane = pane
                .or_else(|| callers_pane(&caller))
                .or_else(|| view.focused_core_chat_key(window, cx))
                .or_else(|| {
                    view.live
                        .snapshot
                        .as_ref()
                        .and_then(|snapshot| snapshot.focused_pane_id.clone())
                });
            let Some(pane) = pane else {
                return Response::error(ErrorCode::InvalidRequest, "Name a pane with --pane");
            };
            if !view.chats.pane(&pane).is_some_and(|chat| chat.available()) {
                return not_found(format!("Pane {pane} has no chat this window can show"));
            }
            let shown = switch.apply(view.chats.shown(&pane));
            view.set_chat_raw(&pane, !shown, window, cx);
            Response::Chat {
                reply: Reply::View {
                    pane_id: pane,
                    shown,
                },
            }
        }),
    }
}

/// The caller's own pane, as chats key it: its Herdr pane, or its
/// herdr-core terminal as `core:t7`, so a caller in one moves its own
/// terminal's agent rather than Herdr 1's focused pane.
fn callers_pane(caller: &crate::control::protocol::Caller) -> Option<String> {
    crate::control::origin(caller.pane_id.as_deref(), caller.terminal_id.as_deref())
        .map(std::borrow::Cow::into_owned)
}

/// How a chat that did not start is reported to the caller.
fn start_code(error: &crate::chat::StartError) -> ErrorCode {
    use crate::chat::StartError;
    match error {
        StartError::AgentMissing(_)
        | StartError::Disabled
        | StartError::NotLocal
        | StartError::ProgramMissing => ErrorCode::Unsupported,
        StartError::NoWorkspace => ErrorCode::NotFound,
        StartError::SlotBusy | StartError::ProgramUnknown => ErrorCode::Busy,
        StartError::NotConnected
        | StartError::Request(_)
        | StartError::Response
        | StartError::Timeout => ErrorCode::Failed,
    }
}

/// How an agent that did not move is reported to the caller.
fn convert_code(error: &crate::chat::ConvertError) -> ErrorCode {
    use crate::chat::ConvertError;
    use sheprd_client::herdr::convert;
    match error {
        ConvertError::Program(error) => start_code(error),
        ConvertError::Convert(
            convert::Error::Line
            | convert::Error::NoCoreSession(_)
            | convert::Error::NoSession(_)
            | convert::Error::NoShell(_)
            | convert::Error::Unknown(_)
            | convert::Error::Unsupported(_),
        )
        | ConvertError::Disabled
        | ConvertError::HostProgram(_)
        | ConvertError::NoHome
        | ConvertError::NotReachable => ErrorCode::Unsupported,
        ConvertError::Convert(
            convert::Error::AlreadyChat
            | convert::Error::FolderGone(_)
            | convert::Error::NoAgent
            | convert::Error::NotRunning(_),
        ) => ErrorCode::NotFound,
        ConvertError::AlreadyConverting | ConvertError::Convert(convert::Error::Busy(_)) => {
            ErrorCode::Busy
        }
        ConvertError::Convert(
            convert::Error::Moved | convert::Error::StillRunning(_) | convert::Error::Stopped,
        )
        | ConvertError::Answer(_)
        | ConvertError::Core(_)
        | ConvertError::Existing(_)
        | ConvertError::NotConnected
        | ConvertError::Stale
        | ConvertError::Type(_)
        | ConvertError::Worker(_) => ErrorCode::Failed,
    }
}

mod info {
    use crate::{
        SheprdWindow,
        control::protocol::chat::{ChatInfo, InputMode},
    };
    use sheprd_chat_protocol::Status;

    pub(super) fn infos(view: &SheprdWindow) -> Vec<ChatInfo> {
        view.chats
            .panes()
            .map(|chat| {
                let transcript = chat.session.transcript();
                let mut status = match transcript.status {
                    Status::Starting => "starting",
                    Status::Idle => "idle",
                    Status::Working => "working",
                    Status::Failed { .. } => "failed",
                    Status::Exited { .. } => "exited",
                };
                if chat.target.existing.is_some()
                    && view.live.snapshot.as_ref().is_some_and(|snapshot| {
                        snapshot.agents.iter().any(|agent| {
                            agent.pane_id == chat.target.pane_id
                                && agent.agent_status
                                    == sheprd_client::protocol::AgentStatus::Blocked
                        })
                    })
                {
                    status = "blocked";
                }
                ChatInfo {
                    pane_id: chat.target.pane_id.clone(),
                    agent: chat.session.agent().to_owned(),
                    input_mode: if chat.target.existing.is_some() {
                        InputMode::Terminal
                    } else if transcript.read_only {
                        InputMode::ReadOnly
                    } else {
                        InputMode::Chat
                    },
                    session_id: transcript
                        .session
                        .as_ref()
                        .map(|session| session.id.clone()),
                    status: status.into(),
                    attached: chat.attached(),
                    shown: view.chats.shown(&chat.target.pane_id),
                    permissions: transcript.permissions.len(),
                    queued: transcript.queued.len(),
                    model: transcript
                        .option(sheprd_chat_protocol::OptionCategory::Model)
                        .map(sheprd_chat_protocol::ConfigOption::current_name),
                    context: transcript
                        .usage
                        .as_ref()
                        .filter(|usage| usage.size > 0)
                        .map(sheprd_chat_protocol::Usage::percent),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
