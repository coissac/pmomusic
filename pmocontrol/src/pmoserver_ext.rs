//! Extension pmoserver pour le Control Point
//!
//! Ce module fournit une API REST pour contrôler les renderers UPnP
//! et naviguer dans les serveurs de médias.

#[cfg(feature = "pmoserver")]
use crate::control_point::{ControlPoint, OpenHomeAccessError};
#[cfg(feature = "pmoserver")]
use crate::media_server::{MediaBrowser, MediaEntry, MediaResource, MusicServer, ServerId};
#[cfg(feature = "pmoserver")]
use crate::model::{RendererCapabilities, RendererId, RendererProtocol};
#[cfg(feature = "pmoserver")]
use crate::openapi::{
    AttachPlaylistRequest, AttachedPlaylistInfo, BrowseResponse, ContainerEntry, ErrorResponse,
    MediaServerSummary, OpenHomePlaylistAddRequest, OpenHomePlaylistSnapshot, PlayContentRequest,
    QueueItem, QueueSnapshot, RendererCapabilitiesSummary, RendererProtocolSummary, RendererState,
    RendererSummary, SuccessResponse, VolumeSetRequest,
};
#[cfg(feature = "pmoserver")]
use crate::playback_queue::PlaybackItem;
#[cfg(feature = "pmoserver")]
use crate::{PlaybackPosition, PlaybackStatus, TransportControl, VolumeControl};

#[cfg(feature = "pmoserver")]
use async_trait::async_trait;
#[cfg(feature = "pmoserver")]
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
#[cfg(feature = "pmoserver")]
use std::sync::Arc;
#[cfg(feature = "pmoserver")]
use std::time::Duration;
#[cfg(feature = "pmoserver")]
use tokio::time;
#[cfg(feature = "pmoserver")]
use tracing::{debug, warn};
#[cfg(feature = "pmoserver")]
use utoipa::OpenApi;

#[cfg(feature = "pmoserver")]
const BROWSE_PAGE_SIZE: u32 = 100;
#[cfg(feature = "pmoserver")]
const MEDIA_SERVER_SOAP_TIMEOUT: Duration = Duration::from_secs(15);
#[cfg(feature = "pmoserver")]
const BROWSE_REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

// Timeouts for simple commands (play/pause/stop)
#[cfg(feature = "pmoserver")]
const TRANSPORT_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

// Timeouts for volume/mute commands (faster than transport)
#[cfg(feature = "pmoserver")]
const VOLUME_COMMAND_TIMEOUT: Duration = Duration::from_secs(3);

// Timeout for queue operations
#[cfg(feature = "pmoserver")]
const QUEUE_COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

// Timeout for attach playlist (includes browse + cache + queue update)
#[cfg(feature = "pmoserver")]
const ATTACH_PLAYLIST_TIMEOUT: Duration = Duration::from_secs(60);

// Timeout for renderer state queries (multiple SOAP calls)
#[cfg(feature = "pmoserver")]
const STATE_QUERY_TIMEOUT: Duration = Duration::from_secs(8);

/// État partagé pour l'API ControlPoint
#[cfg(feature = "pmoserver")]
#[derive(Clone)]
pub struct ControlPointState {
    control_point: Arc<ControlPoint>,
}

#[cfg(feature = "pmoserver")]
impl ControlPointState {
    pub fn new(control_point: Arc<ControlPoint>) -> Self {
        Self { control_point }
    }
}

// ============================================================================
// HANDLERS - RENDERERS
// ============================================================================

/// GET /control/renderers - Liste tous les renderers
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    get,
    path = "/renderers",
    responses(
        (status = 200, description = "Liste des renderers", body = Vec<RendererSummary>)
    ),
    tag = "control"
)]
async fn list_renderers(State(state): State<ControlPointState>) -> Json<Vec<RendererSummary>> {
    let renderers = state.control_point.list_music_renderers();

    let summaries: Vec<RendererSummary> = renderers
        .into_iter()
        .map(|r| {
            let info = r.info();
            RendererSummary {
                id: info.id.0.clone(),
                friendly_name: info.friendly_name.clone(),
                model_name: info.model_name.clone(),
                protocol: protocol_summary(&info.protocol),
                capabilities: capability_summary(&info.capabilities),
                online: info.online,
            }
        })
        .collect();

    Json(summaries)
}

/// GET /control/renderers/{renderer_id} - Récupère l'état d'un renderer
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    get,
    path = "/renderers/{renderer_id}",
    params(
        ("renderer_id" = String, Path, description = "ID unique du renderer")
    ),
    responses(
        (status = 200, description = "État du renderer", body = RendererState),
        (status = 404, description = "Renderer non trouvé", body = ErrorResponse)
    ),
    tag = "control"
)]
async fn get_renderer_state(
    State(state): State<ControlPointState>,
    Path(renderer_id): Path<String>,
) -> Result<Json<RendererState>, (StatusCode, Json<ErrorResponse>)> {
    let rid = RendererId(renderer_id.clone());

    let renderer = state
        .control_point
        .music_renderer_by_id(&rid)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Renderer {} not found", renderer_id),
                }),
            )
        })?;

    let info = renderer.info();
    let renderer_clone = renderer.clone();

    // Spawn blocking task for all SOAP calls to avoid blocking Tokio runtime
    let state_task = tokio::task::spawn_blocking(move || {
        // État de transport
        let transport_state = renderer_clone
            .playback_state()
            .ok()
            .map(state_to_string)
            .unwrap_or_else(|| "UNKNOWN".to_string());

        // Position et durée
        let (position_ms, duration_ms) = renderer_clone
            .playback_position()
            .ok()
            .and_then(|pos| {
                let position = parse_hms_to_ms(pos.rel_time.as_deref());
                let duration = parse_hms_to_ms(pos.track_duration.as_deref());
                Some((position, duration))
            })
            .unwrap_or((None, None));

        // Volume et mute
        let volume = renderer_clone
            .volume()
            .ok()
            .and_then(|v| u8::try_from(v).ok());
        let mute = renderer_clone.mute().ok();

        (transport_state, position_ms, duration_ms, volume, mute)
    });

    let (transport_state, position_ms, duration_ms, volume, mute) =
        time::timeout(STATE_QUERY_TIMEOUT, state_task)
            .await
            .map_err(|_| {
                warn!(
                    "State query for renderer {} exceeded {:?}",
                    renderer_id, STATE_QUERY_TIMEOUT
                );
                (
                    StatusCode::GATEWAY_TIMEOUT,
                    Json(ErrorResponse {
                        error: format!(
                            "State query timed out after {}s",
                            STATE_QUERY_TIMEOUT.as_secs()
                        ),
                    }),
                )
            })?
            .map_err(|e| {
                warn!("Task join error during state query: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Internal task error: {}", e),
                    }),
                )
            })?;

    // Queue (non-blocking, local data)
    let queue_len = state
        .control_point
        .get_queue_snapshot(&rid)
        .ok()
        .map(|q| q.len())
        .unwrap_or(0);

    // Playlist binding (non-blocking, local data)
    let attached_playlist = state
        .control_point
        .current_queue_playlist_binding(&rid)
        .map(
            |(server_id, container_id, has_seen_update)| AttachedPlaylistInfo {
                server_id: server_id.0,
                container_id,
                has_seen_update,
            },
        );

    Ok(Json(RendererState {
        id: info.id.0.clone(),
        friendly_name: info.friendly_name.clone(),
        transport_state,
        position_ms,
        duration_ms,
        volume,
        mute,
        queue_len,
        attached_playlist,
    }))
}

