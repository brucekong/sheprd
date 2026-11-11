//! Moving the agent in a Herdr 2 terminal into a chat from a phone, with a
//! stand-in core and the session record Claude Code keeps of itself.

use super::*;
use crate::hub::{
    Workspace,
    convert::{Error as ConvertError, Result as ConvertResult},
};
use sheprd_chat_protocol::Agent;
use std::{path::PathBuf, sync::atomic::AtomicBool};

const SESSION: &str = "0c6f0c39-3c1e-4b7e-9a51-6d1b8e2d7f10";
const PID: i32 = 4242;

/// `t1` running Claude Code until Ctrl+C reaches it, then its shell.
fn agent_record(stopped: bool, status: &str) -> Value {
    let (name, argv) = if stopped {
        ("zsh", json!(["-zsh"]))
    } else {
        ("claude", json!(["claude", "--model", "opus"]))
    };
    json!({"address": "t1", "cols": 80, "rows": 24, "cwd": "/tmp", "state": "live",
        "pid": 100, "status": {"state": status, "app": "claude"},
        "foreground": {"pid": if stopped { 100 } else { PID }, "name": name,
            "argv": argv, "cwd": "/tmp"}})
}

struct Stand {
    _dir: tempfile::TempDir,
    home: PathBuf,
    source: Arc<Source>,
    /// What reached the core: `terminal.keys` and `terminal.send` params.
    asked: mpsc::Receiver<(String, Value)>,
    server: thread::JoinHandle<()>,
}

fn stand(status: &'static str) -> Stand {
    let dir = tempfile::Builder::new()
        .prefix("rv")
        .tempdir_in("/tmp")
        .unwrap();
    let home = dir.path().join("home");
    let sessions = home.join(".claude").join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::write(
        sessions.join(format!("{PID}.json")),
        format!(r#"{{"pid":{PID},"sessionId":"{SESSION}","cwd":"/tmp"}}"#),
    )
    .unwrap();
    let path = dir.path().join("core.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let (sent, asked) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut hello = String::new();
        BufReader::new(&mut socket).read_line(&mut hello).unwrap();
        let hello: Value = serde_json::from_str(&hello).unwrap();
        let response = json!({"jsonrpc": "2.0", "id": hello["id"], "result": {
            "generation": 1, "version": "0.1.18", "framed": true,
            "core": {"id": "fake-core", "name": "test"},
            "methods": Method::ALL.iter().map(|method| method.as_str()).collect::<Vec<_>>()}});
        writeln!(socket, "{response}").unwrap();
        let mut decoder = Decoder::default();
        let mut stopped = false;
        while let Some(frame) = receive(&mut socket, &mut decoder) {
            if frame.kind != Kind::Data || frame.channel != 0 {
                continue;
            }
            let request: Value = serde_json::from_slice(&frame.payload).unwrap();
            let method = request["method"].as_str().unwrap().to_owned();
            let result = match method.as_str() {
                "terminal.list" => json!({"terminals": [agent_record(stopped, status)]}),
                "terminal.keys" => {
                    stopped = true;
                    json!({})
                }
                "terminal.send" => json!({"turn": 1}),
                // The labelled terminal, as herdr-core answers.
                "terminal.label" => agent_record(stopped, status),
                method => panic!("unexpected method {method}"),
            };
            if method != "terminal.list" {
                sent.send((method, request["params"].clone())).unwrap();
            }
            let response = json!({"jsonrpc": "2.0", "id": request["id"], "result": result});
            socket
                .write_all(&encode(0, Kind::Data, &serde_json::to_vec(&response).unwrap()).unwrap())
                .unwrap();
        }
    });
    let core = Core::connect(&Endpoint {
        socket: path,
        session: "test".into(),
    })
    .unwrap();
    let source = Arc::new(Source {
        connected: AtomicBool::new(true),
        endpoint: Default::default(),
        holds: AtomicU64::new(0),
        own: Mutex::default(),
        poller: Mutex::default(),
        scope: None,
        state: Mutex::new(State {
            core: Some(Arc::new(core)),
            terminals: vec![serde_json::from_value(agent_record(false, status)).unwrap()],
            ..State::default()
        }),
        tabs: None,
    });
    Stand {
        _dir: dir,
        home,
        source,
        asked,
        server,
    }
}

/// A hub listing `t1` as the core projects it, with its move offered, and
/// agents' records read from `home`.
fn hub_on(stand: &Stand, home: Option<PathBuf>) -> Arc<Hub> {
    let record = serde_json::from_value(agent_record(false, "idle")).unwrap();
    let terminal = projection::terminal("fake-core", &id("t1"), "", &[&record], |_| 0).unwrap();
    let layout = Layout {
        boot: "core:fake-core".into(),
        connected: true,
        working: 0,
        workspaces: vec![Workspace {
            branch: None,
            chats: Vec::new(),
            cwd: "/tmp".into(),
            id: UNLISTED.into(),
            label: "Herdr 2".into(),
            session: None,
            terminals: vec![terminal],
            worktree: None,
        }],
    };
    let mut hub = Hub::offline_in(layout, None, None);
    hub.core = Some(stand.source.clone());
    hub.program = Some(PathBuf::from("/opt/sheprd/sheprd-acp"));
    hub.home = home;
    Arc::new(hub)
}

