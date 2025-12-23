//! SSE endpoints pour les événements du Control Point
//!
//! Ce module fournit des endpoints Server-Sent Events pour permettre aux clients
//! web de recevoir en temps réel :
//! - Les événements des renderers (state, volume, position, queue, etc.)
//! - Les événements des serveurs de médias (global updates, container updates)
//!
//! Routes:
//! - GET /api/control/events/renderers - Événements renderers uniquement
//! - GET /api/control/events/servers - Événements serveurs uniquement
//! - GET /api/control/events - Tous les événements (agrégés)
//!
//! ⚠️ Les payloads SSE servent uniquement de signaux de rafraîchissement :
//! l'UI doit toujours refetch l'instantané complet auprès du ControlPoint,
//! seule source de vérité de l'état renderer.

#[cfg(feature = "pmoserver")]
use crate::control_point::ControlPoint;
#[cfg(feature = "pmoserver")]
use crate::model::{MediaServerEvent, RendererEvent};
#[cfg(feature = "pmoserver")]
use async_stream::stream;
#[cfg(feature = "pmoserver")]
use axum::{
    Router,
    extract::State,
    response::IntoResponse,
    response::sse::{Event, KeepAlive, Sse},
};
#[cfg(feature = "pmoserver")]
use serde::Serialize;
#[cfg(feature = "pmoserver")]
use std::sync::Arc;

// ============================================================================
// PAYLOADS SSE
// ============================================================================