/// GET /control/renderers/{renderer_id}/queue - Récupère la queue d'un renderer
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    get,
    path = "/renderers/{renderer_id}/queue",
    params(
        ("renderer_id" = String, Path, description = "ID unique du renderer")
    ),
    responses(
        (
            status = 200,
            description = "Playlist complète du renderer (avec index courant)",
            body = QueueSnapshot
        ),
        (status = 404, description = "Renderer non trouvé", body = ErrorResponse)
    ),
    tag = "control"
)]
async fn get_renderer_queue(
    State(state): State<ControlPointState>,
    Path(renderer_id): Path<String>,
) -> Result<Json<QueueSnapshot>, (StatusCode, Json<ErrorResponse>)> {
    let rid = RendererId(renderer_id.clone());

    // Verify renderer exists in registry
    let _renderer = state
        .control_point
        .music_renderer_by_id(&rid)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Renderer {} not found", renderer_id),
                }),
            )
        })?;

    // Get queue snapshot - if renderer not yet in runtime (just discovered),
    // return empty queue instead of error
    let (items, current_index) = state
        .control_point
        .get_full_queue_snapshot(&rid)
        .unwrap_or_else(|_| {
            debug!(
                renderer = renderer_id.as_str(),
                "Renderer not yet initialized in runtime, returning empty queue"
            );
            (vec![], None)
        });

    let queue_items: Vec<QueueItem> = items
        .into_iter()
        .enumerate()
        .map(|(index, item)| QueueItem {
            index,
            uri: item.uri,
            title: item.title,
            artist: item.artist,
            album: item.album,
            server_id: item.server_id.map(|s| s.0),
            object_id: item.object_id,
        })
        .collect();

    Ok(Json(QueueSnapshot {
        renderer_id,
        items: queue_items,
        current_index,
    }))
}

/// GET /control/renderers/{renderer_id}/binding - Récupère le binding playlist
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    get,
    path = "/renderers/{renderer_id}/binding",
    params(
        ("renderer_id" = String, Path, description = "ID unique du renderer")
    ),
    responses(
        (status = 200, description = "Binding playlist", body = Option<AttachedPlaylistInfo>),
        (status = 404, description = "Renderer non trouvé", body = ErrorResponse)
    ),
    tag = "control"
)]
async fn get_renderer_binding(
    State(state): State<ControlPointState>,
    Path(renderer_id): Path<String>,
) -> Result<Json<Option<AttachedPlaylistInfo>>, (StatusCode, Json<ErrorResponse>)> {
    let rid = RendererId(renderer_id);

    let binding = state
        .control_point
        .current_queue_playlist_binding(&rid)
        .map(
            |(server_id, container_id, has_seen_update)| AttachedPlaylistInfo {
                server_id: server_id.0,
                container_id,
                has_seen_update,
            },
        );

    Ok(Json(binding))
}

// ============================================================================
// HANDLERS - CONTRÔLE TRANSPORT
// ============================================================================

/// POST /control/renderers/{renderer_id}/play - Démarre la lecture
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    post,
    path = "/renderers/{renderer_id}/play",
    params(
        ("renderer_id" = String, Path, description = "ID unique du renderer")
    ),
    responses(
        (status = 200, description = "Lecture démarrée", body = SuccessResponse),
        (status = 404, description = "Renderer non trouvé", body = ErrorResponse),
        (status = 500, description = "Erreur lors de l'exécution", body = ErrorResponse)
    ),
    tag = "control"
)]
async fn play_renderer(
    State(state): State<ControlPointState>,
    Path(renderer_id): Path<String>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let rid = RendererId(renderer_id.clone());

    let renderer = state
        .control_point
        .music_renderer_by_id(&rid)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Renderer {} not found", renderer_id),
                }),
            )
        })?;

    let renderer_clone = renderer.clone();
    let play_task = tokio::task::spawn_blocking(move || renderer_clone.play());

    time::timeout(TRANSPORT_COMMAND_TIMEOUT, play_task)
        .await
        .map_err(|_| {
            warn!(
                "Play command for renderer {} exceeded {:?}",
                renderer_id, TRANSPORT_COMMAND_TIMEOUT
            );
            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(ErrorResponse {
                    error: format!(
                        "Play command timed out after {}s",
                        TRANSPORT_COMMAND_TIMEOUT.as_secs()
                    ),
                }),
            )
        })?
        .map_err(|e| {
            warn!("Task join error during play: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Internal task error: {}", e),
                }),
            )
        })?
        .map_err(|e| {
            warn!("Failed to play renderer {}: {}", renderer_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to play: {}", e),
                }),
            )
        })?;

    Ok(Json(SuccessResponse {
        message: "Playback started".to_string(),
    }))
}

/// POST /control/renderers/{renderer_id}/pause - Met en pause
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    post,
    path = "/renderers/{renderer_id}/pause",
    params(
        ("renderer_id" = String, Path, description = "ID unique du renderer")
    ),
    responses(
        (status = 200, description = "Lecture en pause", body = SuccessResponse),
        (status = 404, description = "Renderer non trouvé", body = ErrorResponse),
        (status = 500, description = "Erreur lors de l'exécution", body = ErrorResponse)
    ),
    tag = "control"
)]
async fn pause_renderer(
    State(state): State<ControlPointState>,
    Path(renderer_id): Path<String>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let rid = RendererId(renderer_id.clone());

    let renderer = state
        .control_point
        .music_renderer_by_id(&rid)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Renderer {} not found", renderer_id),
                }),
            )
        })?;

    let renderer_clone = renderer.clone();
    let pause_task = tokio::task::spawn_blocking(move || renderer_clone.pause());

    time::timeout(TRANSPORT_COMMAND_TIMEOUT, pause_task)
        .await
        .map_err(|_| {
            warn!(
                "Pause command for renderer {} exceeded {:?}",
                renderer_id, TRANSPORT_COMMAND_TIMEOUT
            );
            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(ErrorResponse {
                    error: format!(
                        "Pause command timed out after {}s",
                        TRANSPORT_COMMAND_TIMEOUT.as_secs()
                    ),
                }),
            )
        })?
        .map_err(|e| {
            warn!("Task join error during pause: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Internal task error: {}", e),
                }),
            )
        })?
        .map_err(|e| {
            warn!("Failed to pause renderer {}: {}", renderer_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to pause: {}", e),
                }),
            )
        })?;

    Ok(Json(SuccessResponse {
        message: "Playback paused".to_string(),
    }))
}

/// POST /control/renderers/{renderer_id}/stop - Arrête la lecture
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    post,
    path = "/renderers/{renderer_id}/stop",
    params(
        ("renderer_id" = String, Path, description = "ID unique du renderer")
    ),
    responses(
        (status = 200, description = "Lecture arrêtée", body = SuccessResponse),
        (status = 404, description = "Renderer non trouvé", body = ErrorResponse),
        (status = 500, description = "Erreur lors de l'exécution", body = ErrorResponse)
    ),
    tag = "control"
)]
async fn stop_renderer(
    State(state): State<ControlPointState>,
    Path(renderer_id): Path<String>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let rid = RendererId(renderer_id.clone());

    // Verify renderer exists
    let _renderer = state
        .control_point
        .music_renderer_by_id(&rid)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Renderer {} not found", renderer_id),
                }),
            )
        })?;

    let control_point = state.control_point.clone();
    let rid_clone = rid.clone();
    let stop_task = tokio::task::spawn_blocking(move || control_point.user_stop(&rid_clone));

    time::timeout(TRANSPORT_COMMAND_TIMEOUT, stop_task)
        .await
        .map_err(|_| {
            warn!(
                "Stop command for renderer {} exceeded {:?}",
                renderer_id, TRANSPORT_COMMAND_TIMEOUT
            );
            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(ErrorResponse {
                    error: format!(
                        "Stop command timed out after {}s",
                        TRANSPORT_COMMAND_TIMEOUT.as_secs()
                    ),
                }),
            )
        })?
        .map_err(|e| {
            warn!("Task join error during stop: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Internal task error: {}", e),
                }),
            )
        })?
        .map_err(|e| {
            warn!("Failed to stop renderer {}: {}", renderer_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to stop: {}", e),
                }),
            )
        })?;

    Ok(Json(SuccessResponse {
        message: "Playback stopped".to_string(),
    }))
}

