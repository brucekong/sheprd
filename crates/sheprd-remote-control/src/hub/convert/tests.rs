use super::*;
use crate::hub::{Layout, Workspace, layout_terminal};
use std::path::PathBuf;
#[cfg(unix)]
use std::sync::mpsc;

#[cfg(unix)]
mod folder;

/// A Claude session as Herdr's own integration reports it.
#[cfg(unix)]
const SESSION: &str = "0c6f0c39-3c1e-4b7e-9a51-6d1b8e2d7f10";

/// A layout whose one terminal tab offers its pane `t2-p1` to move, with
/// `agent` in it.
fn layout(agent: Agent) -> Layout {
    let mut terminal = layout_terminal("t2", "zsh");
    terminal.convert = Some(("t2-p1".into(), agent));
    Layout {
        connected: true,
        boot: "b".into(),
        working: 0,
        workspaces: vec![Workspace {
            id: "w1".into(),
            label: "app".into(),
            branch: None,
            cwd: "/code/app".into(),
            worktree: None,
            session: None,
            chats: Vec::new(),
            terminals: vec![terminal],
        }],
    }
}

/// A hub showing `layout`, finding its chat program at `/app/sheprd-acp`,
/// whose API calls go to the socket beside `client_socket`.
fn hub(layout: Layout, client_socket: Option<PathBuf>) -> Arc<Hub> {
    let mut hub = Hub::offline_in(layout, None, client_socket);
    hub.program = Some(PathBuf::from("/app/sheprd-acp"));
    Arc::new(hub)
}

#[cfg(unix)]
fn pane_answer(status: &str) -> Value {
    json!({"type": "pane_info", "pane": {
        "pane_id": "t2-p1", "tab_id": "t2", "cwd": "/shell", "agent": "claude",
        "agent_status": status,
        "agent_session": {
            "agent": "claude", "kind": "id", "source": "herdr:claude", "value": SESSION,
        },
    }})
}

#[cfg(unix)]
fn processes(front: u32, cwd: &str) -> Value {
    json!({"type": "pane_process_info", "process_info": {
        "pane_id": "t2-p1", "shell_pid": 10, "foreground_process_group_id": front,
        "foreground_processes": [
            {"pid": 20, "argv": ["claude", "--model", "opus"], "cwd": cwd},
        ],
    }})
}

/// Serves Herdr's API in `dir`: the pane reports `status`, and the agent
/// leaves once a Ctrl+C arrives. Sends each request's method and params.
#[cfg(unix)]
fn serve(dir: &std::path::Path, status: &'static str) -> mpsc::Receiver<(String, Value)> {
    serve_in(dir, status, "/tmp")
}

