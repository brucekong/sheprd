//! This computer's text shortcuts (`control::shortcuts`) as the daemon holds
//! them, and `GET`/`PUT /api/v2/shortcuts`, a phone's way to read and edit
//! them. Home carries them too, so a change reaches every phone connected.

use super::{ApiError, Device, Shared, kept::Kept, now_ms};
use crate::control::shortcuts::TextShortcuts;
use axum::{Json, extract::State};

pub(crate) type Shortcuts = Kept<TextShortcuts>;

pub(super) async fn get(State(state): State<Shared>, _device: Device) -> Json<TextShortcuts> {
    Json(state.shortcuts.current())
}

pub(super) async fn put(
    State(state): State<Shared>,
    _device: Device,
    Json(offer): Json<TextShortcuts>,
) -> Result<Json<TextShortcuts>, ApiError> {
    let shortcuts = tokio::task::spawn_blocking(move || state.shortcuts.offer(offer, now_ms()))
        .await
        .map_err(|_| ApiError::Invalid("the request was interrupted"))??;
    Ok(Json(shortcuts))
}