/// POST /control/renderers/{renderer_id}/resume - Reprend la lecture depuis le morceau actuel
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    post,
    path = "/renderers/{renderer_id}/resume",
    params(
        ("renderer_id" = String, Path, description = "ID unique du renderer")
    ),
    responses(
        (status = 200, description = "Lecture reprise", body = SuccessResponse),
        (status = 404, description = "Renderer non trouvé", body = ErrorResponse),
        (status = 500, description = "Erreur lors de l'exécution", body = ErrorResponse)
    ),
    tag = "control"
)]
async fn resume_renderer(
    State(state): State<ControlPointState>,
    Path(renderer_id): Path<String>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let rid = RendererId(renderer_id.clone());
    let control_point = state.control_point.clone();
    let rid_clone = rid.clone();

    let resume_task =
        tokio::task::spawn_blocking(move || control_point.play_current_from_queue(&rid_clone));

    time::timeout(QUEUE_COMMAND_TIMEOUT, resume_task)
        .await
        .map_err(|_| {
            warn!(
                "Resume command for renderer {} exceeded {:?}",
                renderer_id, QUEUE_COMMAND_TIMEOUT
            );
            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(ErrorResponse {
                    error: format!(
                        "Resume command timed out after {}s",
                        QUEUE_COMMAND_TIMEOUT.as_secs()
                    ),
                }),
            )
        })?
        .map_err(|e| {
            warn!("Task join error during resume: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Internal task error: {}", e),
                }),
            )
        })?
        .map_err(|e| {
            warn!("Failed to resume renderer {}: {}", renderer_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to resume: {}", e),
                }),
            )
        })?;

    Ok(Json(SuccessResponse {
        message: "Playback resumed from current track".to_string(),
    }))
}

/// POST /control/renderers/{renderer_id}/next - Passe au morceau suivant de la queue
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    post,
    path = "/renderers/{renderer_id}/next",
    params(
        ("renderer_id" = String, Path, description = "ID unique du renderer")
    ),
    responses(
        (status = 200, description = "Passage au suivant", body = SuccessResponse),
        (status = 404, description = "Renderer non trouvé", body = ErrorResponse),
        (status = 500, description = "Erreur lors de l'exécution", body = ErrorResponse)
    ),
    tag = "control"
)]
async fn next_renderer(
    State(state): State<ControlPointState>,
    Path(renderer_id): Path<String>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let rid = RendererId(renderer_id.clone());
    let control_point = state.control_point.clone();
    let rid_clone = rid.clone();

    let next_task =
        tokio::task::spawn_blocking(move || control_point.play_next_from_queue(&rid_clone));

    time::timeout(QUEUE_COMMAND_TIMEOUT, next_task)
        .await
        .map_err(|_| {
            warn!(
                "Next command for renderer {} exceeded {:?}",
                renderer_id, QUEUE_COMMAND_TIMEOUT
            );
            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(ErrorResponse {
                    error: format!(
                        "Next command timed out after {}s",
                        QUEUE_COMMAND_TIMEOUT.as_secs()
                    ),
                }),
            )
        })?
        .map_err(|e| {
            warn!("Task join error during next: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Internal task error: {}", e),
                }),
            )
        })?
        .map_err(|e| {
            warn!(
                "Failed to advance queue for renderer {}: {}",
                renderer_id, e
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to advance queue: {}", e),
                }),
            )
        })?;

    Ok(Json(SuccessResponse {
        message: "Advanced to next track".to_string(),
    }))
}

// ============================================================================
// HANDLERS - VOLUME
// ============================================================================

/// POST /control/renderers/{renderer_id}/volume/set - Définit le volume
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    post,
    path = "/renderers/{renderer_id}/volume/set",
    params(
        ("renderer_id" = String, Path, description = "ID unique du renderer")
    ),
    request_body = VolumeSetRequest,
    responses(
        (status = 200, description = "Volume défini", body = SuccessResponse),
        (status = 400, description = "Requête invalide", body = ErrorResponse),
        (status = 404, description = "Renderer non trouvé", body = ErrorResponse),
        (status = 500, description = "Erreur lors de l'exécution", body = ErrorResponse)
    ),
    tag = "control"
)]
async fn set_renderer_volume(
    State(state): State<ControlPointState>,
    Path(renderer_id): Path<String>,
    Json(req): Json<VolumeSetRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let rid = RendererId(renderer_id.clone());

    let renderer = state
        .control_point
        .music_renderer_by_id(&rid)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Renderer {} not found", renderer_id),
                }),
            )
        })?;

    let renderer_clone = renderer.clone();
    let volume = req.volume;
    let volume_task = tokio::task::spawn_blocking(move || renderer_clone.set_volume(volume as u16));

    time::timeout(VOLUME_COMMAND_TIMEOUT, volume_task)
        .await
        .map_err(|_| {
            warn!(
                "Set volume command for renderer {} exceeded {:?}",
                renderer_id, VOLUME_COMMAND_TIMEOUT
            );
            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(ErrorResponse {
                    error: format!(
                        "Set volume command timed out after {}s",
                        VOLUME_COMMAND_TIMEOUT.as_secs()
                    ),
                }),
            )
        })?
        .map_err(|e| {
            warn!("Task join error during set volume: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Internal task error: {}", e),
                }),
            )
        })?
        .map_err(|e| {
            warn!("Failed to set volume for renderer {}: {}", renderer_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to set volume: {}", e),
                }),
            )
        })?;

    Ok(Json(SuccessResponse {
        message: format!("Volume set to {}", volume),
    }))
}

/// POST /control/renderers/{renderer_id}/volume/up - Augmente le volume
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    post,
    path = "/renderers/{renderer_id}/volume/up",
    params(
        ("renderer_id" = String, Path, description = "ID unique du renderer")
    ),
    responses(
        (status = 200, description = "Volume augmenté", body = SuccessResponse),
        (status = 404, description = "Renderer non trouvé", body = ErrorResponse),
        (status = 500, description = "Erreur lors de l'exécution", body = ErrorResponse)
    ),
    tag = "control"
)]
async fn volume_up_renderer(
    State(state): State<ControlPointState>,
    Path(renderer_id): Path<String>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let rid = RendererId(renderer_id.clone());

    let renderer = state
        .control_point
        .music_renderer_by_id(&rid)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Renderer {} not found", renderer_id),
                }),
            )
        })?;

    let renderer_clone = renderer.clone();
    let volume_task = tokio::task::spawn_blocking(move || {
        let current = renderer_clone.volume()?;
        let new_volume = (current + 5).min(100);
        renderer_clone.set_volume(new_volume)?;
        Ok::<u16, anyhow::Error>(new_volume)
    });

    let new_volume = time::timeout(VOLUME_COMMAND_TIMEOUT, volume_task)
        .await
        .map_err(|_| {
            warn!(
                "Volume up command for renderer {} exceeded {:?}",
                renderer_id, VOLUME_COMMAND_TIMEOUT
            );
            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(ErrorResponse {
                    error: format!(
                        "Volume up command timed out after {}s",
                        VOLUME_COMMAND_TIMEOUT.as_secs()
                    ),
                }),
            )
        })?
        .map_err(|e| {
            warn!("Task join error during volume up: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Internal task error: {}", e),
                }),
            )
        })?
        .map_err(|e| {
            warn!(
                "Failed to increase volume for renderer {}: {}",
                renderer_id, e
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to increase volume: {}", e),
                }),
            )
        })?;

    Ok(Json(SuccessResponse {
        message: format!("Volume increased to {}", new_volume),
    }))
}

