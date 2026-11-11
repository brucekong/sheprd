//! This computer's tags (`control::tags`) as the daemon holds them, and
//! `GET`/`PUT /api/v2/tags`, a phone's way to read and edit them. Home carries
//! them too, so a change reaches every phone connected.

use super::{ApiError, Device, Shared, kept::Kept, now_ms};
use crate::control::tags::ComputerTags;
use axum::{Json, extract::State};

pub(crate) type Tags = Kept<ComputerTags>;

pub(super) async fn get(State(state): State<Shared>, _device: Device) -> Json<ComputerTags> {
    Json(state.tags.current())
}

pub(super) async fn put(
    State(state): State<Shared>,
    _device: Device,
    Json(offer): Json<ComputerTags>,
) -> Result<Json<ComputerTags>, ApiError> {
    let tags = tokio::task::spawn_blocking(move || state.tags.offer(offer, now_ms()))
        .await
        .map_err(|_| ApiError::Invalid("the request was interrupted"))??;
    Ok(Json(tags))
}
