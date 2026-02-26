//! Action handlers SOAP → Pipeline pour le WebRenderer serveur
//!
//! Chaque handler bridge une action UPnP vers une commande `PipelineControl`
//! envoyée au pipeline audio serveur, ou lit l'état partagé pour les requêtes GET.

use std::sync::Arc;

use pmodidl::DIDLLite;
use pmoupnp::actions::{ActionData, ActionError, ActionHandler, get_value};
use pmoupnp::{get, set};
use pmoutils::ToXmlElement;

use crate::messages::PlaybackState;
use crate::pipeline::{PipelineControl, PipelineHandle, seconds_to_upnp_time, upnp_time_to_seconds};
use crate::state::SharedState;

type ActionFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<ActionData, ActionError>> + Send>>;

// ─── AVTransport Handlers ───────────────────────────────────────────────────

pub fn play_handler(pipeline: PipelineHandle, state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let pipeline = pipeline.clone();
        let state = state.clone();
        Box::pin(async move {
            // Ne pas écrire Playing ici : c'est stream_source qui le fera
            // une fois que les premiers bytes FLAC ont été produits.
            // Écrire Transitioning pour signaler que la lecture va démarrer.
            state.write().playback_state = PlaybackState::Transitioning;
            pipeline.send(PipelineControl::Play).await;
            Ok(data)
        })
    })
}

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

pub fn next_handler(pipeline: PipelineHandle) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let pipeline = pipeline.clone();
        Box::pin(async move {
            pipeline.send(PipelineControl::Play).await;
            Ok(data)
        })
    })
}

pub fn previous_handler(pipeline: PipelineHandle) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let pipeline = pipeline.clone();
        Box::pin(async move {
            pipeline.send(PipelineControl::Play).await;
            Ok(data)
        })
    })
}

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

pub fn set_uri_handler(pipeline: PipelineHandle, state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let pipeline = pipeline.clone();
        let state = state.clone();
        Box::pin(async move {
            let uri: String = get!(&data, "CurrentURI", String);
            let metadata: String = get_value::<String>(&data, "CurrentURIMetaData")
                .or_else(|_| {
                    get_value::<DIDLLite>(&data, "CurrentURIMetaData")
                        .map(|didl| didl.to_xml())
                })
                .unwrap_or_default();

            // Envoyer l'URI au pipeline serveur (remplace l'envoi WebSocket)
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

pub fn set_volume_handler(pipeline: PipelineHandle, state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let pipeline = pipeline.clone();
        let state = state.clone();
        Box::pin(async move {
            let volume: u16 = get!(&data, "DesiredVolume", u16);
            pipeline.send(PipelineControl::SetVolume(volume)).await;
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

pub fn set_mute_handler(pipeline: PipelineHandle, state: SharedState) -> ActionHandler {
    Arc::new(move |data: ActionData| -> ActionFuture {
        let pipeline = pipeline.clone();
        let state = state.clone();
        Box::pin(async move {
            let mute: bool = get!(&data, "DesiredMute", bool);
            pipeline.send(PipelineControl::SetMute(mute)).await;
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
            // Serveur-side streaming : on produit du FLAC uniquement
            set!(
                &mut data,
                "Sink",
                "http-get:*:audio/flac:*,http-get:*:audio/x-flac:*".to_string()
            );
            Ok(data)
        })
    })
}