/// POST /control/renderers/{renderer_id}/volume/down - Diminue le volume
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    post,
    path = "/renderers/{renderer_id}/volume/down",
    params(
        ("renderer_id" = String, Path, description = "ID unique du renderer")
    ),
    responses(
        (status = 200, description = "Volume diminué", body = SuccessResponse),
        (status = 404, description = "Renderer non trouvé", body = ErrorResponse),
        (status = 500, description = "Erreur lors de l'exécution", body = ErrorResponse)
    ),
    tag = "control"
)]
async fn volume_down_renderer(
    State(state): State<ControlPointState>,
    Path(renderer_id): Path<String>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let rid = RendererId(renderer_id.clone());

    let renderer = state
        .control_point
        .music_renderer_by_id(&rid)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Renderer {} not found", renderer_id),
                }),
            )
        })?;

    let renderer_clone = renderer.clone();
    let volume_task = tokio::task::spawn_blocking(move || {
        let current = renderer_clone.volume()?;
        let new_volume = current.saturating_sub(5);
        renderer_clone.set_volume(new_volume)?;
        Ok::<u16, anyhow::Error>(new_volume)
    });

    let new_volume = time::timeout(VOLUME_COMMAND_TIMEOUT, volume_task)
        .await
        .map_err(|_| {
            warn!(
                "Volume down command for renderer {} exceeded {:?}",
                renderer_id, VOLUME_COMMAND_TIMEOUT
            );
            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(ErrorResponse {
                    error: format!(
                        "Volume down command timed out after {}s",
                        VOLUME_COMMAND_TIMEOUT.as_secs()
                    ),
                }),
            )
        })?
        .map_err(|e| {
            warn!("Task join error during volume down: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Internal task error: {}", e),
                }),
            )
        })?
        .map_err(|e| {
            warn!(
                "Failed to decrease volume for renderer {}: {}",
                renderer_id, e
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to decrease volume: {}", e),
                }),
            )
        })?;

    Ok(Json(SuccessResponse {
        message: format!("Volume decreased to {}", new_volume),
    }))
}

/// POST /control/renderers/{renderer_id}/mute/toggle - Bascule le mute
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    post,
    path = "/renderers/{renderer_id}/mute/toggle",
    params(
        ("renderer_id" = String, Path, description = "ID unique du renderer")
    ),
    responses(
        (status = 200, description = "Mute basculé", body = SuccessResponse),
        (status = 404, description = "Renderer non trouvé", body = ErrorResponse),
        (status = 500, description = "Erreur lors de l'exécution", body = ErrorResponse)
    ),
    tag = "control"
)]
async fn toggle_mute_renderer(
    State(state): State<ControlPointState>,
    Path(renderer_id): Path<String>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let rid = RendererId(renderer_id.clone());

    let renderer = state
        .control_point
        .music_renderer_by_id(&rid)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Renderer {} not found", renderer_id),
                }),
            )
        })?;

    let renderer_clone = renderer.clone();
    let mute_task = tokio::task::spawn_blocking(move || {
        let current_mute = renderer_clone.mute()?;
        let new_mute = !current_mute;
        renderer_clone.set_mute(new_mute)?;
        Ok::<bool, anyhow::Error>(new_mute)
    });

    let new_mute = time::timeout(VOLUME_COMMAND_TIMEOUT, mute_task)
        .await
        .map_err(|_| {
            warn!(
                "Toggle mute command for renderer {} exceeded {:?}",
                renderer_id, VOLUME_COMMAND_TIMEOUT
            );
            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(ErrorResponse {
                    error: format!(
                        "Toggle mute command timed out after {}s",
                        VOLUME_COMMAND_TIMEOUT.as_secs()
                    ),
                }),
            )
        })?
        .map_err(|e| {
            warn!("Task join error during toggle mute: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Internal task error: {}", e),
                }),
            )
        })?
        .map_err(|e| {
            warn!("Failed to toggle mute for renderer {}: {}", renderer_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to toggle mute: {}", e),
                }),
            )
        })?;

    Ok(Json(SuccessResponse {
        message: format!("Mute {}", if new_mute { "enabled" } else { "disabled" }),
    }))
}

// ============================================================================
// HANDLERS - BINDING PLAYLIST
// ============================================================================

/// POST /control/renderers/{renderer_id}/binding/attach - Attache une playlist
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    post,
    path = "/renderers/{renderer_id}/binding/attach",
    params(
        ("renderer_id" = String, Path, description = "ID unique du renderer")
    ),
    request_body = AttachPlaylistRequest,
    responses(
        (status = 200, description = "Playlist attachée", body = SuccessResponse),
        (status = 404, description = "Renderer non trouvé", body = ErrorResponse)
    ),
    tag = "control"
)]
async fn attach_playlist_binding(
    State(state): State<ControlPointState>,
    Path(renderer_id): Path<String>,
    Json(req): Json<AttachPlaylistRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let rid = RendererId(renderer_id.clone());
    let sid = ServerId(req.server_id.clone());
    let container_id = req.container_id.clone();
    let control_point = Arc::clone(&state.control_point);

    // Spawn blocking task and wait for completion with timeout
    let attach_task = tokio::task::spawn_blocking(move || {
        control_point.attach_queue_to_playlist(&rid, sid, container_id);
    });

    time::timeout(ATTACH_PLAYLIST_TIMEOUT, attach_task)
        .await
        .map_err(|_| {
            warn!(
                "Attach playlist for renderer {} exceeded {:?}",
                renderer_id, ATTACH_PLAYLIST_TIMEOUT
            );
            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(ErrorResponse {
                    error: format!(
                        "Attach playlist timed out after {}s",
                        ATTACH_PLAYLIST_TIMEOUT.as_secs()
                    ),
                }),
            )
        })?
        .map_err(|e| {
            warn!("Task join error during attach playlist: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Internal task error: {}", e),
                }),
            )
        })?;

    debug!(
        renderer = renderer_id.as_str(),
        server = req.server_id.as_str(),
        container = req.container_id.as_str(),
        "Playlist attached via HTTP API"
    );

    Ok(Json(SuccessResponse {
        message: format!("Playlist {} attached to renderer", req.container_id),
    }))
}

/// POST /control/renderers/{renderer_id}/binding/detach - Détache la playlist
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    post,
    path = "/renderers/{renderer_id}/binding/detach",
    params(
        ("renderer_id" = String, Path, description = "ID unique du renderer")
    ),
    responses(
        (status = 200, description = "Playlist détachée", body = SuccessResponse)
    ),
    tag = "control"
)]
async fn detach_playlist_binding(
    State(state): State<ControlPointState>,
    Path(renderer_id): Path<String>,
) -> Json<SuccessResponse> {
    let rid = RendererId(renderer_id.clone());

    state.control_point.detach_queue_playlist(&rid);

    debug!(
        renderer = renderer_id.as_str(),
        "Playlist detached via HTTP API"
    );

    Json(SuccessResponse {
        message: "Playlist detached".to_string(),
    })
}

