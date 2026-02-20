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
    let mut device_udn: Option<String> = None;

    // Boucle de réception des messages du navigateur
    while let Some(msg_result) = stream.next().await {
        match msg_result {
            Ok(Message::Text(text)) => {
                tracing::info!("WebRenderer received text message: {}", &text);
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(ClientMessage::Init { capabilities }) => {
                        tracing::info!("WebRenderer Init received, creating renderer...");
                        // Créer le renderer UPnP pour ce navigateur
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

                                session_token = Some(token.clone());
                                device_udn = Some(udn.clone());

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
                    Ok(ClientMessage::Pong) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to parse client message");
                    }
                }
            }
            Ok(Message::Close(_)) => break,
            Err(e) => {
                tracing::error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    // Cleanup à la déconnexion
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

/// Crée un DeviceInstance UPnP et l'enregistre pour un navigateur
async fn create_renderer_for_browser(
    capabilities: &BrowserCapabilities,
    ws_sender: mpsc::UnboundedSender<ServerMessage>,
    ws_state: &WebSocketState,
) -> Result<Arc<WebRendererSession>, crate::error::WebRendererError> {
    let shared_state: SharedState = Arc::new(RwLock::new(RendererState::default()));
    let token = Uuid::new_v4().to_string();

    // Construire le Device model avec les handlers WS
    tracing::info!("WebRenderer: creating device model...");
    let device = WebRendererFactory::create_device(
        &capabilities.user_agent,
        ws_sender.clone(),
        shared_state.clone(),
    )
    .map_err(|e| crate::error::WebRendererError::DeviceCreationError(e.to_string()))?;
    tracing::info!("WebRenderer: device model created");

    let device = Arc::new(device);

    // Enregistrer le device via UpnpServerExt (gère base_url, register_urls, DEVICE_REGISTRY)
    #[cfg(feature = "pmoserver")]
    let di = {
        use pmoupnp::UpnpServerExt;

        tracing::info!("WebRenderer: getting server arc...");
        let server_arc =
            pmoserver::get_server().ok_or(crate::error::WebRendererError::ServerNotAvailable)?;
        tracing::info!("WebRenderer: got server arc, acquiring write lock...");

        let di = {
            let mut server = server_arc.write().await;
            tracing::info!("WebRenderer: write lock acquired, registering device...");
            server
                .register_device(device)
                .await
                .map_err(|e| crate::error::WebRendererError::RegistrationError(e.to_string()))?
        };
        tracing::info!("WebRenderer: device registered, registering with ControlPoint...");

        // Enregistrer avec le ControlPoint
        register_with_control_point(&di, ws_state)?;
        tracing::info!("WebRenderer: registered with ControlPoint");

        di
    };

    #[cfg(not(feature = "pmoserver"))]
    let di = device.create_instance();

    let udn = di.udn().to_string();

    let session = Arc::new(WebRendererSession {
        token,
        udn,
        device_instance: di,
        ws_sender,
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
        DeviceId(udn.clone()),
        udn.clone(),
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