/// Payload SSE pour un événement renderer
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RendererEventPayload {
    StateChanged {
        renderer_id: String,
        state: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    PositionChanged {
        renderer_id: String,
        track: Option<u32>,
        rel_time: Option<String>,
        track_duration: Option<String>,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    VolumeChanged {
        renderer_id: String,
        volume: u16,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    MuteChanged {
        renderer_id: String,
        mute: bool,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    MetadataChanged {
        renderer_id: String,
        title: Option<String>,
        artist: Option<String>,
        album: Option<String>,
        album_art_uri: Option<String>,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    QueueUpdated {
        renderer_id: String,
        queue_length: usize,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    BindingChanged {
        renderer_id: String,
        server_id: Option<String>,
        container_id: Option<String>,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
}

/// Payload SSE pour un événement serveur de médias
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MediaServerEventPayload {
    GlobalUpdated {
        server_id: String,
        system_update_id: Option<u32>,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    ContainersUpdated {
        server_id: String,
        container_ids: Vec<String>,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
}

/// Payload SSE unifié pour tous les événements
#[cfg(feature = "pmoserver")]
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "category", rename_all = "snake_case")]
pub enum UnifiedEventPayload {
    Renderer(RendererEventPayload),
    MediaServer(MediaServerEventPayload),
}

// ============================================================================
// HANDLERS SSE
// ============================================================================

/// Handler SSE pour les événements renderers
///
/// Route: GET /api/control/events/renderers
///
/// Diffuse tous les événements liés aux renderers (state, volume, position, queue, etc.)
/// en temps réel via Server-Sent Events.
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    get,
    path = "/events/renderers",
    responses(
        (status = 200, description = "Flux SSE des événements renderers", content_type = "text/event-stream")
    ),
    tag = "control"
)]
pub async fn renderer_events_sse(
    State(control_point): State<Arc<ControlPoint>>,
) -> impl IntoResponse {
    // Convert crossbeam channel to tokio channel for async compatibility
    let (tx, mut rx_tokio) = tokio::sync::mpsc::unbounded_channel();
    let rx = control_point.subscribe_events();

    // Spawn blocking task to bridge crossbeam -> tokio
    tokio::task::spawn_blocking(move || {
        while let Ok(event) = rx.recv() {
            if tx.send(event).is_err() {
                break;
            }
        }
    });

    let stream = stream! {
        while let Some(event) = rx_tokio.recv().await {
            let timestamp = chrono::Utc::now();

            let payload = match event {
                RendererEvent::StateChanged { id, state } => {
                    RendererEventPayload::StateChanged {
                        renderer_id: id.0,
                        state: state.as_str().to_string(),
                        timestamp,
                    }
                }
                RendererEvent::PositionChanged { id, position } => {
                    RendererEventPayload::PositionChanged {
                        renderer_id: id.0,
                        track: position.track,
                        rel_time: position.rel_time,
                        track_duration: position.track_duration,
                        timestamp,
                    }
                }
                RendererEvent::VolumeChanged { id, volume } => {
                    RendererEventPayload::VolumeChanged {
                        renderer_id: id.0,
                        volume,
                        timestamp,
                    }
                }
                RendererEvent::MuteChanged { id, mute } => {
                    RendererEventPayload::MuteChanged {
                        renderer_id: id.0,
                        mute,
                        timestamp,
                    }
                }
                RendererEvent::MetadataChanged { id, metadata } => {
                    RendererEventPayload::MetadataChanged {
                        renderer_id: id.0,
                        title: metadata.title,
                        artist: metadata.artist,
                        album: metadata.album,
                        album_art_uri: metadata.album_art_uri,
                        timestamp,
                    }
                }
                RendererEvent::QueueUpdated { id, queue_length } => {
                    RendererEventPayload::QueueUpdated {
                        renderer_id: id.0,
                        queue_length,
                        timestamp,
                    }
                }
                RendererEvent::BindingChanged { id, binding } => {
                    RendererEventPayload::BindingChanged {
                        renderer_id: id.0,
                        server_id: binding.as_ref().map(|b| b.server_id.0.clone()),
                        container_id: binding.as_ref().map(|b| b.container_id.clone()),
                        timestamp,
                    }
                }
            };

            if let Ok(json) = serde_json::to_string(&payload) {
                yield Ok::<_, axum::Error>(Event::default().event("renderer").data(json));
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Handler SSE pour les événements serveurs de médias
///
/// Route: GET /api/control/events/servers
///
/// Diffuse tous les événements liés aux serveurs de médias (global updates, container updates)
/// en temps réel via Server-Sent Events.
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    get,
    path = "/events/servers",
    responses(
        (status = 200, description = "Flux SSE des événements serveurs de médias", content_type = "text/event-stream")
    ),
    tag = "control"
)]
pub async fn media_server_events_sse(
    State(control_point): State<Arc<ControlPoint>>,
) -> impl IntoResponse {
    // Convert crossbeam channel to tokio channel for async compatibility
    let (tx, mut rx_tokio) = tokio::sync::mpsc::unbounded_channel();
    let rx = control_point.subscribe_media_server_events();

    // Spawn blocking task to bridge crossbeam -> tokio
    tokio::task::spawn_blocking(move || {
        while let Ok(event) = rx.recv() {
            if tx.send(event).is_err() {
                break;
            }
        }
    });

    let stream = stream! {
        while let Some(event) = rx_tokio.recv().await {
            let timestamp = chrono::Utc::now();

            let payload = match event {
                MediaServerEvent::GlobalUpdated { server_id, system_update_id } => {
                    MediaServerEventPayload::GlobalUpdated {
                        server_id: server_id.0,
                        system_update_id,
                        timestamp,
                    }
                }
                MediaServerEvent::ContainersUpdated { server_id, container_ids } => {
                    MediaServerEventPayload::ContainersUpdated {
                        server_id: server_id.0,
                        container_ids,
                        timestamp,
                    }
                }
            };

            if let Ok(json) = serde_json::to_string(&payload) {
                yield Ok::<_, axum::Error>(Event::default().event("media_server").data(json));
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Handler SSE pour tous les événements (renderers + serveurs)
///
/// Route: GET /api/control/events
///
/// Diffuse tous les événements du control point (renderers et serveurs) en temps réel.
/// Chaque événement est catégorisé et inclut un timestamp.
#[cfg(feature = "pmoserver")]
#[utoipa::path(
    get,
    path = "/events",
    responses(
        (status = 200, description = "Flux SSE de tous les événements du control point", content_type = "text/event-stream")
    ),
    tag = "control"
)]
pub async fn all_events_sse(State(control_point): State<Arc<ControlPoint>>) -> impl IntoResponse {
    // Convert crossbeam channels to tokio channels for async compatibility
    let (renderer_tx, mut renderer_rx_tokio) = tokio::sync::mpsc::unbounded_channel();
    let (server_tx, mut server_rx_tokio) = tokio::sync::mpsc::unbounded_channel();

    let renderer_rx = control_point.subscribe_events();
    let server_rx = control_point.subscribe_media_server_events();

    // Spawn blocking tasks to bridge crossbeam -> tokio
    tokio::task::spawn_blocking(move || {
        while let Ok(event) = renderer_rx.recv() {
            if renderer_tx.send(event).is_err() {
                break;
            }
        }
    });

    tokio::task::spawn_blocking(move || {
        while let Ok(event) = server_rx.recv() {
            if server_tx.send(event).is_err() {
                break;
            }
        }
    });

    let stream = stream! {
        loop {
            tokio::select! {
                Some(event) = renderer_rx_tokio.recv() => {
                    let timestamp = chrono::Utc::now();

                    let renderer_payload = match event {
                        RendererEvent::StateChanged { id, state } => {
                        RendererEventPayload::StateChanged {
                            renderer_id: id.0,
                            state: state.as_str().to_string(),
                            timestamp,
                        }
                        }
                        RendererEvent::PositionChanged { id, position } => {
                            RendererEventPayload::PositionChanged {
                                renderer_id: id.0,
                                track: position.track,
                                rel_time: position.rel_time,
                                track_duration: position.track_duration,
                                timestamp,
                            }
                        }
                        RendererEvent::VolumeChanged { id, volume } => {
                            RendererEventPayload::VolumeChanged {
                                renderer_id: id.0,
                                volume,
                                timestamp,
                            }
                        }
                        RendererEvent::MuteChanged { id, mute } => {
                            RendererEventPayload::MuteChanged {
                                renderer_id: id.0,
                                mute,
                                timestamp,
                            }
                        }
                        RendererEvent::MetadataChanged { id, metadata } => {
                            RendererEventPayload::MetadataChanged {
                                renderer_id: id.0,
                                title: metadata.title,
                                artist: metadata.artist,
                                album: metadata.album,
                                album_art_uri: metadata.album_art_uri,
                                timestamp,
                            }
                        }
                        RendererEvent::QueueUpdated { id, queue_length } => {
                            RendererEventPayload::QueueUpdated {
                                renderer_id: id.0,
                                queue_length,
                                timestamp,
                            }
                        }
                        RendererEvent::BindingChanged { id, binding } => {
                            RendererEventPayload::BindingChanged {
                                renderer_id: id.0,
                                server_id: binding.as_ref().map(|b| b.server_id.0.clone()),
                                container_id: binding.as_ref().map(|b| b.container_id.clone()),
                                timestamp,
                            }
                        }
                    };

                    let payload = UnifiedEventPayload::Renderer(renderer_payload);

                    if let Ok(json) = serde_json::to_string(&payload) {
                        yield Ok::<_, axum::Error>(Event::default().event("control").data(json));
                    }
                }
                Some(event) = server_rx_tokio.recv() => {
                    let timestamp = chrono::Utc::now();

                    let server_payload = match event {
                        MediaServerEvent::GlobalUpdated { server_id, system_update_id } => {
                            MediaServerEventPayload::GlobalUpdated {
                                server_id: server_id.0,
                                system_update_id,
                                timestamp,
                            }
                        }
                        MediaServerEvent::ContainersUpdated { server_id, container_ids } => {
                            MediaServerEventPayload::ContainersUpdated {
                                server_id: server_id.0,
                                container_ids,
                                timestamp,
                            }
                        }
                    };

                    let payload = UnifiedEventPayload::MediaServer(server_payload);

                    if let Ok(json) = serde_json::to_string(&payload) {
                        yield Ok::<_, axum::Error>(Event::default().event("control").data(json));
                    }
                }
                else => break
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ============================================================================
// ROUTER
// ============================================================================

/// Crée le router SSE pour les événements du Control Point
#[cfg(feature = "pmoserver")]
pub fn create_sse_router(control_point: Arc<ControlPoint>) -> Router {
    use axum::routing::get;

    Router::new()
        .route("/events", get(all_events_sse))
        .route("/events/renderers", get(renderer_events_sse))
        .route("/events/servers", get(media_server_events_sse))
        .with_state(control_point)
}