// ============================================================================
// HANDLERS - OPENHOME PLAYLIST
// ============================================================================

/// GET /control/renderers/{renderer_id}/oh/playlist - Snapshot de la playlist OH
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    get,
    path = "/renderers/{renderer_id}/oh/playlist",
    params(
        ("renderer_id" = String, Path, description = "ID unique du renderer")
    ),
    responses(
        (status = 200, description = "Playlist OpenHome", body = OpenHomePlaylistSnapshot),
        (status = 404, description = "Renderer non trouvé ou sans service OH", body = ErrorResponse)
    ),
    tag = "control"
)]
async fn get_openhome_playlist(
    State(state): State<ControlPointState>,
    Path(renderer_id): Path<String>,
) -> Result<Json<OpenHomePlaylistSnapshot>, (StatusCode, Json<ErrorResponse>)> {
    let rid = RendererId(renderer_id.clone());
    let control_point = Arc::clone(&state.control_point);
    let rid_for_task = rid.clone();

    let fetch_task = tokio::task::spawn_blocking(move || {
        control_point.get_openhome_playlist_snapshot(&rid_for_task)
    });

    let snapshot = fetch_task
        .await
        .map_err(|e| {
            warn!(
                renderer = renderer_id.as_str(),
                error = %e,
                "Join error while fetching OpenHome playlist"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Internal task error: {}", e),
                }),
            )
        })?
        .map_err(|e| {
            warn!(
                renderer = renderer_id.as_str(),
                error = %e,
                "Failed to read OpenHome playlist"
            );
            map_openhome_error(&rid, e, "read OpenHome playlist")
        })?;

    Ok(Json(snapshot))
}

/// POST /control/renderers/{renderer_id}/oh/playlist/clear - Vide la playlist OH
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    post,
    path = "/renderers/{renderer_id}/oh/playlist/clear",
    params(
        ("renderer_id" = String, Path, description = "ID unique du renderer")
    ),
    responses(
        (status = 200, description = "Playlist vidée", body = SuccessResponse),
        (status = 404, description = "Renderer non trouvé ou sans service OH", body = ErrorResponse)
    ),
    tag = "control"
)]
async fn clear_openhome_playlist(
    State(state): State<ControlPointState>,
    Path(renderer_id): Path<String>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let rid = RendererId(renderer_id.clone());
    let control_point = Arc::clone(&state.control_point);
    let rid_for_task = rid.clone();

    let clear_task =
        tokio::task::spawn_blocking(move || control_point.clear_openhome_playlist(&rid_for_task));

    time::timeout(QUEUE_COMMAND_TIMEOUT, clear_task)
        .await
        .map_err(|_| {
            warn!(
                renderer = renderer_id.as_str(),
                timeout = QUEUE_COMMAND_TIMEOUT.as_secs(),
                "Clearing OpenHome playlist timed out"
            );
            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(ErrorResponse {
                    error: format!(
                        "Clear playlist timed out after {}s",
                        QUEUE_COMMAND_TIMEOUT.as_secs()
                    ),
                }),
            )
        })?
        .map_err(|e| {
            warn!(
                renderer = renderer_id.as_str(),
                error = %e,
                "Join error while clearing OpenHome playlist"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Internal task error: {}", e),
                }),
            )
        })?
        .map_err(|e| {
            warn!(
                renderer = renderer_id.as_str(),
                error = %e,
                "Failed to clear OpenHome playlist"
            );
            map_openhome_error(&rid, e, "clear OpenHome playlist")
        })?;

    Ok(Json(SuccessResponse {
        message: "OpenHome playlist cleared".to_string(),
    }))
}

/// POST /control/renderers/{renderer_id}/oh/playlist/add - Ajoute un track OH
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    post,
    path = "/renderers/{renderer_id}/oh/playlist/add",
    params(
        ("renderer_id" = String, Path, description = "ID unique du renderer")
    ),
    request_body = OpenHomePlaylistAddRequest,
    responses(
        (status = 200, description = "Track ajouté", body = SuccessResponse),
        (status = 404, description = "Renderer non trouvé ou sans service OH", body = ErrorResponse)
    ),
    tag = "control"
)]
async fn add_openhome_playlist_item(
    State(state): State<ControlPointState>,
    Path(renderer_id): Path<String>,
    Json(req): Json<OpenHomePlaylistAddRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let rid = RendererId(renderer_id.clone());
    let control_point = Arc::clone(&state.control_point);
    let rid_for_task = rid.clone();

    let add_task = tokio::task::spawn_blocking(move || {
        control_point.add_openhome_track(
            &rid_for_task,
            &req.uri,
            &req.metadata,
            req.after_id,
            req.play,
        )
    });

    time::timeout(QUEUE_COMMAND_TIMEOUT, add_task)
        .await
        .map_err(|_| {
            warn!(
                renderer = renderer_id.as_str(),
                timeout = QUEUE_COMMAND_TIMEOUT.as_secs(),
                "Adding OpenHome track timed out"
            );
            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(ErrorResponse {
                    error: format!(
                        "Add track timed out after {}s",
                        QUEUE_COMMAND_TIMEOUT.as_secs()
                    ),
                }),
            )
        })?
        .map_err(|e| {
            warn!(
                renderer = renderer_id.as_str(),
                error = %e,
                "Join error while adding OpenHome track"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Internal task error: {}", e),
                }),
            )
        })?
        .map_err(|e| {
            warn!(
                renderer = renderer_id.as_str(),
                error = %e,
                "Failed to add OpenHome track"
            );
            map_openhome_error(&rid, e, "add OpenHome track")
        })?;

    Ok(Json(SuccessResponse {
        message: "Track added to OpenHome playlist".to_string(),
    }))
}

/// POST /control/renderers/{renderer_id}/oh/playlist/play/{track_id} - PlayId OH
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    post,
    path = "/renderers/{renderer_id}/oh/playlist/play/{track_id}",
    params(
        ("renderer_id" = String, Path, description = "ID unique du renderer"),
        ("track_id" = String, Path, description = "ID OpenHome du morceau")
    ),
    responses(
        (status = 200, description = "Lecture démarrée", body = SuccessResponse),
        (status = 404, description = "Renderer non trouvé ou sans service OH", body = ErrorResponse)
    ),
    tag = "control"
)]
async fn play_openhome_track(
    State(state): State<ControlPointState>,
    Path((renderer_id, track_id)): Path<(String, String)>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let rid = RendererId(renderer_id.clone());
    let parsed_id = track_id.parse::<u32>().map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("Invalid track id '{}': {}", track_id, e),
            }),
        )
    })?;

    let control_point = Arc::clone(&state.control_point);
    let rid_for_task = rid.clone();

    let play_task = tokio::task::spawn_blocking(move || {
        control_point.play_openhome_track_id(&rid_for_task, parsed_id)
    });

    time::timeout(QUEUE_COMMAND_TIMEOUT, play_task)
        .await
        .map_err(|_| {
            warn!(
                renderer = renderer_id.as_str(),
                timeout = QUEUE_COMMAND_TIMEOUT.as_secs(),
                "PlayId command timed out"
            );
            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(ErrorResponse {
                    error: format!(
                        "Play track timed out after {}s",
                        QUEUE_COMMAND_TIMEOUT.as_secs()
                    ),
                }),
            )
        })?
        .map_err(|e| {
            warn!(
                renderer = renderer_id.as_str(),
                error = %e,
                "Join error while playing OpenHome track"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Internal task error: {}", e),
                }),
            )
        })?
        .map_err(|e| {
            warn!(
                renderer = renderer_id.as_str(),
                error = %e,
                track_id = parsed_id,
                "Failed to start OpenHome track"
            );
            map_openhome_error(&rid, e, "play OpenHome track")
        })?;

    Ok(Json(SuccessResponse {
        message: format!("Playing OpenHome track {}", parsed_id),
    }))
}

