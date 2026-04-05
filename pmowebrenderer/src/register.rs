//! Handlers HTTP pour l'enregistrement/désenregistrement des instances WebRenderer.
//!
//! - POST /api/webrenderer/register  → crée ou reconnecte une instance
//! - DELETE /api/webrenderer/{id}    → désenregistrement explicite

use axum::{
    extract::{Path, State},
    http::{StatusCode, header::HeaderMap},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::messages::PlaybackState;
use crate::registry::RendererRegistry;

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub instance_id: String,
    pub user_agent: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub stream_url: String,
    pub udn: String,
    /// true si le backend est déjà en lecture — le frontend doit démarrer immédiatement
    pub should_play: bool,
}

/// POST /api/webrenderer/register
#[axum::debug_handler]
pub async fn register_handler(
    State(registry): State<Arc<RendererRegistry>>,
    Json(req): Json<RegisterRequest>,
) -> impl IntoResponse {
    tracing::info!(
        instance_id = %req.instance_id,
        user_agent = %req.user_agent,
        "WebRenderer: register request"
    );

    match registry
        .register_or_reconnect(&req.instance_id, &req.user_agent)
        .await
    {
        Ok((stream_url, udn, should_play)) => {
            tracing::info!(
                instance_id = %req.instance_id,
                stream_url = %stream_url,
                udn = %udn,
                should_play = %should_play,
                "WebRenderer: registered"
            );
            (StatusCode::OK, Json(RegisterResponse { stream_url, udn, should_play })).into_response()
        }
        Err(e) => {
            tracing::error!(
                instance_id = %req.instance_id,
                error = %e,
                "WebRenderer: registration failed"
            );
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PositionUpdateRequest {
    pub _position_sec: f64,
    pub duration_sec: Option<f64>,
}

/// POST /api/webrenderer/{id}/position
/// position_sec est ignoré (géré par PlayerEvent::Position côté serveur).
/// duration_sec est utilisé comme fallback si la source ne connaît pas la durée (flux radio).
#[axum::debug_handler]
pub async fn position_update_handler(
    State(registry): State<Arc<RendererRegistry>>,
    Path(instance_id): Path<String>,
    Json(req): Json<PositionUpdateRequest>,
) -> impl IntoResponse {
    registry.update_duration(&instance_id, req.duration_sec);
    StatusCode::NO_CONTENT
}

/// DELETE /api/webrenderer/{id}
#[axum::debug_handler]
pub async fn unregister_handler(
    State(registry): State<Arc<RendererRegistry>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    tracing::info!(instance_id = %instance_id, "WebRenderer: explicit unregister");
    registry.schedule_unregister(&instance_id);
    StatusCode::NO_CONTENT
}

#[derive(Debug, Deserialize)]
pub struct UriRequest {
    pub uri: String,
}

/// POST /api/webrenderer/{id}/set_uri - charge une URI et joue
#[axum::debug_handler]
pub async fn set_uri_handler(
    State(registry): State<Arc<RendererRegistry>>,
    Path(instance_id): Path<String>,
    Json(req): Json<UriRequest>,
) -> impl IntoResponse {
    tracing::info!(instance_id = %instance_id, uri = %req.uri, "WebRenderer: set_uri request");
    registry.load_uri(&instance_id, req.uri).await;
    StatusCode::OK
}

/// POST /api/webrenderer/{id}/pause
#[axum::debug_handler]
pub async fn pause_handler(
    State(registry): State<Arc<RendererRegistry>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    tracing::info!(instance_id = %instance_id, "WebRenderer: pause request");
    registry.send_pause_command(&instance_id).await;
    StatusCode::OK
}

// ─── Rapports du player ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct PlayerStateReport {
    pub position_sec: Option<f64>,
    pub duration_sec: Option<f64>,
    pub state: Option<String>,
    pub ready_state: Option<String>,
}

/// POST /api/webrenderer/{id}/report - recoit rapports position/state du player
#[axum::debug_handler]
pub async fn report_handler(
    State(registry): State<Arc<RendererRegistry>>,
    Path(instance_id): Path<String>,
    Json(report): Json<PlayerStateReport>,
) -> impl IntoResponse {
    // Registry met à jour l'état avec les rapports du player
    registry.update_player_state(&instance_id, report).await;
    StatusCode::OK
}

// ─── Commandes vers le player ─────────────────────────────────

/// GET /api/webrenderer/{id}/command - recupere commande pending pour le player
#[axum::debug_handler]
pub async fn command_handler(
    State(registry): State<Arc<RendererRegistry>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    match registry.get_pending_command(&instance_id).await {
        Some(cmd) => (StatusCode::OK, Json(cmd)).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

/// POST /api/webrenderer/{id}/play - tell player to stream and play
#[axum::debug_handler]
pub async fn play_handler(
    State(registry): State<Arc<RendererRegistry>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    // Check if there's a valid URI loaded - if not, ignore the play command
    if !registry.has_current_uri(&instance_id) {
        tracing::warn!(instance_id = %instance_id, "Play command ignored: no URI loaded");
        let mut headers = HeaderMap::new();
        headers.insert(axum::http::header::CONTENT_TYPE, "text/plain".parse().unwrap());
        return (StatusCode::BAD_REQUEST, headers, "No URI loaded").into_response();
    }
    
    tracing::info!(instance_id = %instance_id, "WebRenderer: play request");
    
    // Get stream URL and tell player to play it
    let stream_url = format!("/api/webrenderer/{}/stream", instance_id);
    
    // Set command for player to start streaming
    let command = serde_json::json!({
        "type": "stream",
        "url": stream_url
    });
    registry.set_player_command(&instance_id, command);
    
    // Also tell pipeline to play (if not already) - use existing method
    registry.send_play_command(&instance_id).await;
    
    (StatusCode::OK, "OK").into_response()
}

// ─── Metadata endpoints ─────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct NowPlayingResponse {
    pub state: String,
    pub current_uri: Option<String>,
    pub current_metadata: Option<String>,
    pub position: Option<String>,
    pub duration: Option<String>,
    pub volume: u16,
    pub mute: bool,
}

#[axum::debug_handler]
pub async fn nowplaying_handler(
    State(registry): State<Arc<RendererRegistry>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let state = match registry.get_state(&instance_id) {
        Some(s) => s,
        None => return StatusCode::NOT_FOUND.into_response(),
    };
    let s = state.read();
    let response = NowPlayingResponse {
        state: match s.playback_state {
            PlaybackState::Playing => "PLAYING",
            PlaybackState::Paused => "PAUSED",
            PlaybackState::Stopped => "STOPPED",
            PlaybackState::Transitioning => "TRANSITIONING",
        }.to_string(),
        current_uri: s.current_uri.clone(),
        current_metadata: s.current_metadata.clone(),
        position: s.position.clone(),
        duration: s.duration.clone(),
        volume: s.volume,
        mute: s.mute,
    };
    (StatusCode::OK, Json(response)).into_response()
}

#[derive(Debug, Serialize)]
pub struct RendererStateResponse {
    pub instance_id: String,
    pub udn: String,
    pub playback_state: String,
    pub current_uri: Option<String>,
    pub current_metadata: Option<String>,
    pub next_uri: Option<String>,
    pub next_metadata: Option<String>,
    pub position: Option<String>,
    pub duration: Option<String>,
    pub volume: u16,
    pub mute: bool,
}

#[axum::debug_handler]
pub async fn state_handler(
    State(registry): State<Arc<RendererRegistry>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let (state, udn) = match registry.get_state_and_udn(&instance_id) {
        Some((s, u)) => (s, u),
        None => return StatusCode::NOT_FOUND.into_response(),
    };
    let s = state.read();
    let response = RendererStateResponse {
        instance_id: instance_id.clone(),
        udn,
        playback_state: match s.playback_state {
            PlaybackState::Playing => "PLAYING",
            PlaybackState::Paused => "PAUSED",
            PlaybackState::Stopped => "STOPPED",
            PlaybackState::Transitioning => "TRANSITIONING",
        }.to_string(),
        current_uri: s.current_uri.clone(),
        current_metadata: s.current_metadata.clone(),
        next_uri: s.next_uri.clone(),
        next_metadata: s.next_metadata.clone(),
        position: s.position.clone(),
        duration: s.duration.clone(),
        volume: s.volume,
        mute: s.mute,
    };
    (StatusCode::OK, Json(response)).into_response()
}
