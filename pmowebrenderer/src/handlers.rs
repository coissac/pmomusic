//! Action handlers SOAP → Pipeline pour le WebRenderer serveur
//!
//! Chaque handler bridge une action UPnP vers une commande `PipelineControl`
//! envoyée au pipeline audio serveur, ou lit l'état partagé pour les requêtes GET.

use std::sync::Arc;

use pmodidl::DIDLLite;
use pmoupnp::actions::{ActionData, ActionError, ActionHandler, get_value};
use pmoupnp::{get, set};
use pmodidl::ToXmlElement;

use crate::messages::PlaybackState;
use crate::pipeline::{PipelineControl, PipelineHandle, upnp_time_to_seconds};
use crate::state::SharedState;

type ActionFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<ActionData, ActionError>> + Send>>;

// ─── AVTransport Handlers ───────────────────────────────────────────────────

/// Handler pour l'action UPnP "Play" - lance la lecture du flux audio
pub fn play_handler(pipeline: PipelineHandle, state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let pipeline = pipeline.clone();
        let state = state.clone();
        Box::pin(async move {
            tracing::info!("[WebRenderer] UPnP Play action invoked");
            let has_uri = state.read().current_uri.is_some();
            {
                let mut s = state.write();
                s.playback_state = PlaybackState::Transitioning;
            }
            if has_uri {
                state.write().player_command = Some(serde_json::json!({
                    "type": "stream",
                    "url": "/api/webrenderer/stream"
                }));
                tracing::info!("UPnP Play: stored stream command for frontend polling");
            }
            pipeline.send(PipelineControl::Play).await;
            Ok(data)
        })
    })
}

/// Handler pour l'action UPnP "Stop" - arrête la lecture
pub fn stop_handler(pipeline: PipelineHandle, state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let pipeline = pipeline.clone();
        let state = state.clone();
        Box::pin(async move {
            pipeline.send(PipelineControl::Stop).await;
            state.write().playback_state = PlaybackState::Stopped;
            Ok(data)
        })
    })
}

/// Handler pour l'action UPnP "Pause" - met en pause la lecture
pub fn pause_handler(pipeline: PipelineHandle, state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let pipeline = pipeline.clone();
        let state = state.clone();
        Box::pin(async move {
            pipeline.send(PipelineControl::Pause).await;
            state.write().playback_state = PlaybackState::Paused;
            Ok(data)
        })
    })
}

/// Handler pour l'action UPnP "Next" - passe à la piste suivante
pub fn next_handler(pipeline: PipelineHandle) -> ActionHandler {
    let pipeline = pipeline.clone();
    Arc::new(move |data: ActionData| -> ActionFuture {
        let pipeline = pipeline.clone();
        Box::pin(async move {
            pipeline.send(PipelineControl::Play).await;
            Ok(data)
        })
    })
}

/// Handler pour l'action UPnP "Previous" - retourne au début de la piste actuelle
pub fn previous_handler(pipeline: PipelineHandle) -> ActionHandler {
    let pipeline = pipeline.clone();
    Arc::new(move |data: ActionData| -> ActionFuture {
        let pipeline = pipeline.clone();
        Box::pin(async move {
            pipeline.send(PipelineControl::Play).await;
            Ok(data)
        })
    })
}

/// Handler pour l'action UPnP "Seek" - seek à une position donnée
pub fn seek_handler(pipeline: PipelineHandle) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let pipeline = pipeline.clone();
        Box::pin(async move {
            let target: String = get!(&data, "Target", String);
            let pos_sec = upnp_time_to_seconds(&target);
            pipeline.send(PipelineControl::Seek(pos_sec)).await;
            Ok(data)
        })
    })
}