// ============================================================================
// HANDLERS - QUEUE CONTENT
// ============================================================================

/// POST /control/renderers/{renderer_id}/queue/play - Lire du contenu immédiatement
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    post,
    path = "/renderers/{renderer_id}/queue/play",
    params(
        ("renderer_id" = String, Path, description = "ID unique du renderer")
    ),
    request_body = PlayContentRequest,
    responses(
        (status = 200, description = "Contenu en cours de lecture", body = SuccessResponse),
        (status = 404, description = "Renderer ou serveur non trouvé", body = ErrorResponse),
        (status = 500, description = "Erreur lors de l'exécution", body = ErrorResponse)
    ),
    tag = "control"
)]
async fn play_content(
    State(state): State<ControlPointState>,
    Path(renderer_id): Path<String>,
    Json(req): Json<PlayContentRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let rid = RendererId(renderer_id.clone());
    let sid = ServerId(req.server_id.clone());
    let object_id = req.object_id.clone();
    let object_id_for_log = object_id.clone();

    // Get renderer to verify it exists
    let renderer = state
        .control_point
        .music_renderer_by_id(&rid)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Renderer {} not found", renderer_id),
                }),
            )
        })?;

    let control_point = Arc::clone(&state.control_point);

    // Spawn blocking task for content loading
    let play_task = tokio::task::spawn_blocking(move || {
        // Fetch playback items from server
        let items = fetch_playback_items(&control_point, &sid, &object_id)?;

        if items.is_empty() {
            return Err(anyhow::anyhow!("No playable content found"));
        }

        // Clear queue
        control_point.clear_queue(&rid)?;

        // Enqueue items
        control_point.enqueue_items(&rid, items.clone())?;

        // Start playback
        renderer.play()?;

        // Auto-bind if playing a container (playlist/album)
        // Rule: if multiple items, it's a container that should be bound
        // We use _without_refresh because the queue was already populated by enqueue_items above
        if items.len() > 1 {
            debug!(
                renderer = rid.0.as_str(),
                server = sid.0.as_str(),
                object = object_id.as_str(),
                item_count = items.len(),
                "Auto-binding playlist to renderer queue (without initial refresh)"
            );
            control_point.attach_queue_to_playlist_without_refresh(
                &rid,
                sid.clone(),
                object_id.clone(),
            );
        }

        Ok::<(), anyhow::Error>(())
    });

    time::timeout(QUEUE_COMMAND_TIMEOUT, play_task)
        .await
        .map_err(|_| {
            warn!(
                "Play content command for renderer {} exceeded {:?}",
                renderer_id, QUEUE_COMMAND_TIMEOUT
            );
            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(ErrorResponse {
                    error: format!(
                        "Play content timed out after {}s",
                        QUEUE_COMMAND_TIMEOUT.as_secs()
                    ),
                }),
            )
        })?
        .map_err(|e| {
            warn!("Task join error during play content: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Internal task error: {}", e),
                }),
            )
        })?
        .map_err(|e| {
            warn!("Failed to play content on renderer {}: {}", renderer_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to play content: {}", e),
                }),
            )
        })?;

    debug!(
        renderer = renderer_id.as_str(),
        server = req.server_id.as_str(),
        object = object_id_for_log.as_str(),
        "Content playing via HTTP API"
    );

    Ok(Json(SuccessResponse {
        message: "Content playing".to_string(),
    }))
}

/// POST /control/renderers/{renderer_id}/queue/add - Ajouter du contenu à la queue
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    post,
    path = "/renderers/{renderer_id}/queue/add",
    params(
        ("renderer_id" = String, Path, description = "ID unique du renderer")
    ),
    request_body = PlayContentRequest,
    responses(
        (status = 200, description = "Contenu ajouté à la queue", body = SuccessResponse),
        (status = 404, description = "Renderer ou serveur non trouvé", body = ErrorResponse),
        (status = 500, description = "Erreur lors de l'exécution", body = ErrorResponse)
    ),
    tag = "control"
)]
async fn add_to_queue(
    State(state): State<ControlPointState>,
    Path(renderer_id): Path<String>,
    Json(req): Json<PlayContentRequest>,
) -> Result<Json<SuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let rid = RendererId(renderer_id.clone());
    let sid = ServerId(req.server_id.clone());
    let object_id = req.object_id.clone();
    let object_id_for_log = object_id.clone();

    // Verify renderer exists
    state
        .control_point
        .music_renderer_by_id(&rid)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Renderer {} not found", renderer_id),
                }),
            )
        })?;

    let control_point = Arc::clone(&state.control_point);

    // Spawn blocking task for content loading
    let add_task = tokio::task::spawn_blocking(move || {
        // Fetch playback items from server
        let items = fetch_playback_items(&control_point, &sid, &object_id)?;

        if items.is_empty() {
            return Err(anyhow::anyhow!("No playable content found"));
        }

        // Enqueue items
        control_point.enqueue_items(&rid, items)?;

        Ok::<(), anyhow::Error>(())
    });

    time::timeout(QUEUE_COMMAND_TIMEOUT, add_task)
        .await
        .map_err(|_| {
            warn!(
                "Add to queue command for renderer {} exceeded {:?}",
                renderer_id, QUEUE_COMMAND_TIMEOUT
            );
            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(ErrorResponse {
                    error: format!(
                        "Add to queue timed out after {}s",
                        QUEUE_COMMAND_TIMEOUT.as_secs()
                    ),
                }),
            )
        })?
        .map_err(|e| {
            warn!("Task join error during add to queue: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Internal task error: {}", e),
                }),
            )
        })?
        .map_err(|e| {
            warn!(
                "Failed to add content to queue for renderer {}: {}",
                renderer_id, e
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to add to queue: {}", e),
                }),
            )
        })?;

    debug!(
        renderer = renderer_id.as_str(),
        server = req.server_id.as_str(),
        object = object_id_for_log.as_str(),
        "Content added to queue via HTTP API"
    );

    Ok(Json(SuccessResponse {
        message: "Content added to queue".to_string(),
    }))
}

// ============================================================================
// HANDLERS - MEDIA SERVERS
// ============================================================================

/// GET /control/servers - Liste tous les serveurs de médias
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    get,
    path = "/servers",
    responses(
        (status = 200, description = "Liste des serveurs de médias", body = Vec<MediaServerSummary>)
    ),
    tag = "control"
)]
async fn list_servers(State(state): State<ControlPointState>) -> Json<Vec<MediaServerSummary>> {
    let servers = state.control_point.list_media_servers();

    let summaries: Vec<MediaServerSummary> = servers
        .into_iter()
        .map(|s| MediaServerSummary {
            id: s.id.0,
            friendly_name: s.friendly_name,
            model_name: s.model_name,
            online: s.online,
        })
        .collect();

    Json(summaries)
}

