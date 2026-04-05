//! Action handlers SOAP → Pipeline pour le WebRenderer serveur
//!
//! Chaque handler bridge une action UPnP vers une commande `PipelineControl`
//! envoyée au pipeline audio serveur, ou lit l'état partagé pour les requêtes GET.

use pmodidl::DIDLLite;
use pmodidl::ToXmlElement;
use pmoupnp::actions::{get_value, ActionHandler};
use pmoupnp::{action_handler, get, set};

use crate::messages::PlaybackState;
use crate::pipeline::{upnp_time_to_seconds, PipelineControl, PipelineHandle};
use crate::state::SharedState;

// ─── AVTransport : commandes de transport ─────────────────────────────────────

pub fn play_handler(
    pipeline: PipelineHandle,
    state: SharedState,
    instance_id: String,
) -> ActionHandler {
    action_handler!(
        captures(pipeline, state, instance_id) | data | {
            tracing::info!("[WebRenderer] UPnP Play action invoked");
            let has_uri = state.read().current_uri.is_some();
            if !has_uri {
                tracing::warn!("[WebRenderer] UPnP Play ignored: no URI loaded");
                return Ok(data);
            }
            {
                let mut s = state.write();
                s.playback_state = PlaybackState::Transitioning;
                s.push_command(crate::adapter::DeviceCommand::Stream {
                    url: format!("/api/webrenderer/{}/stream", instance_id),
                });
                tracing::info!("UPnP Play: stored stream command for frontend polling");
            }
            pipeline.flac_handle.resume();
            pipeline.send(PipelineControl::Play).await;
            Ok(data)
        }
    )
}

pub fn stop_handler(pipeline: PipelineHandle, state: SharedState) -> ActionHandler {
    action_handler!(
        captures(pipeline, state) | data | {
            pipeline.send(PipelineControl::Stop).await;
            pipeline.flac_handle.pause();
            pipeline
                .adapter
                .deliver(crate::adapter::DeviceCommand::Flush);
            pipeline
                .adapter
                .deliver(crate::adapter::DeviceCommand::Stop);
            state.write().playback_state = PlaybackState::Stopped;
            Ok(data)
        }
    )
}

pub fn pause_handler(pipeline: PipelineHandle, state: SharedState) -> ActionHandler {
    action_handler!(
        captures(pipeline, state) | data | {
            pipeline.send(PipelineControl::Pause).await;
            pipeline.flac_handle.pause();
            pipeline
                .adapter
                .deliver(crate::adapter::DeviceCommand::Pause);
            state.write().playback_state = PlaybackState::Paused;
            Ok(data)
        }
    )
}

pub fn next_handler(pipeline: PipelineHandle) -> ActionHandler {
    action_handler!(
        captures(pipeline) | data | {
            pipeline.send(PipelineControl::Play).await;
            Ok(data)
        }
    )
}

pub fn previous_handler(pipeline: PipelineHandle) -> ActionHandler {
    action_handler!(
        captures(pipeline) | data | {
            pipeline.send(PipelineControl::Play).await;
            Ok(data)
        }
    )
}

pub fn seek_handler(pipeline: PipelineHandle) -> ActionHandler {
    action_handler!(
        captures(pipeline) | data | {
            let target: String = get!(&data, "Target", String);
            let pos_sec = upnp_time_to_seconds(&target);
            pipeline.send(PipelineControl::Seek(pos_sec)).await;
            Ok(data)
        }
    )
}

// ─── AVTransport : chargement de média ────────────────────────────────────────

pub fn set_uri_handler(pipeline: PipelineHandle, state: SharedState) -> ActionHandler {
    action_handler!(captures(pipeline, state) |mut data| {
        tracing::info!("[WebRenderer] UPnP SetAVTransportURI action invoked");
        let uri: String = get!(&data, "CurrentURI", String);
        let metadata: String = get_value::<String>(&data, "CurrentURIMetaData")
            .or_else(|_| get_value::<DIDLLite>(&data, "CurrentURIMetaData").map(|didl| didl.to_xml()))
            .unwrap_or_default();

        tracing::info!(uri = %uri, "SetAVTransportURI handler called - loading URI into pipeline");
        {
            let mut s = state.write();
            s.current_uri = Some(uri.clone());
            s.current_metadata = Some(metadata);
            s.playback_state = PlaybackState::Transitioning;
        }
        pipeline.send(PipelineControl::LoadUri(uri)).await;
        Ok(data)
    })
}

