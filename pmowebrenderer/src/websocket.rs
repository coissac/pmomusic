//! Handler WebSocket pour les connexions navigateur → WebRenderer

use std::sync::Arc;
use std::time::SystemTime;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use parking_lot::RwLock;
use tokio::sync::mpsc;
use uuid::Uuid;

use pmoupnp::devices::DeviceInstance;
use pmoupnp::variable_types::StateValue;
#[cfg(not(feature = "pmoserver"))]
use pmoupnp::UpnpModel;
use pmoupnp::UpnpTypedInstance;

use crate::messages::*;
use crate::renderer::WebRendererFactory;
use crate::session::{SessionManager, WebRendererSession};
use crate::state::{RendererState, SharedState};

#[cfg(feature = "pmoserver")]
use pmocontrol::model::{RendererCapabilities, RendererProtocol};
#[cfg(feature = "pmoserver")]
use pmocontrol::{ControlPoint, DeviceId};

/// État partagé du serveur WebSocket
#[derive(Clone)]
pub struct WebSocketState {
    pub session_manager: Arc<SessionManager>,
    #[cfg(feature = "pmoserver")]
    pub control_point: Arc<ControlPoint>,
}

/// Handler pour la connexion WebSocket
pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<WebSocketState>,
) -> impl IntoResponse {
    tracing::info!("WebRenderer WebSocket upgrade request received");
    ws.on_upgrade(move |socket: WebSocket| handle_socket(socket, state))
}