/// As [`serve`], with the agent running in `cwd`.
#[cfg(unix)]
fn serve_in(
    dir: &std::path::Path,
    status: &'static str,
    cwd: &'static str,
) -> mpsc::Receiver<(String, Value)> {
    use std::{
        io::{BufRead, BufReader, Write},
        os::unix::net::UnixListener,
    };
    let listener = UnixListener::bind(dir.join("h.sock")).unwrap();
    let (sender, requests) = mpsc::channel();
    std::thread::spawn(move || {
        let mut stopped = false;
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            let method = request["method"].as_str().unwrap().to_owned();
            let result = match method.as_str() {
                "pane.get" => pane_answer(status),
                "pane.process_info" => processes(if stopped { 10 } else { 20 }, cwd),
                "pane.send_keys" => {
                    stopped = true;
                    json!({})
                }
                _ => json!({}),
            };
            // Recorded before the answer, so a caller that has its answer
            // finds the request here.
            let _ = sender.send((method, request["params"].clone()));
            let answer = json!({"id": "remote", "result": result}).to_string();
            stream.write_all(format!("{answer}\n").as_bytes()).unwrap();
        }
    });
    requests
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn an_idle_agent_stops_on_ctrl_c_and_continues_as_a_chat() {
    let dir = tempfile::tempdir().unwrap();
    let requests = serve(dir.path(), "idle");
    let hub = hub(
        layout(Agent::Claude),
        Some(dir.path().join("h-client.sock")),
    );
    hub.convert_to_chat("t2-p1", Agent::Claude).await.unwrap();
    let seen: Vec<_> = requests.try_iter().collect();
    let methods: Vec<_> = seen.iter().map(|(method, _)| method.as_str()).collect();
    assert_eq!(
        methods,
        [
            "pane.get",
            "pane.process_info",
            "pane.process_info",
            "pane.send_keys",
            "pane.process_info",
            "pane.send_input",
        ]
    );
    assert_eq!(seen[3].1["keys"], json!(["ctrl+c"]));
    let typed = &seen[5].1;
    assert_eq!(typed["keys"], json!(["Enter"]));
    assert_eq!(
        typed["text"],
        format!(
            " sh -c 'exec \"$0\" \"$@\"' '/app/sheprd-acp' '--resume' '{SESSION}' \
             '--cwd' '/tmp' 'claude' '--' '--model' 'opus'"
        )
    );
    assert!(hub.moving.claim("t2-p1").is_ok(), "the claim is released");
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_busy_agent_is_left_running() {
    let dir = tempfile::tempdir().unwrap();
    let requests = serve(dir.path(), "working");
    let hub = hub(
        layout(Agent::Claude),
        Some(dir.path().join("h-client.sock")),
    );
    assert!(matches!(
        hub.convert_to_chat("t2-p1", Agent::Claude).await,
        Err(Error::Refused(convert::Error::Busy(Agent::Claude)))
    ));
    let methods: Vec<_> = requests.try_iter().map(|(method, _)| method).collect();
    assert_eq!(methods, ["pane.get"]);
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn another_agent_in_the_pane_is_not_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let requests = serve(dir.path(), "idle");
    let hub = hub(layout(Agent::Codex), Some(dir.path().join("h-client.sock")));
    assert!(matches!(
        hub.convert_to_chat("t2-p1", Agent::Codex).await,
        Err(Error::Refused(convert::Error::Moved))
    ));
    let methods: Vec<_> = requests.try_iter().map(|(method, _)| method).collect();
    assert_eq!(methods, ["pane.get"]);
}

#[tokio::test]
async fn a_pane_its_tab_does_not_offer_is_refused_before_herdr_is_asked() {
    let hub = hub(layout(Agent::Claude), None);
    for pane in ["t2-p2", "nope"] {
        let refused = hub.convert_to_chat(pane, Agent::Claude).await;
        // Windows refuses every move first, as its typed line needs `sh`.
        if cfg!(windows) {
            assert!(matches!(refused, Err(Error::Platform)));
        } else {
            assert!(matches!(
                refused,
                Err(Error::Refused(convert::Error::NoAgent))
            ));
        }
    }
}

#[tokio::test]
async fn an_agent_replaced_since_the_phone_saw_it_is_refused_before_herdr_is_asked() {
    // The phone was shown Claude; the pane now holds Codex. No socket
    // answers, so asking Herdr anything would fail differently.
    let hub = hub(layout(Agent::Codex), None);
    let refused = hub.convert_to_chat("t2-p1", Agent::Claude).await;
    if cfg!(windows) {
        assert!(matches!(refused, Err(Error::Platform)));
    } else {
        assert!(matches!(
            refused,
            Err(Error::Refused(convert::Error::Moved))
        ));
    }
}

#[tokio::test]
async fn without_the_chat_program_nothing_is_touched() {
    let hub = Arc::new(Hub::offline_in(layout(Agent::Claude), None, None));
    let refused = hub.convert_to_chat("t2-p1", Agent::Claude).await;
    if cfg!(windows) {
        assert!(matches!(refused, Err(Error::Platform)));
    } else {
        assert!(matches!(
            refused,
            Err(Error::Hub(error)) if matches!(*error, crate::hub::Error::ProgramMissing)
        ));
    }
}

#[test]
fn one_move_at_a_time_per_pane() {
    let moving = Moving::default();
    let claim = moving.claim("p").unwrap();
    assert!(matches!(moving.claim("p"), Err(Error::AlreadyConverting)));
    assert!(moving.claim("q").is_ok());
    let cancelled = claim.cancelled.clone();
    drop(claim);
    assert!(
        cancelled.load(Ordering::Acquire),
        "dropping cancels the stop"
    );
    assert!(moving.claim("p").is_ok());
}
