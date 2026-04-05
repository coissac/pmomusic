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

use pmomediarenderer::PlaybackState;
use pmomediarenderer::PipelineControl;
use pmomediarenderer::{DeviceCommand, MediaRendererRegistry};

use crate::adapter::BrowserAdapter;

#[derive(Debug, serde::Deserialize)]
pub struct PlayerStateReport {
    pub position_sec: Option<f64>,
    pub duration_sec: Option<f64>,
    pub state: Option<String>,
    #[allow(dead_code)]
    pub ready_state: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub instance_id: String,
    pub user_agent: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub stream_url: String,
    pub udn: String,
    pub should_play: bool,
}

#[axum::debug_handler]
pub async fn register_handler(
    State(registry): State<Arc<MediaRendererRegistry>>,
    Json(req): Json<RegisterRequest>,
) -> impl IntoResponse {
    tracing::info!(
        instance_id = %req.instance_id,
        user_agent = %req.user_agent,
        "WebRenderer: register request"
    );

    match registry
        .register_or_reconnect(
            &req.instance_id,
            "/api/webrenderer",
            "PMOMusic WebRenderer/2.0",
            |state| Arc::new(BrowserAdapter::new(state)),
        )
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

#[axum::debug_handler]
pub async fn position_update_handler(
    State(registry): State<Arc<MediaRendererRegistry>>,
    Path(instance_id): Path<String>,
    Json(req): Json<PositionUpdateRequest>,
) -> impl IntoResponse {
    registry.update_duration(&instance_id, req.duration_sec);
    StatusCode::NO_CONTENT
}

#[axum::debug_handler]
pub async fn unregister_handler(
    State(registry): State<Arc<MediaRendererRegistry>>,
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

#[axum::debug_handler]
pub async fn set_uri_handler(
    State(registry): State<Arc<MediaRendererRegistry>>,
    Path(instance_id): Path<String>,
    Json(req): Json<UriRequest>,
) -> impl IntoResponse {
    tracing::info!(instance_id = %instance_id, uri = %req.uri, "WebRenderer: set_uri request");
    if let Some(pipeline) = registry.get_pipeline(&instance_id) {
        pipeline.send(PipelineControl::LoadUri(req.uri.clone())).await;
        pipeline.send(PipelineControl::Play).await;
    }
    StatusCode::OK
}

#[axum::debug_handler]
pub async fn pause_handler(
    State(registry): State<Arc<MediaRendererRegistry>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    tracing::info!(instance_id = %instance_id, "WebRenderer: pause request");
    if let Some(instance) = registry.get_instance(instance_id.as_str()) {
        instance.adapter.deliver(DeviceCommand::Pause);
    }
    StatusCode::OK
}

#[axum::debug_handler]
pub async fn report_handler(
    State(registry): State<Arc<MediaRendererRegistry>>,
    Path(instance_id): Path<String>,
    Json(report): Json<PlayerStateReport>,
) -> impl IntoResponse {
    let instance = match registry.get_instance(&instance_id) {
        Some(i) => i,
        None => return StatusCode::NOT_FOUND.into_response(),
    };
    let mut state = instance.state.write();
    if let Some(pos) = report.position_sec {
        state.position = Some(pmomediarenderer::seconds_to_upnp_time(pos));
    }
    if let Some(dur) = report.duration_sec {
        state.duration = Some(pmomediarenderer::seconds_to_upnp_time(dur));
    }
    if let Some(s) = &report.state {
        state.playback_state = match s.as_str() {
            "playing" => PlaybackState::Playing,
            "paused" => PlaybackState::Paused,
            "stopped" => PlaybackState::Stopped,
            _ => state.playback_state.clone(),
        };
    }
    tracing::debug!(instance_id = %instance_id, position = ?state.position, "player state updated");
    StatusCode::OK.into_response()
}

#[axum::debug_handler]
pub async fn command_handler(
    State(registry): State<Arc<MediaRendererRegistry>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let Some(instance) = registry.get_instance(&instance_id) else {
        return StatusCode::NO_CONTENT.into_response();
    };
    let cmd = instance.state.write().pop_command();
    match cmd.and_then(|c| serde_json::to_value(c).ok()) {
        Some(v) => (StatusCode::OK, Json(v)).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

#[axum::debug_handler]
pub async fn play_handler(
    State(registry): State<Arc<MediaRendererRegistry>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let instance = match registry.get_instance(&instance_id) {
        Some(i) => i,
        None => {
            return (StatusCode::NOT_FOUND, "Instance not found").into_response();
        }
    };
    
    let has_uri = instance.state.read().current_uri.is_some();
    if !has_uri {
        tracing::warn!(instance_id = %instance_id, "Play command ignored: no URI loaded");
        let mut headers = HeaderMap::new();
        headers.insert(axum::http::header::CONTENT_TYPE, "text/plain".parse().unwrap());
        return (StatusCode::BAD_REQUEST, headers, "No URI loaded").into_response();
    }
    
    tracing::info!(instance_id = %instance_id, "WebRenderer: play request");
    
    let stream_url = format!("/api/webrenderer/{}/stream", instance_id);
    instance.adapter.deliver(DeviceCommand::Stream { url: stream_url });
    
    instance.pipeline.send(PipelineControl::Play).await;
    
    (StatusCode::OK, "OK").into_response()
}

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
    State(registry): State<Arc<MediaRendererRegistry>>,
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
    State(registry): State<Arc<MediaRendererRegistry>>,
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