/// Gestion de la connexion WebSocket
async fn handle_socket(socket: WebSocket, state: WebSocketState) {
    tracing::info!("WebRenderer WebSocket connection established");
    let (mut sink, mut stream) = socket.split();

    // Canal pour envoyer des messages au navigateur
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerMessage>();

    // Task pour envoyer les messages du canal vers le WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&msg) {
                if sink.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
        }
    });

    let mut session_token: Option<String> = None;
    #[allow(unused_variables, unused_assignments, unused_mut)]
    let mut device_udn: Option<String> = None;

    // Boucle de réception des messages du navigateur
    while let Some(msg_result) = stream.next().await {
        match msg_result {
            Ok(Message::Text(text)) => {
                tracing::info!("WebRenderer received text message: {}", &text);
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(ClientMessage::Init { capabilities }) => {
                        tracing::info!("WebRenderer Init received, creating renderer...");
                        // Créer ou reconnecter le renderer UPnP pour ce navigateur.
                        match create_renderer_for_browser(&capabilities, tx.clone(), &state).await {
                            Ok(session) => {
                                tracing::info!("WebRenderer create_renderer_for_browser OK");
                                let token = session.token.clone();
                                let udn = session.udn.clone();

                                let model = session.device_instance.get_model();

                                // Envoyer la confirmation au navigateur
                                let _ = tx.send(ServerMessage::SessionCreated {
                                    token: token.clone(),
                                    renderer_info: RendererInfo {
                                        udn: udn.clone(),
                                        friendly_name: model.friendly_name().to_string(),
                                        model_name: model.model_name().to_string(),
                                        description_url: format!(
                                            "{}{}",
                                            session.device_instance.base_url(),
                                            session.device_instance.description_route()
                                        ),
                                    },
                                });

                                // Si une URI est déjà chargée (reconnexion en cours de lecture),
                                // envoyer l'état complet pour que le navigateur puisse reprendre.
                                {
                                    let s = session.state.read();
                                    if s.current_uri.is_some() {
                                        let _ = tx.send(ServerMessage::StateSync {
                                            current_uri: s.current_uri.clone(),
                                            current_metadata: s.current_metadata.clone(),
                                            next_uri: s.next_uri.clone(),
                                            next_metadata: s.next_metadata.clone(),
                                            playback_state: s.playback_state.clone(),
                                            position: s.position.clone(),
                                            volume: s.volume,
                                            mute: s.mute,
                                        });
                                        tracing::info!(
                                            udn = %udn,
                                            state = ?s.playback_state,
                                            "WebRenderer: sent StateSync to reconnected browser"
                                        );
                                    }
                                }

                                session_token = Some(token.clone());
                                #[cfg(feature = "pmoserver")]
                                { device_udn = Some(udn.clone()); }

                                state.session_manager.add_session(session);

                                tracing::info!(
                                    token = %token,
                                    udn = %udn,
                                    "WebRenderer initialized for browser: {}",
                                    capabilities.user_agent
                                );
                            }
                            Err(e) => {
                                tracing::error!("Failed to create WebRenderer: {:?}", e);
                                break;
                            }
                        }
                    }
                    Ok(ClientMessage::StateUpdate { state: new_state }) => {
                        if let Some(ref token) = session_token {
                            if let Some(session) = state.session_manager.get_session(token) {
                                {
                                    session.state.write().playback_state = new_state.clone();
                                }
                                update_transport_state_var(&session.device_instance, &new_state)
                                    .await;
                            }
                        }
                    }
                    Ok(ClientMessage::PositionUpdate { position, duration }) => {
                        if let Some(ref token) = session_token {
                            if let Some(session) = state.session_manager.get_session(token) {
                                {
                                    let mut s = session.state.write();
                                    s.position = Some(position.clone());
                                    s.duration = Some(duration.clone());
                                }
                                update_position_vars(
                                    &session.device_instance,
                                    &position,
                                    &duration,
                                )
                                .await;
                            }
                        }
                    }
                    Ok(ClientMessage::MetadataUpdate { metadata }) => {
                        if let Some(ref token) = session_token {
                            if let Some(session) = state.session_manager.get_session(token) {
                                let didl = build_didl_from_metadata(&metadata);
                                {
                                    session.state.write().current_metadata = Some(didl.clone());
                                }
                                update_metadata_var(&session.device_instance, &didl).await;
                            }
                        }
                    }
                    Ok(ClientMessage::VolumeUpdate { volume, mute }) => {
                        if let Some(ref token) = session_token {
                            if let Some(session) = state.session_manager.get_session(token) {
                                {
                                    let mut s = session.state.write();
                                    s.volume = volume;
                                    s.mute = mute;
                                }
                                update_volume_vars(&session.device_instance, volume, mute).await;
                            }
                        }
                    }
                    Ok(ClientMessage::TrackEnded) => {
                        if let Some(ref token) = session_token {
                            if let Some(session) = state.session_manager.get_session(token) {
                                let (next_uri, next_metadata, had_next) = {
                                    let mut s = session.state.write();
                                    let uri = s.next_uri.take();
                                    let meta = s.next_metadata.take();
                                    let had_next = uri.is_some();
                                    s.current_uri = uri.clone();
                                    s.current_metadata = meta.clone();
                                    s.next_uri = None;
                                    s.next_metadata = None;
                                    s.position = None;
                                    s.duration = None;
                                    // Si on avait une piste suivante (gapless), on reste en Playing.
                                    // Sinon, on reste en Stopped pour que le watcher déclenche l'auto-advance.
                                    if had_next {
                                        s.playback_state = PlaybackState::Playing;
                                    }
                                    // Si had_next == false, le navigateur a déjà envoyé state_update:STOPPED,
                                    // donc s.playback_state est déjà Stopped. On le laisse tel quel.
                                    (uri, meta, had_next)
                                };
                                let new_state = if had_next {
                                    PlaybackState::Playing
                                } else {
                                    PlaybackState::Stopped
                                };
                                update_transport_state_var(
                                    &session.device_instance,
                                    &new_state,
                                )
                                .await;
                                // Mettre à jour AVTransportURI pour que le ControlPoint
                                // voie la nouvelle piste courante et envoie SetNextAVTransportURI
                                update_uri_vars(
                                    &session.device_instance,
                                    next_uri.as_deref().unwrap_or(""),
                                    next_metadata.as_deref().unwrap_or(""),
                                    "",  // next_uri vide : le ControlPoint le remplira
                                    "",
                                )
                                .await;
                                // Si c'était une transition gapless (on avait une piste suivante),
                                // avancer l'index de la queue dans le ControlPoint et prefetch la piste N+2.
                                // Si pas de piste suivante, le watcher verra STOPPED et déclenchera l'auto-advance.
                                #[cfg(feature = "pmoserver")]
                                if had_next {
                                    let udn = session.udn.clone();
                                    let cp = state.control_point.clone();
                                    tokio::spawn(async move {
                                        cp.advance_queue_and_prefetch(
                                            &pmocontrol::DeviceId(udn),
                                        );
                                    });
                                }
                                tracing::debug!(
                                    uri = ?next_uri,
                                    had_next,
                                    "WebRenderer TrackEnded: advanced to next track"
                                );
                            }
                        }
                    }
                    Ok(ClientMessage::Pong) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to parse client message");
                    }
                }
            }
            Ok(Message::Binary(b)) => {
                tracing::warn!("WebRenderer received binary message ({} bytes)", b.len());
            }
            Ok(Message::Close(_)) => {
                tracing::info!("WebRenderer WebSocket closed by client");
                break;
            }
            Err(e) => {
                tracing::error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    // Cleanup à la déconnexion
    tracing::info!("WebRenderer WebSocket handler exiting (session_token={:?})", session_token);
    if let Some(token) = session_token {
        state.session_manager.remove_session(&token);
    }

    // Marquer le renderer comme offline dans le ControlPoint
    #[cfg(feature = "pmoserver")]
    if let Some(ref udn) = device_udn {
        if let Ok(mut registry) = state.control_point.registry().write() {
            registry.device_says_byebye(udn);
        }
        tracing::info!(udn = %udn, "WebRenderer disconnected and marked offline");
    }

    send_task.abort();
}

