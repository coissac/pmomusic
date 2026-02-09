//! Action handlers SOAP → WebSocket pour le WebRenderer
//!
//! Chaque handler bridge une action UPnP vers une commande WebSocket
//! envoyée au navigateur, ou lit l'état partagé pour les requêtes GET.

use std::sync::Arc;
use tokio::sync::mpsc;

use pmoupnp::actions::{ActionData, ActionError, ActionHandler};
use pmoupnp::{get, set};

use crate::messages::{CommandParams, PlaybackState, ServerMessage, TransportAction};
use crate::state::SharedState;

type ActionFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<ActionData, ActionError>> + Send>>;

// ─── AVTransport Handlers ───────────────────────────────────────────────────

pub fn play_handler(ws: mpsc::UnboundedSender<ServerMessage>, state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let ws = ws.clone();
        let state = state.clone();
        Box::pin(async move {
            let _ = ws.send(ServerMessage::Command {
                action: TransportAction::Play,
                params: None,
            });
            state.write().playback_state = PlaybackState::Playing;
            Ok(data)
        })
    })
}

pub fn stop_handler(ws: mpsc::UnboundedSender<ServerMessage>, state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let ws = ws.clone();
        let state = state.clone();
        Box::pin(async move {
            let _ = ws.send(ServerMessage::Command {
                action: TransportAction::Stop,
                params: None,
            });
            state.write().playback_state = PlaybackState::Stopped;
            Ok(data)
        })
    })
}

pub fn pause_handler(
    ws: mpsc::UnboundedSender<ServerMessage>,
    state: SharedState,
) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let ws = ws.clone();
        let state = state.clone();
        Box::pin(async move {
            let _ = ws.send(ServerMessage::Command {
                action: TransportAction::Pause,
                params: None,
            });
            state.write().playback_state = PlaybackState::Paused;
            Ok(data)
        })
    })
}

pub fn next_handler(ws: mpsc::UnboundedSender<ServerMessage>) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let ws = ws.clone();
        Box::pin(async move {
            let _ = ws.send(ServerMessage::Command {
                action: TransportAction::Play,
                params: None,
            });
            Ok(data)
        })
    })
}

pub fn previous_handler(ws: mpsc::UnboundedSender<ServerMessage>) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let ws = ws.clone();
        Box::pin(async move {
            let _ = ws.send(ServerMessage::Command {
                action: TransportAction::Play,
                params: None,
            });
            Ok(data)
        })
    })
}

pub fn seek_handler(ws: mpsc::UnboundedSender<ServerMessage>) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let ws = ws.clone();
        Box::pin(async move {
            let target: String = get!(&data, "Target", String);
            let _ = ws.send(ServerMessage::Command {
                action: TransportAction::Seek,
                params: Some(CommandParams {
                    uri: None,
                    metadata: None,
                    position: Some(target),
                }),
            });
            Ok(data)
        })
    })
}

pub fn set_uri_handler(
    ws: mpsc::UnboundedSender<ServerMessage>,
    state: SharedState,
) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let ws = ws.clone();
        let state = state.clone();
        Box::pin(async move {
            let uri: String = get!(&data, "CurrentURI", String);
            let metadata: String = get!(&data, "CurrentURIMetaData", String);
            let _ = ws.send(ServerMessage::Command {
                action: TransportAction::SetUri,
                params: Some(CommandParams {
                    uri: Some(uri.clone()),
                    metadata: Some(metadata.clone()),
                    position: None,
                }),
            });
            {
                let mut s = state.write();
                s.current_uri = Some(uri);
                s.current_metadata = Some(metadata);
                s.playback_state = PlaybackState::Transitioning;
            }
            Ok(data)
        })
    })
}

pub fn set_next_uri_handler(_state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        Box::pin(async move {
            let _uri: String = get!(&data, "NextURI", String);
            let _metadata: String = get!(&data, "NextURIMetaData", String);
            Ok(data)
        })
    })
}