/// GET /control/servers/{server_id}/containers/{container_id} - Browse un container
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    get,
    path = "/servers/{server_id}/containers/{container_id}",
    params(
        ("server_id" = String, Path, description = "ID unique du serveur"),
        ("container_id" = String, Path, description = "ID du container (use '0' for root)")
    ),
    responses(
        (status = 200, description = "Contenu du container", body = BrowseResponse),
        (status = 404, description = "Serveur non trouvé", body = ErrorResponse),
        (status = 500, description = "Erreur lors du browse", body = ErrorResponse)
    ),
    tag = "control"
)]
async fn browse_container(
    State(state): State<ControlPointState>,
    Path((server_id, container_id)): Path<(String, String)>,
) -> Result<Json<BrowseResponse>, (StatusCode, Json<ErrorResponse>)> {
    let sid = ServerId(server_id.clone());

    let server_info = state.control_point.media_server(&sid).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Server {} not found", server_id),
            }),
        )
    })?;

    if !server_info.online {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: format!("Server {} is offline", server_id),
            }),
        ));
    }

    if !server_info.has_content_directory {
        return Err((
            StatusCode::NOT_IMPLEMENTED,
            Json(ErrorResponse {
                error: format!("Server {} does not support ContentDirectory", server_id),
            }),
        ));
    }

    let music_server =
        MusicServer::from_info(&server_info, MEDIA_SERVER_SOAP_TIMEOUT).map_err(|e| {
            warn!("Failed to create MusicServer for {}: {}", server_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to initialize server: {}", e),
                }),
            )
        })?;

    // Use spawn_blocking to avoid blocking the async runtime with synchronous SOAP calls
    let container_id_clone = container_id.clone();
    let browse_task = tokio::task::spawn_blocking(move || {
        music_server.browse_children(&container_id_clone, 0, BROWSE_PAGE_SIZE)
    });

    let entries = time::timeout(BROWSE_REQUEST_TIMEOUT, browse_task)
        .await
        .map_err(|_| {
            warn!(
                "Browse request for container {} on server {} exceeded {:?}",
                container_id, server_id, BROWSE_REQUEST_TIMEOUT
            );
            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(ErrorResponse {
                    error: format!(
                        "Browse request timed out after {}s",
                        BROWSE_REQUEST_TIMEOUT.as_secs()
                    ),
                }),
            )
        })?
        .map_err(|e| {
            warn!("Task join error during browse: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Internal task error: {}", e),
                }),
            )
        })?
        .map_err(|e| {
            warn!(
                "Failed to browse container {} on server {}: {}",
                container_id, server_id, e
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to browse container: {}", e),
                }),
            )
        })?;

    let container_entries: Vec<ContainerEntry> = entries
        .into_iter()
        .map(|e| ContainerEntry {
            id: e.id,
            title: e.title,
            class: e.class,
            is_container: e.is_container,
            child_count: None, // Could be extracted from DIDL-Lite if needed
            artist: e.artist,
            album: e.album,
            album_art_uri: e.album_art_uri,
        })
        .collect();

    Ok(Json(BrowseResponse {
        container_id,
        entries: container_entries,
    }))
}

// ============================================================================
// HELPERS
// ============================================================================

#[cfg(feature = "pmoserver")]
fn map_openhome_error(
    renderer_id: &RendererId,
    err: anyhow::Error,
    context: &str,
) -> (StatusCode, Json<ErrorResponse>) {
    if err.downcast_ref::<OpenHomeAccessError>().is_some() {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: err.to_string(),
            }),
        )
    } else {
        (
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse {
                error: format!(
                    "Failed to {context} for renderer {}: {}",
                    renderer_id.0, err
                ),
            }),
        )
    }
}

/// Helper to fetch playback items from a media server object (container or item).
///
/// This function browses the server to get the entries and converts them to PlaybackItem.
/// For containers, it browses children. For items, it browses metadata.
#[cfg(feature = "pmoserver")]
fn fetch_playback_items(
    control_point: &ControlPoint,
    server_id: &ServerId,
    object_id: &str,
) -> anyhow::Result<Vec<PlaybackItem>> {
    // Get server info from registry
    let server_info = control_point
        .media_server(server_id)
        .ok_or_else(|| anyhow::anyhow!("Server {} not found", server_id.0))?;

    if !server_info.online {
        return Err(anyhow::anyhow!("Server {} is offline", server_id.0));
    }

    if !server_info.has_content_directory {
        return Err(anyhow::anyhow!(
            "Server {} does not support ContentDirectory",
            server_id.0
        ));
    }

    // Create MusicServer
    let music_server = MusicServer::from_info(&server_info, MEDIA_SERVER_SOAP_TIMEOUT)?;

    // Browse the object to get entries
    let entries = music_server.browse_children(object_id, 0, BROWSE_PAGE_SIZE)?;

    // Convert to PlaybackItem
    let items: Vec<PlaybackItem> = entries
        .iter()
        .filter_map(|entry| playback_item_from_entry(&music_server, entry))
        .collect();

    Ok(items)
}

/// Helper to convert a MediaEntry to a PlaybackItem.
#[cfg(feature = "pmoserver")]
fn playback_item_from_entry(server: &MusicServer, entry: &MediaEntry) -> Option<PlaybackItem> {
    // Ignore containers
    if entry.is_container {
        return None;
    }

    // Skip "live stream" entries
    if entry.title.to_ascii_lowercase().contains("live stream") {
        return None;
    }

    // Find an audio resource
    let resource = entry.resources.iter().find(|res| is_audio_resource(res))?;

    let mut item = PlaybackItem::new(resource.uri.clone());
    item.title = Some(entry.title.clone());
    item.server_id = Some(server.id().clone());
    item.object_id = Some(entry.id.clone());
    item.artist = entry.artist.clone();
    item.album = entry.album.clone();
    item.genre = entry.genre.clone();
    item.album_art_uri = entry.album_art_uri.clone();
    item.date = entry.date.clone();
    item.track_number = entry.track_number.clone();
    item.creator = entry.creator.clone();

    Some(item)
}

/// Helper to detect if a MediaResource is audio content.
#[cfg(feature = "pmoserver")]
fn is_audio_resource(res: &MediaResource) -> bool {
    let lower = res.protocol_info.to_ascii_lowercase();
    if lower.contains("audio/") {
        return true;
    }
    // Check MIME type in protocolInfo (format: protocol:network:contentFormat:additionalInfo)
    lower
        .split(':')
        .nth(2)
        .map(|mime| mime.starts_with("audio/"))
        .unwrap_or(false)
}

#[cfg(feature = "pmoserver")]
fn protocol_summary(protocol: &RendererProtocol) -> RendererProtocolSummary {
    match protocol {
        RendererProtocol::UpnpAvOnly => RendererProtocolSummary::Upnp,
        RendererProtocol::OpenHomeOnly => RendererProtocolSummary::Openhome,
        RendererProtocol::Hybrid => RendererProtocolSummary::Hybrid,
    }
}

#[cfg(feature = "pmoserver")]
fn capability_summary(caps: &RendererCapabilities) -> RendererCapabilitiesSummary {
    RendererCapabilitiesSummary {
        has_avtransport: caps.has_avtransport,
        has_avtransport_set_next: caps.has_avtransport_set_next,
        has_rendering_control: caps.has_rendering_control,
        has_connection_manager: caps.has_connection_manager,
        has_linkplay_http: caps.has_linkplay_http,
        has_arylic_tcp: caps.has_arylic_tcp,
        has_oh_playlist: caps.has_oh_playlist,
        has_oh_volume: caps.has_oh_volume,
        has_oh_info: caps.has_oh_info,
        has_oh_time: caps.has_oh_time,
        has_oh_radio: caps.has_oh_radio,
    }
}

#[cfg(feature = "pmoserver")]
fn state_to_string(state: crate::PlaybackState) -> String {
    use crate::PlaybackState;
    match state {
        PlaybackState::Stopped => "STOPPED".to_string(),
        PlaybackState::Playing => "PLAYING".to_string(),
        PlaybackState::Paused => "PAUSED".to_string(),
        PlaybackState::Transitioning => "TRANSITIONING".to_string(),
        PlaybackState::NoMedia => "NO_MEDIA".to_string(),
        PlaybackState::Unknown(s) => s,
    }
}