/// Crée ou reconnecte un DeviceInstance UPnP pour un navigateur.
///
/// - Première connexion : crée le device, l'enregistre, crée la session.
/// - Reconnexion (reload) : retrouve la session existante par UDN, met à jour le
///   `SharedSender` avec le nouveau tx WebSocket (les handlers continuent de fonctionner),
///   et crée une nouvelle session avec un nouveau token.
async fn create_renderer_for_browser(
    capabilities: &BrowserCapabilities,
    ws_sender: mpsc::UnboundedSender<ServerMessage>,
    ws_state: &WebSocketState,
) -> Result<Arc<WebRendererSession>, crate::error::WebRendererError> {
    let token = Uuid::new_v4().to_string();

    // Persister l'UDN dérivé de l'instance_id dans la config pour que device_instance.rs
    // le retrouve de façon déterministe. La clé ("MediaRenderer", instance_id) est unique
    // par onglet/navigateur et stable entre les reloads.
    let instance_udn = capabilities.instance_id.clone();
    if let Err(e) = pmoconfig::get_config().set_device_udn(
        "MediaRenderer",
        &instance_udn,
        instance_udn.clone(),
    ) {
        tracing::warn!("WebRenderer: failed to persist UDN in config: {:?}", e);
    }

    // UDN normalisé tel que stocké dans le DEVICE_REGISTRY (sans préfixe "uuid:")
    let candidate_udn = instance_udn.to_ascii_lowercase();
    // UDN avec préfixe "uuid:" pour le ControlPoint et la session
    let full_udn = format!("uuid:{}", candidate_udn);

    // ── Reconnexion : session existante par UDN ───────────────────────────────
    // Si une session avec ce même UDN existe encore dans le SessionManager, on
    // met à jour son SharedSender (les handlers UPnP enverront vers le nouveau WS).
    if let Some(existing_session) = ws_state.session_manager.get_session_by_udn(&full_udn) {
        tracing::info!(udn = %full_udn, "WebRenderer: reconnecting via existing session");
        existing_session.shared_sender.set(ws_sender.clone());

        #[cfg(feature = "pmoserver")]
        register_with_control_point(&existing_session.device_instance, ws_state)?;

        // Nouvelle session avec nouveau token, mais même device/state/sender partagés
        let session = Arc::new(WebRendererSession {
            token,
            udn: full_udn,
            device_instance: existing_session.device_instance.clone(),
            shared_sender: existing_session.shared_sender.clone(),
            state: existing_session.state.clone(),
            created_at: existing_session.created_at,
            last_activity: existing_session.last_activity.clone(),
        });
        return Ok(session);
    }

    // ── Première connexion : création complète ────────────────────────────────

    // Enregistrer le device via UpnpServerExt (gère base_url, register_urls, DEVICE_REGISTRY)
    // Retourne (DeviceInstance, SharedSender effectif, SharedState effective pour cette session)
    #[cfg(feature = "pmoserver")]
    let (di, shared_sender, shared_state) = {
        use pmoupnp::UpnpServerExt;

        tracing::info!("WebRenderer: candidate UDN = {}", candidate_udn);

        // Vérifier si un device avec ce même UDN est déjà dans le DEVICE_REGISTRY
        // (cas où la session a expiré mais le device est encore enregistré).
        let server_arc =
            pmoserver::get_server().ok_or(crate::error::WebRendererError::ServerNotAvailable)?;
        let existing_di = {
            let server = server_arc.read().await;
            server.get_device(&candidate_udn)
        };

        if let Some(di) = existing_di {
            tracing::info!(udn = %candidate_udn, "WebRenderer: reusing device from registry (session expired)");
            // Mettre à jour le SharedSender de ce device (session supprimée mais device toujours dans registry).
            // Le SharedSender et le SharedState sont ceux capturés dans les handlers du di existant.
            let effective_sender = if let Some(existing_sender) = ws_state.session_manager.get_sender_by_udn(&full_udn) {
                existing_sender.set(ws_sender);
                tracing::info!(udn = %full_udn, "WebRenderer: updated SharedSender for reused device");
                existing_sender
            } else {
                // Fallback : ne devrait pas arriver mais on crée un sender neuf
                tracing::warn!(udn = %full_udn, "WebRenderer: no SharedSender found for reused device");
                let new_state: SharedState = Arc::new(RwLock::new(RendererState::default()));
                let (_, new_sender) = WebRendererFactory::create_device_with_name(
                    &instance_udn, &capabilities.user_agent, ws_sender, new_state.clone(),
                ).map_err(|e| crate::error::WebRendererError::DeviceCreationError(e.to_string()))?;
                new_sender
            };
            let effective_state = ws_state.session_manager.get_state_by_udn(&full_udn)
                .unwrap_or_else(|| Arc::new(RwLock::new(RendererState::default())));
            register_with_control_point(&di, ws_state)?;
            (di, effective_sender, effective_state)
        } else {
            // Véritablement première connexion : créer device + state + sender
            let new_state: SharedState = Arc::new(RwLock::new(RendererState::default()));
            tracing::info!("WebRenderer: creating device model...");
            let (device, new_sender) = WebRendererFactory::create_device_with_name(
                &instance_udn,
                &capabilities.user_agent,
                ws_sender,
                new_state.clone(),
            )
            .map_err(|e| crate::error::WebRendererError::DeviceCreationError(e.to_string()))?;
            tracing::info!("WebRenderer: device model created");

            let device = Arc::new(device);
            tracing::info!("WebRenderer: registering new device...");
            let di = {
                let mut server = server_arc.write().await;
                server
                    .register_device(device)
                    .await
                    .map_err(|e| crate::error::WebRendererError::RegistrationError(e.to_string()))?
            };
            tracing::info!("WebRenderer: device registered");
            register_with_control_point(&di, ws_state)?;
            (di, new_sender, new_state)
        }
    };

    #[cfg(not(feature = "pmoserver"))]
    let (di, shared_sender, shared_state) = {
        let new_state: SharedState = Arc::new(RwLock::new(RendererState::default()));
        let (device, new_sender) = WebRendererFactory::create_device_with_name(
            &instance_udn,
            &capabilities.user_agent,
            ws_sender,
            new_state.clone(),
        )
        .map_err(|e| crate::error::WebRendererError::DeviceCreationError(e.to_string()))?;
        (Arc::new(device).create_instance(), new_sender, new_state)
    };

    let session = Arc::new(WebRendererSession {
        token,
        udn: full_udn,
        device_instance: di,
        shared_sender,
        state: shared_state,
        created_at: SystemTime::now(),
        last_activity: Arc::new(RwLock::new(SystemTime::now())),
    });

    Ok(session)
}