#[test]
fn a_terminal_running_a_movable_agent_offers_it() {
    let record = serde_json::from_value(agent_record(false, "working")).unwrap();
    let terminal = projection::terminal("fake-core", &id("t1"), "", &[&record], |_| 0).unwrap();
    // Offered however busy: the move itself waits for the turn to end.
    assert_eq!(terminal.convert, Some((id("t1"), Agent::Claude)));
    let shell = serde_json::from_value(record_of_shell()).unwrap();
    let terminal = projection::terminal("fake-core", &id("t1"), "", &[&shell], |_| 0).unwrap();
    assert_eq!(terminal.convert, None);
}

fn record_of_shell() -> Value {
    json!({"address": "t1", "cols": 80, "rows": 24, "cwd": "/tmp", "state": "live",
        "foreground": {"pid": 100, "name": "zsh", "argv": ["-zsh"], "cwd": "/tmp"}})
}

#[test]
fn an_idle_agent_stops_on_ctrl_c_and_continues_as_a_chat() {
    let stand = stand("idle");
    let cancelled = AtomicBool::new(false);
    let program = std::path::Path::new("/opt/sheprd/sheprd-acp");
    stand
        .source
        .convert(&id("t1"), Agent::Claude, program, &stand.home, &cancelled)
        .unwrap();
    let (method, params) = stand.asked.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(method, "terminal.keys");
    assert_eq!(params, json!({"terminal": "t1", "keys": ["ctrl+c"]}));
    let (method, params) = stand.asked.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(method, "terminal.send");
    let line = params["text"].as_str().unwrap();
    assert!(line.contains("/opt/sheprd/sheprd-acp"), "{line}");
    assert!(line.contains(SESSION), "{line}");
    // The terminal is then labelled with the chat the line started, so the
    // daemon lists it as a chat.
    let (method, params) = stand.asked.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(method, "terminal.label");
    let endpoint = params["labels"][sheprd_chat_protocol::TOKEN]
        .as_str()
        .unwrap();
    assert!(line.contains(endpoint), "{line}");
    let Stand { source, server, .. } = stand;
    drop(source);
    server.join().unwrap();
}

#[test]
fn a_busy_agent_is_left_running() {
    let stand = stand("working");
    let refused: ConvertResult<()> = stand.source.convert(
        &id("t1"),
        Agent::Claude,
        std::path::Path::new("/opt/sheprd/sheprd-acp"),
        &stand.home,
        &AtomicBool::new(false),
    );
    assert!(matches!(
        refused,
        Err(ConvertError::Refused(
            sheprd_client::herdr::convert::Error::Busy(Agent::Claude)
        ))
    ));
    assert!(
        stand.asked.try_recv().is_err(),
        "nothing was pressed or typed"
    );
    let Stand { source, server, .. } = stand;
    drop(source);
    server.join().unwrap();
}

#[tokio::test]
async fn a_phone_moves_the_agent_its_herdr_2_tab_offers() {
    let stand = stand("idle");
    let hub = hub_on(&stand, Some(stand.home.clone()));
    // The phone names an agent the tab does not run: refused, untouched.
    assert!(matches!(
        hub.convert_to_chat(&id("t1"), Agent::Codex).await,
        Err(ConvertError::Refused(
            sheprd_client::herdr::convert::Error::Moved
        ))
    ));
    assert!(stand.asked.try_recv().is_err());
    hub.convert_to_chat(&id("t1"), Agent::Claude).await.unwrap();
    let asked: Vec<String> = stand.asked.try_iter().map(|(method, _)| method).collect();
    assert_eq!(asked, ["terminal.keys", "terminal.send", "terminal.label"]);
    drop(hub);
    let Stand { source, server, .. } = stand;
    drop(source);
    server.join().unwrap();
}

#[tokio::test]
async fn an_agent_whose_session_is_unknown_is_left_running() {
    let stand = stand("idle");
    let empty = tempfile::tempdir().unwrap();
    let hub = hub_on(&stand, Some(empty.path().to_owned()));
    assert!(matches!(
        hub.convert_to_chat(&id("t1"), Agent::Claude).await,
        Err(ConvertError::Refused(
            sheprd_client::herdr::convert::Error::NoCoreSession(Agent::Claude)
        ))
    ));
    assert!(
        stand.asked.try_recv().is_err(),
        "nothing was pressed or typed"
    );
    drop(hub);
    let Stand { source, server, .. } = stand;
    drop(source);
    server.join().unwrap();
}

#[test]
fn another_machines_terminal_offers_no_move() {
    let mut remote = agent_record(false, "idle");
    remote["address"] = json!("box/t7");
    let record = serde_json::from_value(remote).unwrap();
    let terminal = projection::terminal("fake-core", &id("box/t7"), "", &[&record], |_| 0).unwrap();
    assert_eq!(terminal.convert, None);
}

#[test]
fn a_moved_agents_terminal_is_a_chat_only_while_the_chat_runs() {
    let endpoint = sheprd_chat_protocol::EndpointId::random().to_string();
    let labelled = |front: &str| -> Terminal {
        serde_json::from_value(json!({"address": "t1", "cols": 80, "rows": 24,
            "cwd": "/tmp", "state": "live",
            "labels": {sheprd_chat_protocol::TOKEN: endpoint},
            "foreground": {"pid": 7, "name": front, "argv": [front], "cwd": "/tmp"}}))
        .unwrap()
    };
    for (front, chat) in [("sheprd-acp", true), ("zsh", false)] {
        let records = [labelled(front)];
        let mut layout = projection::project("fake-core", &records, None, None);
        chats::lift(&mut layout, "fake-core", &records);
        let workspace = &layout.workspaces[0];
        assert_eq!(workspace.chats.len(), usize::from(chat), "{front} in front");
        assert_eq!(
            workspace.terminals.len(),
            usize::from(!chat),
            "{front} in front"
        );
    }
}