/// Handler pour l'action UPnP "SetAVTransportURI" - définit l'URI à jouer
pub fn set_uri_handler(pipeline: PipelineHandle, state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let pipeline = pipeline.clone();
        let state = state.clone();
        Box::pin(async move {
            tracing::info!("[WebRenderer] UPnP SetAVTransportURI action invoked");
            let uri: String = get!(&data, "CurrentURI", String);
            let metadata: String = get_value::<String>(&data, "CurrentURIMetaData")
                .or_else(|_| {
                    get_value::<DIDLLite>(&data, "CurrentURIMetaData")
                        .map(|didl| didl.to_xml())
                })
                .unwrap_or_default();

            tracing::info!(uri = %uri, "SetAVTransportURI handler called - loading URI into pipeline");
            pipeline.send(PipelineControl::LoadUri(uri.clone())).await;

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

/// Handler pour l'action UPnP "SetNextAVTransportURI" - définit l'URI suivante (gapless)
pub fn set_next_uri_handler(pipeline: PipelineHandle, state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let pipeline = pipeline.clone();
        let state = state.clone();
        Box::pin(async move {
            let uri: String = get!(&data, "NextURI", String);
            let metadata: String = get_value::<String>(&data, "NextURIMetaData")
                .or_else(|_| {
                    get_value::<DIDLLite>(&data, "NextURIMetaData")
                        .map(|didl| didl.to_xml())
                })
                .unwrap_or_default();

            pipeline.send(PipelineControl::LoadNextUri(uri.clone())).await;

            {
                let mut s = state.write();
                s.next_uri = Some(uri);
                s.next_metadata = Some(metadata);
            }
            Ok(data)
        })
    })
}

/// Handler pour l'action UPnP "GetPositionInfo" - retourne la position actuelle
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

/// Handler pour l'action UPnP "GetTransportInfo" - retourne l'état du transport
pub fn get_transport_info_handler(state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let state = state.clone();
        Box::pin(async move {
            let mut data = data;
            let s = state.read();
            tracing::info!("[WebRenderer] GetTransportInfo: state={:?}", s.playback_state);
            let transport_state = match s.playback_state {
                PlaybackState::Stopped => "STOPPED",
                PlaybackState::Playing => "PLAYING",
                PlaybackState::Paused => "PAUSED_PLAYBACK",
                PlaybackState::Transitioning => "TRANSITIONING",
            };
            set!(&mut data, "CurrentTransportState", transport_state.to_string());
            set!(&mut data, "CurrentTransportStatus", "OK".to_string());
            set!(&mut data, "CurrentSpeed", "1".to_string());
            Ok(data)
        })
    })
}

/// Handler pour l'action UPnP "GetMediaInfo" - retourne les infos du média
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
            set!(&mut data, "CurrentURI", s.current_uri.clone().unwrap_or_default());
            set!(&mut data, "CurrentURIMetaData", s.current_metadata.clone().unwrap_or_default());
            set!(&mut data, "NextURI", s.next_uri.clone().unwrap_or_default());
            set!(&mut data, "NextURIMetaData", s.next_metadata.clone().unwrap_or_default());
            Ok(data)
        })
    })
}

// ─── RenderingControl Handlers ──────────────────────────────────────────────

/// Handler pour l'action UPnP "SetVolume" - définit le volume
pub fn set_volume_handler(_pipeline: PipelineHandle, state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let state = state.clone();
        Box::pin(async move {
            let volume: u16 = get!(&data, "DesiredVolume", u16);
            state.write().volume = volume;
            Ok(data)
        })
    })
}

/// Handler pour l'action UPnP "GetVolume" - retourne le volume actuel
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

/// Handler pour l'action UPnP "SetMute" - définit le mute
pub fn set_mute_handler(_pipeline: PipelineHandle, state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let state = state.clone();
        Box::pin(async move {
            let mute: bool = get!(&data, "DesiredMute", bool);
            state.write().mute = mute;
            Ok(data)
        })
    })
}

/// Handler pour l'action UPnP "GetMute" - retourne l'état mute
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

/// Handler pour l'action UPnP "GetProtocolInfo" - retourne les protocoles supportés
pub fn get_protocol_info_handler() -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        Box::pin(async move {
            let mut data = data;
            set!(&mut data, "Source", String::new());
            set!(
                &mut data,
                "Sink",
                "http-get:*:audio/flac:*,http-get:*:audio/x-flac:*".to_string()
            );
            Ok(data)
        })
    })
}