/// Enregistre le DeviceInstance auprès du ControlPoint comme un renderer
#[cfg(feature = "pmoserver")]
fn register_with_control_point(
    di: &Arc<DeviceInstance>,
    ws_state: &WebSocketState,
) -> Result<(), crate::error::WebRendererError> {
    let base_url = di.base_url().to_string();
    let udn = di.udn().to_ascii_lowercase();
    // Préfixer avec "uuid:" pour correspondre au format SSDP et éviter les doublons
    let udn_with_prefix = format!("uuid:{}", udn);
    let device_route = di.route();
    let model = di.get_model();

    let avtransport_control_url = Some(format!(
        "{}{}/service/AVTransport/control",
        base_url, device_route
    ));
    let rendering_control_url = Some(format!(
        "{}{}/service/RenderingControl/control",
        base_url, device_route
    ));
    let connection_manager_url = Some(format!(
        "{}{}/service/ConnectionManager/control",
        base_url, device_route
    ));

    let renderer_info = pmocontrol::RendererInfo::make(
        DeviceId(udn_with_prefix.clone()),
        udn_with_prefix.clone(),
        model.friendly_name().to_string(),
        model.model_name().to_string(),
        "PMOMusic".to_string(),
        RendererProtocol::UpnpAvOnly,
        RendererCapabilities {
            has_avtransport: true,
            has_avtransport_set_next: true,
            has_rendering_control: true,
            has_connection_manager: true,
            ..Default::default()
        },
        format!("{}{}", base_url, di.description_route()),
        "PMOMusic WebRenderer/1.0".to_string(),
        Some("urn:schemas-upnp-org:service:AVTransport:1".to_string()),
        avtransport_control_url,
        Some("urn:schemas-upnp-org:service:RenderingControl:1".to_string()),
        rendering_control_url,
        Some("urn:schemas-upnp-org:service:ConnectionManager:1".to_string()),
        connection_manager_url,
        None, // oh_playlist_service_type
        None, // oh_playlist_control_url
        None, // oh_playlist_event_sub_url
        None, // oh_info_service_type
        None, // oh_info_control_url
        None, // oh_info_event_sub_url
        None, // oh_time_service_type
        None, // oh_time_control_url
        None, // oh_time_event_sub_url
        None, // oh_volume_service_type
        None, // oh_volume_control_url
        None, // oh_radio_service_type
        None, // oh_radio_control_url
        None, // oh_product_service_type
        None, // oh_product_control_url
    );

    if let Ok(mut registry) = ws_state.control_point.registry().write() {
        // max_age élevé car pas de SSDP — cleanup à la déconnexion WS
        registry.push_renderer(&renderer_info, 86400);
    }

    tracing::info!(udn = %udn, "WebRenderer registered with ControlPoint");
    Ok(())
}

