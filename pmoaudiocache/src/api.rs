//! API REST pour le cache audio

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::AudioCache;

/// Liste toutes les pistes audio
pub async fn list_tracks(State(cache): State<Arc<AudioCache>>) -> Response {
    match cache.db.get_all() {
        Ok(tracks) => Json(tracks).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Cannot list tracks").into_response(),
    }
}

/// Requête pour ajouter une piste
#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "pmoserver", derive(utoipa::ToSchema))]
pub struct AddTrackRequest {
    pub url: String,
}

/// Ajoute une piste depuis une URL
pub async fn add_track(
    State(cache): State<Arc<AudioCache>>,
    Json(req): Json<AddTrackRequest>,
) -> Response {
    match cache.add_from_url(&req.url, None).await {
        Ok((pk, _)) => Json(serde_json::json!({
            "pk": pk,
            "status": "added"
        }))
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Cannot add track: {}", e),
        )
            .into_response(),
    }
}

/// Récupère les informations d'une piste
pub async fn get_track_info(
    State(cache): State<Arc<AudioCache>>,
    Path(pk): Path<String>,
) -> Response {
    match cache.get_entry(&pk).await {
        Ok(entry) => Json(entry).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "Track not found").into_response(),
    }
}

/// Récupère les métadonnées d'une piste
pub async fn get_track_metadata(
    State(cache): State<Arc<AudioCache>>,
    Path(pk): Path<String>,
) -> Response {
    match cache.get_metadata(&pk).await {
        Ok(metadata) => Json(metadata).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "Track not found").into_response(),
    }
}

/// Récupère le DIDL-Lite d'une piste
pub async fn get_track_didl(
    State(cache): State<Arc<AudioCache>>,
    Path(pk): Path<String>,
) -> Response {
    let base_url = "http://localhost:8080"; // TODO: from config
    match cache.get_didl(&pk, base_url).await {
        Ok(didl) => (StatusCode::OK, [("content-type", "application/xml")], didl).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "Track not found").into_response(),
    }
}

/// Supprime une piste
pub async fn delete_track(
    State(cache): State<Arc<AudioCache>>,
    Path(pk): Path<String>,
) -> Response {
    match cache.delete(&pk).await {
        Ok(_) => (StatusCode::OK, "Track deleted").into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Cannot delete track: {}", e),
        )
            .into_response(),
    }
}

/// Purge tout le cache
pub async fn purge_cache(State(cache): State<Arc<AudioCache>>) -> Response {
    match cache.purge().await {
        Ok(_) => (StatusCode::OK, "Cache purged").into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Cannot purge cache: {}", e),
        )
            .into_response(),
    }
}

/// Consolide le cache
pub async fn consolidate_cache(State(cache): State<Arc<AudioCache>>) -> Response {
    match cache.consolidate().await {
        Ok(_) => (StatusCode::OK, "Cache consolidated").into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Cannot consolidate cache: {}", e),
        )
            .into_response(),
    }
}
