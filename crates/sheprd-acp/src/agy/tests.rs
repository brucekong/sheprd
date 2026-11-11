#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

const SAMPLE: &str = r#"{
  "status": "SUCCESS",
  "command": {
    "name": "usage",
    "data": {
      "groups": [
        {
          "name": "Gemini Models",
          "buckets": [
            {
              "id": "gemini-weekly",
              "name": "Weekly Limit Remaining",
              "remaining_fraction": 0.64,
              "reset_time": "2026-11-14T23:52:39Z"
            },
            {
              "id": "gemini-5h",
              "name": "Five Hour Limit Remaining",
              "remaining_fraction": 1.0,
              "reset_time": "2026-11-10T17:49:13Z"
            }
          ]
        },
        {
          "name": "Gemini 4 Argon",
          "buckets": [
            {
              "id": "gemini-argon-wtu-weekly",
              "name": "Gemini 4 Argon Weekly Limit",
              "remaining_fraction": 0.77,
              "reset_time": "2026-11-15T02:07:17Z"
            }
          ]
        }
      ]
    }
  }
}"#;

#[test]
fn injects_usage_into_available_commands() {
    let mut bridge = Bridge::new(|_| Ok(SAMPLE.to_owned()));
    let outs = bridge.on_harness(&json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "available_commands_update",
                "availableCommands": [
                    {"name": "goal", "description": "Run until finished"}
                ]
            }
        }
    }));
    assert_eq!(outs.len(), 1);
    let Out::Session(Inbound::Notification { params, .. }) = &outs[0] else {
        panic!("expected notification");
    };
    let commands = params
        .pointer("/update/availableCommands")
        .and_then(Value::as_array)
        .unwrap();
    assert_eq!(commands[0]["name"], "usage");
    assert_eq!(commands[1]["name"], "goal");
}

#[test]
fn intercepts_usage_prompt_and_formats_report() {
    let mut bridge = Bridge::new(|_| Ok(SAMPLE.to_owned()));
    let outs = bridge.on_session(Inbound::Request {
        id: wire::RequestId::Number(7),
        method: "session/prompt".to_owned(),
        params: json!({
            "sessionId": "s1",
            "prompt": [{"type": "text", "text": "/usage"}]
        }),
    });
    assert_eq!(outs.len(), 2);
    let Out::Session(Inbound::Notification { params, .. }) = &outs[0] else {
        panic!("expected update notification");
    };
    let text = params
        .pointer("/update/content/text")
        .and_then(Value::as_str)
        .unwrap();
    assert!(text.contains("**Gemini Models**"));
    assert!(text.contains("**Weekly Limit Remaining:** `64%` remaining"));
    assert!(text.contains("**Gemini 4 Argon**"));
    assert!(matches!(
        &outs[1],
        Out::Session(Inbound::Response {
            id: wire::RequestId::Number(7),
            result: Ok(_)
        })
    ));
}

#[test]
fn drops_echoed_user_message_chunks_outside_session_load() {
    let mut bridge = Bridge::new(|_| Ok(SAMPLE.to_owned()));
    let user_chunk = json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "user_message_chunk",
                "content": {"type": "text", "text": "hi"}
            }
        }
    });
    assert!(bridge.on_harness(&user_chunk).is_empty());

    let _ = bridge.on_session(Inbound::Request {
        id: wire::RequestId::Number(2),
        method: "session/load".to_owned(),
        params: json!({"sessionId": "s1", "cwd": "/tmp"}),
    });
    assert_eq!(bridge.on_harness(&user_chunk).len(), 1);

    let _ = bridge.on_harness(&json!({
        "jsonrpc": "2.0",
        "id": 2,
        "result": {}
    }));
    assert!(bridge.on_harness(&user_chunk).is_empty());
}

#[test]
fn session_cancel_stops_in_flight_usage_check() {
    let cancelled = Arc::new(AtomicBool::new(false));
    let seen_cancel = Arc::clone(&cancelled);
    let mut bridge = Bridge::new(move |cancel| {
        while !cancel.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(5));
        }
        seen_cancel.store(true, Ordering::Relaxed);
        Err(UsageError::TimedOut)
    });

    let outs = bridge.on_session(Inbound::Request {
        id: wire::RequestId::Number(9),
        method: "session/prompt".to_owned(),
        params: json!({
            "sessionId": "s1",
            "prompt": [{"type": "text", "text": "/usage"}]
        }),
    });
    assert!(outs.is_empty());
    assert!(bridge.next_tick().is_some());

    let cancel_outs = bridge.on_session(Inbound::Notification {
        method: "session/cancel".to_owned(),
        params: json!({"sessionId": "s1"}),
    });
    assert_eq!(cancel_outs.len(), 1);
    let Out::Session(Inbound::Response {
        id: wire::RequestId::Number(9),
        result: Ok(val),
    }) = &cancel_outs[0]
    else {
        panic!("expected cancelled prompt response");
    };
    assert_eq!(val["stopReason"], "cancelled");
    assert!(bridge.next_tick().is_none());

    for _ in 0..40 {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(cancelled.load(Ordering::Relaxed));
}

#[test]
fn parses_and_checks_minimum_agy_version() {
    assert_eq!(parse_version("agy 1.2.0\n"), Some((1, 2, 0)));
    assert_eq!(parse_version("v1.1.11"), Some((1, 1, 11)));
    assert!(parse_version("1.1.10").unwrap() < MINIMUM_VERSION);
    assert!(parse_version("1.1.11").unwrap() >= MINIMUM_VERSION);
}