// ─── Mise à jour des StateVarInstance UPnP ──────────────────────────────────

async fn update_transport_state_var(di: &DeviceInstance, state: &PlaybackState) {
    let upnp_state = match state {
        PlaybackState::Stopped => "STOPPED",
        PlaybackState::Playing => "PLAYING",
        PlaybackState::Paused => "PAUSED_PLAYBACK",
        PlaybackState::Transitioning => "TRANSITIONING",
    };

    if let Some(service) = di.get_service("AVTransport") {
        if let Some(var) = service.get_variable("TransportState") {
            let _ = var
                .set_value(StateValue::String(upnp_state.to_string()))
                .await;
        }
    }
}

async fn update_position_vars(di: &DeviceInstance, position: &str, duration: &str) {
    if let Some(service) = di.get_service("AVTransport") {
        if let Some(var) = service.get_variable("RelativeTimePosition") {
            let _ = var
                .set_value(StateValue::String(position.to_string()))
                .await;
        }
        if let Some(var) = service.get_variable("AbsoluteTimePosition") {
            let _ = var
                .set_value(StateValue::String(position.to_string()))
                .await;
        }
        if let Some(var) = service.get_variable("CurrentTrackDuration") {
            let _ = var
                .set_value(StateValue::String(duration.to_string()))
                .await;
        }
    }
}