pub fn get_position_info_handler(state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let state = state.clone();
        Box::pin(async move {
            let mut data = data;
            let s = state.read();
            set!(
                &mut data,
                "Track",
                if s.current_uri.is_some() { 1u32 } else { 0u32 }
            );
            set!(
                &mut data,
                "TrackDuration",
                s.duration.clone().unwrap_or_else(|| "00:00:00".to_string())
            );
            set!(
                &mut data,
                "TrackURI",
                s.current_uri.clone().unwrap_or_default()
            );
            set!(
                &mut data,
                "TrackMetaData",
                s.current_metadata.clone().unwrap_or_default()
            );
            set!(
                &mut data,
                "RelTime",
                s.position.clone().unwrap_or_else(|| "00:00:00".to_string())
            );
            set!(
                &mut data,
                "AbsTime",
                s.position.clone().unwrap_or_else(|| "00:00:00".to_string())
            );
            Ok(data)
        })
    })
}

pub fn get_transport_info_handler(state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let state = state.clone();
        Box::pin(async move {
            let mut data = data;
            let s = state.read();
            let transport_state = match s.playback_state {
                PlaybackState::Stopped => "STOPPED",
                PlaybackState::Playing => "PLAYING",
                PlaybackState::Paused => "PAUSED_PLAYBACK",
                PlaybackState::Transitioning => "TRANSITIONING",
            };
            set!(
                &mut data,
                "CurrentTransportState",
                transport_state.to_string()
            );
            set!(&mut data, "CurrentTransportStatus", "OK".to_string());
            Ok(data)
        })
    })
}

pub fn get_media_info_handler(state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let state = state.clone();
        Box::pin(async move {
            let mut data = data;
            let s = state.read();
            set!(
                &mut data,
                "NrTracks",
                if s.current_uri.is_some() { 1u32 } else { 0u32 }
            );
            set!(
                &mut data,
                "CurrentURI",
                s.current_uri.clone().unwrap_or_default()
            );
            set!(
                &mut data,
                "CurrentURIMetaData",
                s.current_metadata.clone().unwrap_or_default()
            );
            set!(&mut data, "NextURI", String::new());
            set!(&mut data, "NextURIMetaData", String::new());
            Ok(data)
        })
    })
}

// ─── RenderingControl Handlers ──────────────────────────────────────────────

pub fn set_volume_handler(
    ws: mpsc::UnboundedSender<ServerMessage>,
    state: SharedState,
) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let ws = ws.clone();
        let state = state.clone();
        Box::pin(async move {
            let volume: u16 = get!(&data, "DesiredVolume", u16);
            let _ = ws.send(ServerMessage::SetVolume { volume });
            state.write().volume = volume;
            Ok(data)
        })
    })
}

pub fn get_volume_handler(state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let state = state.clone();
        Box::pin(async move {
            let mut data = data;
            let volume = state.read().volume;
            set!(&mut data, "CurrentVolume", volume);
            Ok(data)
        })
    })
}

pub fn set_mute_handler(
    ws: mpsc::UnboundedSender<ServerMessage>,
    state: SharedState,
) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let ws = ws.clone();
        let state = state.clone();
        Box::pin(async move {
            let mute: bool = get!(&data, "DesiredMute", bool);
            let _ = ws.send(ServerMessage::SetMute { mute });
            state.write().mute = mute;
            Ok(data)
        })
    })
}

pub fn get_mute_handler(state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let state = state.clone();
        Box::pin(async move {
            let mut data = data;
            let mute = state.read().mute;
            set!(&mut data, "CurrentMute", mute);
            Ok(data)
        })
    })
}

// ─── ConnectionManager Handlers ─────────────────────────────────────────────

pub fn get_protocol_info_handler() -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        Box::pin(async move {
            let mut data = data;
            set!(&mut data, "Source", String::new());
            set!(
                &mut data,
                "Sink",
                "http-get:*:audio/mpeg:*,http-get:*:audio/mp4:*,http-get:*:audio/ogg:*,http-get:*:audio/flac:*,http-get:*:audio/wav:*,http-get:*:audio/x-flac:*,http-get:*:audio/aac:*,http-get:*:audio/webm:*".to_string()
            );
            Ok(data)
        })
    })
}