pub fn set_next_uri_handler(pipeline: PipelineHandle, state: SharedState) -> ActionHandler {
    action_handler!(captures(pipeline, state) |mut data| {
        let uri: String = get!(&data, "NextURI", String);
        let metadata: String = get_value::<String>(&data, "NextURIMetaData")
            .or_else(|_| get_value::<DIDLLite>(&data, "NextURIMetaData").map(|didl| didl.to_xml()))
            .unwrap_or_default();

        {
            let mut s = state.write();
            s.next_uri = Some(uri.clone());
            s.next_metadata = Some(metadata);
        }
        pipeline.send(PipelineControl::LoadNextUri(uri)).await;
        Ok(data)
    })
}

// ─── AVTransport : getters ─────────────────────────────────────────────────────

pub fn get_position_info_handler(state: SharedState) -> ActionHandler {
    action_handler!(captures(state) |mut data| {
        let s = state.read();
        set!(&mut data, "Track", if s.current_uri.is_some() { 1u32 } else { 0u32 });
        set!(&mut data, "TrackDuration", s.duration.clone().unwrap_or_else(|| "00:00:00".to_string()));
        set!(&mut data, "TrackURI", s.current_uri.clone().unwrap_or_default());
        set!(&mut data, "TrackMetaData", s.current_metadata.clone().unwrap_or_default());
        set!(&mut data, "RelTime", s.position.clone().unwrap_or_else(|| "00:00:00".to_string()));
        set!(&mut data, "AbsTime", s.position.clone().unwrap_or_else(|| "00:00:00".to_string()));
        Ok(data)
    })
}

pub fn get_transport_info_handler(state: SharedState) -> ActionHandler {
    action_handler!(captures(state) |mut data| {
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
}

pub fn get_media_info_handler(state: SharedState) -> ActionHandler {
    action_handler!(captures(state) |mut data| {
        let s = state.read();
        set!(&mut data, "NrTracks", if s.current_uri.is_some() { 1u32 } else { 0u32 });
        set!(&mut data, "CurrentURI", s.current_uri.clone().unwrap_or_default());
        set!(&mut data, "CurrentURIMetaData", s.current_metadata.clone().unwrap_or_default());
        set!(&mut data, "NextURI", s.next_uri.clone().unwrap_or_default());
        set!(&mut data, "NextURIMetaData", s.next_metadata.clone().unwrap_or_default());
        Ok(data)
    })
}

// ─── ConnectionManager ─────────────────────────────────────────────────────────

pub fn get_protocol_info_handler() -> ActionHandler {
    action_handler!(|mut data| {
        set!(&mut data, "Source", String::new());
        set!(
            &mut data,
            "Sink",
            "http-get:*:audio/flac:*,http-get:*:audio/x-flac:*".to_string()
        );
        Ok(data)
    })
}

// ─── RenderingControl ──────────────────────────────────────────────────────────

pub fn set_volume_handler(state: SharedState) -> ActionHandler {
    action_handler!(captures(state) |mut data| {
        let volume: u16 = get!(&data, "DesiredVolume", u16);
        state.write().volume = volume;
        Ok(data)
    })
}

pub fn get_volume_handler(state: SharedState) -> ActionHandler {
    action_handler!(captures(state) |mut data| {
        let volume = state.read().volume;
        set!(&mut data, "CurrentVolume", volume);
        Ok(data)
    })
}

pub fn set_mute_handler(state: SharedState) -> ActionHandler {
    action_handler!(captures(state) |mut data| {
        let mute: bool = get!(&data, "DesiredMute", bool);
        state.write().mute = mute;
        Ok(data)
    })
}

pub fn get_mute_handler(state: SharedState) -> ActionHandler {
    action_handler!(captures(state) |mut data| {
        let mute = state.read().mute;
        set!(&mut data, "CurrentMute", mute);
        Ok(data)
    })
}
