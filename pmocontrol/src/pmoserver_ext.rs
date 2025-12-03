//! Extension pmoserver pour le Control Point
//!
//! Ce module fournit une API REST pour contrôler les renderers UPnP
//! et naviguer dans les serveurs de médias.

#[cfg(feature = "pmoserver")]
use crate::control_point::ControlPoint;
#[cfg(feature = "pmoserver")]
use crate::media_server::{MediaBrowser, MusicServer, ServerId};
#[cfg(feature = "pmoserver")]
use crate::model::{RendererId, RendererProtocol};
#[cfg(feature = "pmoserver")]
use crate::openapi::{
    AttachedPlaylistInfo, AttachPlaylistRequest, BrowseResponse, ContainerEntry, ErrorResponse,
    MediaServerSummary, QueueItem, QueueSnapshot, RendererState, RendererSummary, SuccessResponse,
    VolumeSetRequest,
};
#[cfg(feature = "pmoserver")]
use crate::{PlaybackPosition, PlaybackStatus, TransportControl, VolumeControl};

#[cfg(feature = "pmoserver")]
use async_trait::async_trait;
#[cfg(feature = "pmoserver")]
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
#[cfg(feature = "pmoserver")]
use std::sync::Arc;
#[cfg(feature = "pmoserver")]
use std::time::Duration;
#[cfg(feature = "pmoserver")]
use tracing::{debug, warn};
#[cfg(feature = "pmoserver")]
use utoipa::OpenApi;

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
async fn list_renderers(
    State(state): State<ControlPointState>,
) -> Json<Vec<RendererSummary>> {
    let renderers = state.control_point.list_music_renderers();

    let summaries: Vec<RendererSummary> = renderers
        .into_iter()
        .map(|r| {
            let info = r.info();
            RendererSummary {
                id: info.id.0.clone(),
                friendly_name: info.friendly_name.clone(),
                model_name: info.model_name.clone(),
                protocol: protocol_to_string(&info.protocol),
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

    // État de transport
    let transport_state = renderer
        .playback_state()
        .ok()
        .map(state_to_string)
        .unwrap_or_else(|| "UNKNOWN".to_string());

    // Position et durée
    let (position_ms, duration_ms) = renderer
        .playback_position()
        .ok()
        .and_then(|pos| {
            let position = parse_hms_to_ms(pos.rel_time.as_deref());
            let duration = parse_hms_to_ms(pos.track_duration.as_deref());
            Some((position, duration))
        })
        .unwrap_or((None, None));

    // Volume et mute
    let volume = renderer.volume().ok().and_then(|v| u8::try_from(v).ok());
    let mute = renderer.mute().ok();

    // Queue
    let queue_len = state
        .control_point
        .get_queue_snapshot(&rid)
        .ok()
        .map(|q| q.len())
        .unwrap_or(0);

    // Playlist binding
    let attached_playlist = state
        .control_point
        .current_queue_playlist_binding(&rid)
        .map(|(server_id, container_id, has_seen_update)| AttachedPlaylistInfo {
            server_id: server_id.0,
            container_id,
            has_seen_update,
        });

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
        (status = 200, description = "Queue du renderer", body = QueueSnapshot),
        (status = 404, description = "Renderer non trouvé", body = ErrorResponse)
    ),
    tag = "control"
)]
async fn get_renderer_queue(
    State(state): State<ControlPointState>,
    Path(renderer_id): Path<String>,
) -> Result<Json<QueueSnapshot>, (StatusCode, Json<ErrorResponse>)> {
    let rid = RendererId(renderer_id.clone());

    let items = state
        .control_point
        .get_queue_snapshot(&rid)
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Failed to get queue: {}", e),
                }),
            )
        })?;

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
        .map(|(server_id, container_id, has_seen_update)| AttachedPlaylistInfo {
            server_id: server_id.0,
            container_id,
            has_seen_update,
        });

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

    renderer.play().map_err(|e| {
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

    renderer.pause().map_err(|e| {
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

    renderer.stop().map_err(|e| {
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

    state
        .control_point
        .play_next_from_queue(&rid)
        .map_err(|e| {
            warn!("Failed to advance queue for renderer {}: {}", renderer_id, e);
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

    renderer.set_volume(req.volume as u16).map_err(|e| {
        warn!("Failed to set volume for renderer {}: {}", renderer_id, e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to set volume: {}", e),
            }),
        )
    })?;

    Ok(Json(SuccessResponse {
        message: format!("Volume set to {}", req.volume),
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

    let current = renderer.volume().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to get current volume: {}", e),
            }),
        )
    })?;

    let new_volume = (current + 5).min(100);
    renderer.set_volume(new_volume).map_err(|e| {
        warn!("Failed to increase volume for renderer {}: {}", renderer_id, e);
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

    let current = renderer.volume().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to get current volume: {}", e),
            }),
        )
    })?;

    let new_volume = current.saturating_sub(5);
    renderer.set_volume(new_volume).map_err(|e| {
        warn!("Failed to decrease volume for renderer {}: {}", renderer_id, e);
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

    let current_mute = renderer.mute().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to get current mute state: {}", e),
            }),
        )
    })?;

    let new_mute = !current_mute;
    renderer.set_mute(new_mute).map_err(|e| {
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

    state
        .control_point
        .attach_queue_to_playlist(&rid, sid, req.container_id.clone());

    debug!(
        renderer = renderer_id.as_str(),
        server = req.server_id.as_str(),
        container = req.container_id.as_str(),
        "Playlist attached via HTTP API"
    );

    Ok(Json(SuccessResponse {
        message: format!(
            "Playlist {} attached to renderer",
            req.container_id
        ),
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
async fn list_servers(
    State(state): State<ControlPointState>,
) -> Json<Vec<MediaServerSummary>> {
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

    let server_info = state
        .control_point
        .media_server(&sid)
        .ok_or_else(|| {
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

    let music_server = MusicServer::from_info(&server_info, Duration::from_secs(10)).map_err(
        |e| {
            warn!("Failed to create MusicServer for {}: {}", server_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to initialize server: {}", e),
                }),
            )
        },
    )?;

    let entries = music_server
        .browse_children(&container_id, 0, 100)
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
fn protocol_to_string(protocol: &RendererProtocol) -> String {
    match protocol {
        RendererProtocol::UpnpAvOnly => "UpnpAvOnly".to_string(),
        RendererProtocol::OpenHomeOnly => "OpenHomeOnly".to_string(),
        RendererProtocol::Hybrid => "Hybrid".to_string(),
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
        .route("/renderers/{renderer_id}/binding", get(get_renderer_binding))
        // Transport control
        .route("/renderers/{renderer_id}/play", post(play_renderer))
        .route("/renderers/{renderer_id}/pause", post(pause_renderer))
        .route("/renderers/{renderer_id}/stop", post(stop_renderer))
        .route("/renderers/{renderer_id}/next", post(next_renderer))
        // Volume control
        .route("/renderers/{renderer_id}/volume/set", post(set_renderer_volume))
        .route("/renderers/{renderer_id}/volume/up", post(volume_up_renderer))
        .route("/renderers/{renderer_id}/volume/down", post(volume_down_renderer))
        .route("/renderers/{renderer_id}/mute/toggle", post(toggle_mute_renderer))
        // Playlist binding
        .route("/renderers/{renderer_id}/binding/attach", post(attach_playlist_binding))
        .route("/renderers/{renderer_id}/binding/detach", post(detach_playlist_binding))
        // Servers
        .route("/servers", get(list_servers))
        .route("/servers/{server_id}/containers/{container_id}", get(browse_container))
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
    /// Initialise l'API Control Point
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
    /// * `control_point` - Instance du ControlPoint
    async fn init_control_point(&mut self, control_point: Arc<ControlPoint>);
}

#[cfg(feature = "pmoserver")]
#[async_trait]
impl ControlPointExt for pmoserver::Server {
    async fn init_control_point(&mut self, control_point: Arc<ControlPoint>) {
        let state = ControlPointState::new(control_point.clone());

        // Créer le router API (inclut REST et SSE)
        let api_router = create_api_router(state, control_point);

        // L'enregistrer avec OpenAPI
        self.add_openapi(api_router, crate::openapi::ApiDoc::openapi(), "control")
            .await;
    }
}
