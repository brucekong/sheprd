//! `/api/v2/shortcuts`: a phone reads and edits the computer's text
//! shortcuts, which Home announces by their edit time.

use super::*;
use crate::control::shortcuts::TextShortcuts;
use serde_json::{Value, json};

fn with_bearer(token: &str, mut request: HttpRequest<Body>) -> HttpRequest<Body> {
    request.headers_mut().insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
    );
    request
}

fn put(body: &Value) -> HttpRequest<Body> {
    HttpRequest::put("/api/v2/shortcuts")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, ORIGIN)
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn shortcuts_need_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    assert_eq!(
        send(&state, get("/api/v2/shortcuts")).await.0,
        StatusCode::UNAUTHORIZED
    );
    let offer =
        json!({"shortcuts": [{"id": "a", "title": "Go", "text": "Carry on."}], "updated_ms": 1});
    assert_eq!(send(&state, put(&offer)).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(state.shortcuts.current(), TextShortcuts::default());
}

#[tokio::test]
async fn a_phone_edits_the_shortcuts_and_the_latest_edit_stands() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (_, token) = signed_in(&state, 1);
    let announced = state.shortcuts.subscribe();

    let offer = json!({"shortcuts": [
        {"id": "a", "title": " Carry  on ", "text": "Carry on with the plan."},
        {"id": "b", "title": "Empty", "text": "  "},
    ], "updated_ms": 10});
    let (status, _, body) = send(&state, with_bearer(&token, put(&offer))).await;
    assert_eq!(status, StatusCode::OK);
    let stood: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        stood,
        json!({"shortcuts": [{"id": "a", "title": "Carry on", "text": "Carry on with the plan."}], "updated_ms": 10})
    );
    assert!(
        announced.has_changed().unwrap(),
        "live homes hear of the edit"
    );

    let stale = json!({"shortcuts": [], "updated_ms": 5});
    let (_, _, body) = send(&state, with_bearer(&token, put(&stale))).await;
    let stood: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(stood["updated_ms"], json!(10), "an older edit loses");

    let (_, _, body) = send(&state, with_bearer(&token, get("/api/v2/shortcuts"))).await;
    let read: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(read["shortcuts"][0]["id"], json!("a"));
    let saved: TextShortcuts = crate::app::edited::load(&crate::app::shortcuts::file(dir.path()));
    assert_eq!(
        saved.updated_ms, 10,
        "an edit is on disk before it is answered"
    );
}

#[tokio::test]
async fn a_full_list_is_not_refused_for_its_size() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (_, token) = signed_in(&state, 1);
    let text = "é".repeat(crate::control::shortcuts::MAX_TEXT_CHARS);
    let shortcuts: Vec<Value> = (0..crate::control::shortcuts::MAX_SHORTCUTS)
        .map(|index| json!({"id": format!("s{index}"), "title": "t", "text": text}))
        .collect();
    let offer = json!({"shortcuts": shortcuts, "updated_ms": 3});
    assert!(
        offer.to_string().len() > 320 * 1024,
        "larger than other requests may be"
    );
    let (status, _, _) = send(&state, with_bearer(&token, put(&offer))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        state.shortcuts.current().shortcuts.len(),
        crate::control::shortcuts::MAX_SHORTCUTS
    );
}