async fn update_uri_vars(
    di: &DeviceInstance,
    current_uri: &str,
    current_metadata: &str,
    next_uri: &str,
    next_metadata: &str,
) {
    if let Some(service) = di.get_service("AVTransport") {
        if let Some(var) = service.get_variable("AVTransportURI") {
            let _ = var
                .set_value(StateValue::String(current_uri.to_string()))
                .await;
        }
        if let Some(var) = service.get_variable("AVTransportURIMetaData") {
            let _ = var
                .set_value(StateValue::String(current_metadata.to_string()))
                .await;
        }
        if let Some(var) = service.get_variable("AVTransportNextURI") {
            let _ = var
                .set_value(StateValue::String(next_uri.to_string()))
                .await;
        }
        if let Some(var) = service.get_variable("AVTransportNextURIMetaData") {
            let _ = var
                .set_value(StateValue::String(next_metadata.to_string()))
                .await;
        }
    }
}

async fn update_metadata_var(di: &DeviceInstance, didl: &str) {
    if let Some(service) = di.get_service("AVTransport") {
        if let Some(var) = service.get_variable("CurrentTrackMetaData") {
            let _ = var.set_value(StateValue::String(didl.to_string())).await;
        }
    }
}

async fn update_volume_vars(di: &DeviceInstance, volume: u16, mute: bool) {
    if let Some(service) = di.get_service("RenderingControl") {
        if let Some(var) = service.get_variable("Volume") {
            let _ = var.set_value(StateValue::UI2(volume)).await;
        }
        if let Some(var) = service.get_variable("Mute") {
            let _ = var.set_value(StateValue::Boolean(mute)).await;
        }
    }
}

fn build_didl_from_metadata(metadata: &TrackMetadata) -> String {
    use pmodidl::{DIDLLite, Item, Resource};
    use pmoutils::ToXmlElement;

    let item = Item {
        id: "0".to_string(),
        parent_id: "-1".to_string(),
        restricted: Some("1".to_string()),
        title: metadata
            .title
            .clone()
            .unwrap_or_else(|| "Unknown".to_string()),
        creator: None,
        class: "object.item.audioItem.musicTrack".to_string(),
        artist: metadata.artist.clone(),
        album: metadata.album.clone(),
        genre: None,
        album_art: metadata.album_art_uri.clone(),
        album_art_pk: None,
        date: None,
        original_track_number: None,
        resources: vec![Resource {
            protocol_info: "http-get:*:audio/*:*".to_string(),
            duration: metadata.duration.clone(),
            url: "".to_string(),
            bits_per_sample: None,
            sample_frequency: None,
            nr_audio_channels: None,
        }],
        descriptions: vec![],
    };

    let didl = DIDLLite {
        items: vec![item],
        ..Default::default()
    };

    didl.to_xml()
}