#[cfg(feature = "pmoserver")]
fn parse_hms_to_ms(hms: Option<&str>) -> Option<u64> {
    let hms = hms?;
    let parts: Vec<&str> = hms.split(':').collect();
    if parts.len() != 3 {
        return None;
    }

    let hours: u64 = parts[0].parse().ok()?;
    let minutes: u64 = parts[1].parse().ok()?;
    let seconds: u64 = parts[2].parse().ok()?;

    Some((hours * 3600 + minutes * 60 + seconds) * 1000)
}

// ============================================================================
// ROUTER & TRAIT
// ============================================================================

/// Crée le router pour l'API Control Point
#[cfg(feature = "pmoserver")]
pub fn create_api_router(state: ControlPointState, control_point: Arc<ControlPoint>) -> Router {
    Router::new()
        // Renderers
        .route("/renderers", get(list_renderers))
        .route("/renderers/{renderer_id}", get(get_renderer_state))
        .route("/renderers/{renderer_id}/queue", get(get_renderer_queue))
        .route(
            "/renderers/{renderer_id}/binding",
            get(get_renderer_binding),
        )
        // Transport control
        .route("/renderers/{renderer_id}/play", post(play_renderer))
        .route("/renderers/{renderer_id}/pause", post(pause_renderer))
        .route("/renderers/{renderer_id}/stop", post(stop_renderer))
        .route("/renderers/{renderer_id}/resume", post(resume_renderer))
        .route("/renderers/{renderer_id}/next", post(next_renderer))
        // Volume control
        .route(
            "/renderers/{renderer_id}/volume/set",
            post(set_renderer_volume),
        )
        .route(
            "/renderers/{renderer_id}/volume/up",
            post(volume_up_renderer),
        )
        .route(
            "/renderers/{renderer_id}/volume/down",
            post(volume_down_renderer),
        )
        .route(
            "/renderers/{renderer_id}/mute/toggle",
            post(toggle_mute_renderer),
        )
        // Playlist binding
        .route(
            "/renderers/{renderer_id}/binding/attach",
            post(attach_playlist_binding),
        )
        .route(
            "/renderers/{renderer_id}/binding/detach",
            post(detach_playlist_binding),
        )
        // OpenHome playlist
        .route(
            "/renderers/{renderer_id}/oh/playlist",
            get(get_openhome_playlist),
        )
        .route(
            "/renderers/{renderer_id}/oh/playlist/clear",
            post(clear_openhome_playlist),
        )
        .route(
            "/renderers/{renderer_id}/oh/playlist/add",
            post(add_openhome_playlist_item),
        )
        .route(
            "/renderers/{renderer_id}/oh/playlist/play/{track_id}",
            post(play_openhome_track),
        )
        // Queue content
        .route("/renderers/{renderer_id}/queue/play", post(play_content))
        .route("/renderers/{renderer_id}/queue/add", post(add_to_queue))
        // Servers
        .route("/servers", get(list_servers))
        .route(
            "/servers/{server_id}/containers/{container_id}",
            get(browse_container),
        )
        .with_state(state)
        // SSE events - merge the SSE router
        .merge(crate::sse::create_sse_router(control_point))
}

/// Trait d'extension pour pmoserver::Server
///
/// Permet d'initialiser le ControlPoint avec routes HTTP complètes
#[cfg(feature = "pmoserver")]
#[async_trait]
pub trait ControlPointExt {
    /// Enregistre et initialise le Control Point avec son API complète
    ///
    /// Cette fonction de haut niveau :
    /// 1. Lance le runtime du ControlPoint (découverte SSDP, polling renderers, etc.)
    /// 2. Enregistre toutes les routes HTTP REST
    /// 3. Enregistre tous les endpoints SSE pour les événements
    /// 4. Génère la documentation OpenAPI
    ///
    /// # Routes créées
    ///
    /// - API REST: `/api/control/*`
    ///   - `/renderers` - Liste et état des renderers
    ///   - `/servers` - Liste et navigation des serveurs de médias
    ///   - Contrôles de transport, volume, queue, binding
    /// - SSE Events: `/api/control/events/*`
    ///   - `/events` - Tous les événements (renderers + serveurs)
    ///   - `/events/renderers` - Événements renderers uniquement
    ///   - `/events/servers` - Événements serveurs uniquement
    /// - Swagger: `/swagger-ui/control`
    ///
    /// # Arguments
    ///
    /// * `timeout_secs` - Timeout HTTP pour les requêtes UPnP (recommandé: 5 secondes)
    ///
    /// # Returns
    ///
    /// Retourne l'instance du ControlPoint dans un Arc pour permettre
    /// d'interagir avec depuis l'application.
    ///
    /// # Errors
    ///
    /// Retourne une erreur si le runtime SSDP ne peut pas être démarré.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use pmocontrol::ControlPointExt;
    /// use pmoserver::Server;
    ///
    /// let server = Server::create_upnp_server().await?;
    ///
    /// // Enregistrer le Control Point avec timeout de 5 secondes
    /// let control_point = server
    ///     .write()
    ///     .await
    ///     .register_control_point(5)
    ///     .await?;
    ///
    /// // Le Control Point est maintenant actif et ses routes HTTP/SSE sont enregistrées
    /// // On peut l'utiliser directement si besoin
    /// let renderers = control_point.list_music_renderers();
    /// ```
    async fn register_control_point(
        &mut self,
        timeout_secs: u64,
    ) -> std::io::Result<Arc<ControlPoint>>;

    /// Initialise l'API Control Point (bas niveau)
    ///
    /// Cette méthode est appelée automatiquement par `register_control_point()`.
    /// Utilisez `register_control_point()` pour la plupart des cas d'usage.
    ///
    /// # Routes créées
    ///
    /// - API REST: `/api/control/*`
    /// - SSE Events: `/api/control/events/*`
    /// - Swagger: `/swagger-ui/control`
    ///
    /// # Arguments
    ///
    /// * `control_point` - Instance du ControlPoint déjà créée
    async fn init_control_point(&mut self, control_point: Arc<ControlPoint>);
}

#[cfg(feature = "pmoserver")]
#[async_trait]
impl ControlPointExt for pmoserver::Server {
    async fn register_control_point(
        &mut self,
        timeout_secs: u64,
    ) -> std::io::Result<Arc<ControlPoint>> {
        use tracing::info;

        info!("🎛️  Initializing Control Point...");

        // 1. Lancer le runtime du ControlPoint
        let control_point = ControlPoint::spawn(timeout_secs)?;
        let control_point = Arc::new(control_point);

        info!("✅ Control Point runtime started");
        info!("   - SSDP discovery active");
        info!("   - Renderer polling active (1s interval)");
        info!("   - MediaServer event subscriptions active");

        // 2. Enregistrer les routes HTTP REST et SSE
        self.init_control_point(control_point.clone()).await;

        info!("✅ Control Point API registered:");
        info!("   - REST API: /api/control/*");
        info!("   - SSE Events: /api/control/events/*");
        info!("   - OpenAPI docs: /swagger-ui/control");

        Ok(control_point)
    }

    async fn init_control_point(&mut self, control_point: Arc<ControlPoint>) {
        let state = ControlPointState::new(control_point.clone());

        // Créer le router API (inclut REST et SSE)
        let api_router = create_api_router(state, control_point);

        // L'enregistrer avec OpenAPI
        self.add_openapi(api_router, crate::openapi::ApiDoc::openapi(), "control")
            .await;
    }
}
