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
    http::header::HeaderMap,
    response::IntoResponse,
    response::sse::{Event, KeepAlive, Sse},
};
#[cfg(feature = "pmoserver")]
use serde::Serialize;
#[cfg(feature = "pmoserver")]
use std::sync::Arc;

#[cfg(feature = "pmoserver")]
use pmocovers;

use crate::{DeviceIdentity, DeviceOnline};
use tracing::error;

// ============================================================================
// HELPERS - Transformation des URLs de covers LAN externes
// ============================================================================

/// Transforme une URL de cover pour qu'elle soit accessible depuis le client
///
/// Si l'URL est une route locale de notre cache (/covers/...), on la transforme en URL absolue.
/// Sinon, on utilise pmocovers::proxy_cover_url() pour mettre en cache et retourner notre URL.
#[cfg(feature = "pmoserver")]
async fn transform_cover_url(url: Option<&str>, base_url: &pmoserver::BaseUrl) -> Option<String> {
    let url = url?;
    
    // Si c'est déjà une route locale de notre cache, la transformer en URL absolue
    if url.starts_with("/covers/") {
        return Some(base_url.url_for(url));
    }
    
    // Si c'est une URL de notre instance, la retourner directement
    if url.starts_with(&base_url.0) {
        return Some(url.to_string());
    }
    
    // Pour les autres URLs, utiliser le mechanisme de proxy standard
    match pmocovers::proxy_cover_url(url, base_url).await {
        Ok(local_url) => Some(local_url),
        Err(e) => {
            tracing::warn!("Failed to proxy cover URL {}: {}", url, e);
            Some(url.to_string())
        }
    }
}

/// Transforme une URL de cover pour qu'elle soit accessible depuis le client (version synchrone)
///
/// Utilise un thread séparé avec son propre runtime tokio.
#[cfg(feature = "pmoserver")]
fn transform_cover_url_sync(url: Option<&str>, base_url: &pmoserver::BaseUrl) -> Option<String> {
    let url = url?;
    
    // Si c'est déjà une route locale de notre cache, la transformer en URL absolue
    if url.starts_with("/covers/") {
        return Some(base_url.url_for(url));
    }
    
    // Si c'est une URL de notre instance, la retourner directement
    if url.starts_with(&base_url.0) {
        return Some(url.to_string());
    }
    
    // Pour les autres URLs, utiliser un thread avec runtime
    let url_owned = url.to_string();
    let base_url_owned = base_url.0.clone();
    
    let result = std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async move {
            pmocovers::proxy_cover_url(&url_owned, &pmoserver::BaseUrl(base_url_owned)).await
        })
    }).join();
    
    match result {
        Ok(Ok(local_url)) => Some(local_url),
        _ => Some(url.to_string())
    }
}

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
    QueueRefreshing {
        renderer_id: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    QueueReadyToPlay {
        renderer_id: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    QueueSyncCancelled {
        renderer_id: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    BindingChanged {
        renderer_id: String,
        server_id: Option<String>,
        container_id: Option<String>,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    StreamStateChanged {
        renderer_id: String,
        is_stream: bool,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    TimerStarted {
        renderer_id: String,
        duration_seconds: u32,
        remaining_seconds: u32,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    TimerUpdated {
        renderer_id: String,
        duration_seconds: u32,
        remaining_seconds: u32,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    TimerTick {
        renderer_id: String,
        remaining_seconds: u32,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    TimerExpired {
        renderer_id: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    TimerCancelled {
        renderer_id: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    Online {
        renderer_id: String,
        friendly_name: String,
        model_name: String,
        manufacturer: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    Offline {
        renderer_id: String,
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
    Online {
        server_id: String,
        friendly_name: String,
        model_name: String,
        manufacturer: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    Offline {
        server_id: String,
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
// HELPER FUNCTIONS
// ============================================================================

/// Convertit un RendererEvent en RendererEventPayload pour SSE
///
/// Cette fonction centralise la conversion pour éviter la duplication de code
/// entre les différents streams SSE (renderers-only et all-events).
#[cfg(feature = "pmoserver")]
async fn renderer_event_to_payload(
    event: RendererEvent,
    timestamp: chrono::DateTime<chrono::Utc>,
    base_url: &pmoserver::BaseUrl,
) -> RendererEventPayload {
    match event {
        RendererEvent::StateChanged { id, state } => RendererEventPayload::StateChanged {
            renderer_id: id.0,
            state: state.as_str().to_string(),
            timestamp,
        },
        RendererEvent::PositionChanged { id, position } => RendererEventPayload::PositionChanged {
            renderer_id: id.0,
            track: position.track,
            rel_time: position.rel_time,
            track_duration: position.track_duration,
            timestamp,
        },
        RendererEvent::VolumeChanged { id, volume } => RendererEventPayload::VolumeChanged {
            renderer_id: id.0,
            volume,
            timestamp,
        },
        RendererEvent::MuteChanged { id, mute } => RendererEventPayload::MuteChanged {
            renderer_id: id.0,
            mute,
            timestamp,
        },
        RendererEvent::MetadataChanged { id, metadata } => RendererEventPayload::MetadataChanged {
            renderer_id: id.0,
            title: metadata.title,
            artist: metadata.artist,
            album: metadata.album,
            album_art_uri: transform_cover_url(metadata.album_art_uri.as_deref(), base_url).await,
            timestamp,
        },
        RendererEvent::QueueUpdated { id, queue_length } => RendererEventPayload::QueueUpdated {
            renderer_id: id.0,
            queue_length,
            timestamp,
        },
        RendererEvent::QueueRefreshing { id } => RendererEventPayload::QueueRefreshing {
            renderer_id: id.0,
            timestamp,
        },
        RendererEvent::QueueReadyToPlay { id } => RendererEventPayload::QueueReadyToPlay {
            renderer_id: id.0,
            timestamp,
        },
        RendererEvent::QueueSyncCancelled { id } => RendererEventPayload::QueueSyncCancelled {
            renderer_id: id.0,
            timestamp,
        },
        RendererEvent::BindingChanged { id, binding } => RendererEventPayload::BindingChanged {
            renderer_id: id.0,
            server_id: binding.as_ref().map(|b| b.server_id.0.clone()),
            container_id: binding.as_ref().map(|b| b.container_id.clone()),
            timestamp,
        },
        RendererEvent::StreamStateChanged { id, is_stream } => {
            RendererEventPayload::StreamStateChanged {
                renderer_id: id.0,
                is_stream,
                timestamp,
            }
        }
        RendererEvent::TimerStarted {
            id,
            duration_seconds,
            remaining_seconds,
        } => RendererEventPayload::TimerStarted {
            renderer_id: id.0,
            duration_seconds,
            remaining_seconds,
            timestamp,
        },
        RendererEvent::TimerUpdated {
            id,
            duration_seconds,
            remaining_seconds,
        } => RendererEventPayload::TimerUpdated {
            renderer_id: id.0,
            duration_seconds,
            remaining_seconds,
            timestamp,
        },
        RendererEvent::TimerTick {
            id,
            remaining_seconds,
        } => RendererEventPayload::TimerTick {
            renderer_id: id.0,
            remaining_seconds,
            timestamp,
        },
        RendererEvent::TimerExpired { id } => RendererEventPayload::TimerExpired {
            renderer_id: id.0,
            timestamp,
        },
        RendererEvent::TimerCancelled { id } => RendererEventPayload::TimerCancelled {
            renderer_id: id.0,
            timestamp,
        },
        RendererEvent::Online { id, info } => RendererEventPayload::Online {
            renderer_id: id.0,
            friendly_name: info.friendly_name,
            model_name: info.model_name,
            manufacturer: info.manufacturer,
            timestamp,
        },
        RendererEvent::Offline { id } => RendererEventPayload::Offline {
            renderer_id: id.0,
            timestamp,
        },
    }
}

/// Convertit un MediaServerEvent en MediaServerEventPayload pour SSE
///
/// Cette fonction centralise la conversion pour éviter la duplication de code
/// entre les différents streams SSE (servers-only et all-events).
#[cfg(feature = "pmoserver")]
async fn media_server_event_to_payload(
    event: MediaServerEvent,
    timestamp: chrono::DateTime<chrono::Utc>,
) -> MediaServerEventPayload {
    match event {
        MediaServerEvent::GlobalUpdated {
            server_id,
            system_update_id,
        } => MediaServerEventPayload::GlobalUpdated {
            server_id: server_id.0,
            system_update_id,
            timestamp,
        },
        MediaServerEvent::ContainersUpdated {
            server_id,
            container_ids,
        } => MediaServerEventPayload::ContainersUpdated {
            server_id: server_id.0,
            container_ids,
            timestamp,
        },
        MediaServerEvent::Online { server_id, info } => MediaServerEventPayload::Online {
            server_id: server_id.0,
            friendly_name: info.friendly_name,
            model_name: info.model_name,
            manufacturer: info.manufacturer,
            timestamp,
        },
        MediaServerEvent::Offline { server_id } => MediaServerEventPayload::Offline {
            server_id: server_id.0,
            timestamp,
        },
    }
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
    headers: HeaderMap,
) -> impl IntoResponse {
    let base_url_str = pmoserver::get_base_url_from_request(&headers);
    let base_url = pmoserver::BaseUrl(base_url_str);
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

    let cp_for_heartbeat = control_point.clone();

    let stream = stream! {
        // INITIAL SNAPSHOT: Send Online events for all currently discovered renderers
        // This ensures clients see devices that were discovered before they connected
        let initial_renderers = {
            let registry = control_point.registry();
            let reg = registry.read().unwrap();
            match reg.list_renderers() {
                Ok(renderers) => renderers,
                Err(e) => {
                    error!("Failed to list renderers: {}", e);
                    Vec::new()
                }
            }
        };

        for renderer in initial_renderers {
            if renderer.is_online() {
                let timestamp = chrono::Utc::now();
                let payload = RendererEventPayload::Online {
                    renderer_id: renderer.id().0,
                    friendly_name: renderer.friendly_name().to_string(),
                    model_name: renderer.model_name().to_string(),
                    manufacturer: renderer.manufacturer().to_string(),
                    timestamp,
                };

                if let Ok(json) = serde_json::to_string(&payload) {
                    yield Ok::<_, axum::Error>(Event::default().event("renderer").data(json));
                }
            }
        }

        // Heartbeat interval: re-send Online events every 2 minutes for all online devices
        // This allows UIs that reconnect to quickly recover device state without waiting
        // for an actual state change event
        let mut heartbeat_interval = tokio::time::interval(tokio::time::Duration::from_secs(120));
        heartbeat_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        // Then stream future events with periodic heartbeat
        loop {
            tokio::select! {
                // Regular events from the control point
                Some(event) = rx_tokio.recv() => {
            let timestamp = chrono::Utc::now();
            let payload = renderer_event_to_payload(event, timestamp, &base_url).await;

                    if let Ok(json) = serde_json::to_string(&payload) {
                        yield Ok::<_, axum::Error>(Event::default().event("renderer").data(json));
                    }
                }

                // Periodic heartbeat: re-send Online events for all online renderers
                _ = heartbeat_interval.tick() => {
                    let heartbeat_renderers = {
                        let registry = cp_for_heartbeat.registry();
                        let reg = registry.read().unwrap();
                        match reg.list_renderers() {
                            Ok(renderers) => renderers,
                            Err(e) => {
                                error!("Failed to list renderers for heartbeat: {}", e);
                                Vec::new()
                            }
                        }
                    };

                    for renderer in heartbeat_renderers {
                        if renderer.is_online() {
                            let timestamp = chrono::Utc::now();
                            let payload = RendererEventPayload::Online {
                                renderer_id: renderer.id().0,
                                friendly_name: renderer.friendly_name().to_string(),
                                model_name: renderer.model_name().to_string(),
                                manufacturer: renderer.manufacturer().to_string(),
                                timestamp,
                            };

                            if let Ok(json) = serde_json::to_string(&payload) {
                                yield Ok::<_, axum::Error>(Event::default().event("renderer_heartbeat").data(json));
                            }
                        }
                    }
                }
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
    _headers: HeaderMap,
) -> impl IntoResponse {
    // Note: les événements media server n'ont pas de album_art_uri à transformer
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

    let cp_for_heartbeat = control_point.clone();

    let stream = stream! {
        // INITIAL SNAPSHOT: Send Online events for all currently discovered media servers
        // This ensures clients see servers that were discovered before they connected
        let initial_servers = {
            let registry = control_point.registry();
            let reg = registry.read().unwrap();
            match reg.list_servers() {
                Ok(servers) => servers,
                Err(e) => {
                    error!("Failed to list servers: {}", e);
                    Vec::new()
                }
            }
        };

        for server in initial_servers {
            if server.is_online() {
                let timestamp = chrono::Utc::now();
                let payload = MediaServerEventPayload::Online {
                    server_id: server.id().0,
                    friendly_name: server.friendly_name().to_string(),
                    model_name: server.model_name().to_string(),
                    manufacturer: server.manufacturer().to_string(),
                    timestamp,
                };

                if let Ok(json) = serde_json::to_string(&payload) {
                    yield Ok::<_, axum::Error>(Event::default().event("media_server").data(json));
                }
            }
        }

        // Heartbeat interval: re-send Online events every 2 minutes for all online servers
        let mut heartbeat_interval = tokio::time::interval(tokio::time::Duration::from_secs(120));
        heartbeat_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        // Then stream future events with periodic heartbeat
        loop {
            tokio::select! {
                // Regular events from the control point
                Some(event) = rx_tokio.recv() => {
                    let timestamp = chrono::Utc::now();
                    let payload = media_server_event_to_payload(event, timestamp).await;

                    if let Ok(json) = serde_json::to_string(&payload) {
                        yield Ok::<_, axum::Error>(Event::default().event("media_server").data(json));
                    }
                }

                // Periodic heartbeat: re-send Online events for all online servers
                _ = heartbeat_interval.tick() => {
                    let heartbeat_servers = {
                        let registry = cp_for_heartbeat.registry();
                        let reg = registry.read().unwrap();
                        match reg.list_servers() {
                            Ok(servers) => servers,
                            Err(e) => {
                                error!("Failed to list servers for heartbeat: {}", e);
                                Vec::new()
                            }
                        }
                    };

                    for server in heartbeat_servers {
                        if server.is_online() {
                            let timestamp = chrono::Utc::now();
                            let payload = MediaServerEventPayload::Online {
                                server_id: server.id().0,
                                friendly_name: server.friendly_name().to_string(),
                                model_name: server.model_name().to_string(),
                                manufacturer: server.manufacturer().to_string(),
                                timestamp,
                            };

                            if let Ok(json) = serde_json::to_string(&payload) {
                                yield Ok::<_, axum::Error>(Event::default().event("media_server_heartbeat").data(json));
                            }
                        }
                    }
                }
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
pub async fn all_events_sse(
    State(control_point): State<Arc<ControlPoint>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let base_url_str = pmoserver::get_base_url_from_request(&headers);
    let base_url = pmoserver::BaseUrl(base_url_str);
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
        // INITIAL SNAPSHOT: Send Online events for all currently discovered devices
        // This ensures clients see devices that were discovered before they connected
        let (initial_renderers, initial_servers) = {
            let registry = control_point.registry();
            let reg = registry.read().unwrap();
            let renderers = match reg.list_renderers() {
                Ok(r) => r,
                Err(e) => {
                    error!("Failed to list renderers: {}", e);
                    Vec::new()
                }
            };
            let servers = match reg.list_servers() {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to list servers: {}", e);
                    Vec::new()
                }
            };
            (renderers, servers)
        };

        // Send renderer Online events
        for renderer in initial_renderers {
            if renderer.is_online() {
                let timestamp = chrono::Utc::now();
                let renderer_payload = RendererEventPayload::Online {
                    renderer_id: renderer.id().0,
                    friendly_name: renderer.friendly_name().to_string(),
                    model_name: renderer.model_name().to_string(),
                    manufacturer: renderer.manufacturer().to_string(),
                    timestamp,
                };
                let payload = UnifiedEventPayload::Renderer(renderer_payload);

                if let Ok(json) = serde_json::to_string(&payload) {
                    yield Ok::<_, axum::Error>(Event::default().event("control").data(json));
                }
            }
        }

        // Send server Online events
        for server in initial_servers {
            if server.is_online() {
                let timestamp = chrono::Utc::now();
                let server_payload = MediaServerEventPayload::Online {
                    server_id: server.id().0,
                    friendly_name: server.friendly_name().to_string(),
                    model_name: server.model_name().to_string(),
                    manufacturer: server.manufacturer().to_string(),
                    timestamp,
                };
                let payload = UnifiedEventPayload::MediaServer(server_payload);

                if let Ok(json) = serde_json::to_string(&payload) {
                    yield Ok::<_, axum::Error>(Event::default().event("control").data(json));
                }
            }
        }

        // Then stream future events
        loop {
            tokio::select! {
                Some(event) = renderer_rx_tokio.recv() => {
                    let timestamp = chrono::Utc::now();
                    let renderer_payload = renderer_event_to_payload(event, timestamp, &base_url).await;

                    let payload = UnifiedEventPayload::Renderer(renderer_payload);

                    if let Ok(json) = serde_json::to_string(&payload) {
                        yield Ok::<_, axum::Error>(Event::default().event("control").data(json));
                    }
                }
                Some(event) = server_rx_tokio.recv() => {
                    let timestamp = chrono::Utc::now();
                    let server_payload = media_server_event_to_payload(event, timestamp).await;
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
