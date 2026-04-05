//! Factory pour créer des instances WebRenderer privées
//!
//! Construit des Device/Service models UPnP dynamiques avec des action handlers
//! qui relaient les commandes SOAP vers le navigateur via WebSocket.

use pmoupnp::actions::{Action, Argument};
use pmoupnp::devices::Device;
use pmoupnp::services::Service;
use std::sync::Arc;
use thiserror::Error;

use crate::handlers;
use crate::pipeline::PipelineHandle;
use crate::state::SharedState;

// ─── Réimport des variables statiques de pmomediarenderer ───────────────────
// Variables AVTransport
use pmomediarenderer::avtransport::variables::{
    ABSOLUTETIMEPOSITION, AVTRANSPORTNEXTURI, AVTRANSPORTNEXTURIMETADATA, AVTRANSPORTURI,
    AVTRANSPORTURIMETADATA, A_ARG_TYPE_INSTANCE_ID as AVT_INSTANCE_ID, A_ARG_TYPE_PLAY_SPEED,
    A_ARG_TYPE_SEEKMODE, CURRENTMEDIADURATION, CURRENTPLAYMODE, CURRENTTRACK, CURRENTTRACKDURATION,
    CURRENTTRACKMETADATA, CURRENTTRACKURI, NUMBEROFTRACKS, PLAYBACKSTORAGEMEDIUM,
    POSSIBLEPLAYBACKSTORAGEMEDIA, RELATIVETIMEPOSITION, SEEKMODE, TRANSPORTPLAYSPEED,
    TRANSPORTSTATE, TRANSPORTSTATUS,
};

// Variables RenderingControl
use pmomediarenderer::renderingcontrol::variables::{
    A_ARG_TYPE_CHANNEL, A_ARG_TYPE_INSTANCE_ID as RC_INSTANCE_ID, MUTE, VOLUME,
};

// Variables ConnectionManager
use pmomediarenderer::connectionmanager::variables::{
    A_ARG_TYPE_AVTRANSPORTID, A_ARG_TYPE_CONNECTIONID, A_ARG_TYPE_CONNECTIONSTATUS,
    A_ARG_TYPE_DIRECTION, A_ARG_TYPE_PROTOCOLINFO, A_ARG_TYPE_RCSID, CURRENTCONNECTIONIDS,
    SINKPROTOCOLINFO, SOURCEPROTOCOLINFO,
};

#[derive(Error, Debug)]
pub enum FactoryError {
    #[error("Failed to add service to device: {0}")]
    ServiceError(String),
    #[error("Failed to add action to service: {0}")]
    ActionError(String),
    #[error("Failed to add variable to service: {0}")]
    VariableError(String),
}

macro_rules! add_action_arg {
    ($action:ident, $name:expr, $var:ident, $direction:ident) => {{
        $action
            .add_argument(Arc::new(Argument::new_$direction(
                $name.to_string(),
                Arc::clone(&$var),
            )))
            .map_err(|e| FactoryError::ActionError(e.to_string()))
    }};
    ($action:ident, $name:expr, $var:ident, in) => {
        add_action_arg!($action, $name, $var, in)
    };
    ($action:ident, $name:expr, $var:ident, out) => {
        add_action_arg!($action, $name, $var, out)
    };
}

macro_rules! add_action {
    ($svc:ident, $action:ident) => {{
        $svc.add_action(Arc::new($action))
            .map_err(|e| FactoryError::ActionError(e.to_string()))
    }};
}

macro_rules! add_var {
    ($svc:ident, $var:ident) => {{
        $svc.add_variable(Arc::clone(&$var))
            .map_err(|e| FactoryError::VariableError(e.to_string()))
    }};
}

/// Extrait un nom de navigateur court depuis un User-Agent complet.
fn extract_browser_name(ua: &str) -> &str {
    if ua.contains("Edg/") || ua.contains("EdgA/") {
        "Edge"
    } else if ua.contains("OPR/") || ua.contains("Opera") {
        "Opera"
    } else if ua.contains("Chrome/") {
        "Chrome"
    } else if ua.contains("Firefox/") {
        "Firefox"
    } else if ua.contains("Safari/") {
        "Safari"
    } else {
        "Browser"
    }
}

/// Factory pour créer des Device UPnP WebRenderer avec un pipeline audio serveur
pub struct WebRendererFactory;

impl WebRendererFactory {
    /// Crée un Device model UPnP complet pour un WebRenderer.
    ///
    /// `device_name` sert de clé pour retrouver l'UDN persistant dans la config.
    /// `browser_ua` est le User-Agent complet (pour déterminer le nom affiché).
    pub fn create_device_with_pipeline(
        device_name: &str,
        browser_ua: &str,
        pipeline: PipelineHandle,
        state: SharedState,
    ) -> Result<Device, FactoryError> {
        let avtransport = Self::build_avtransport(pipeline.clone(), state.clone())?;
        let renderingcontrol = Self::build_renderingcontrol(pipeline.clone(), state.clone())?;
        let connectionmanager = Self::build_connectionmanager()?;

        let short_name = extract_browser_name(browser_ua);
        let mut device = Device::new(
            device_name.to_string(),
            "MediaRenderer".to_string(),
            format!("Web Audio – {}", short_name),
        );
        device.set_model_name("WebRenderer".to_string());
        device
            .add_service(Arc::new(avtransport))
            .map_err(|e| FactoryError::ServiceError(format!("{:?}", e)))?;
        device
            .add_service(Arc::new(renderingcontrol))
            .map_err(|e| FactoryError::ServiceError(format!("{:?}", e)))?;
        device
            .add_service(Arc::new(connectionmanager))
            .map_err(|e| FactoryError::ServiceError(format!("{:?}", e)))?;

        Ok(device)
    }

    /// Construit le service AVTransport avec les handlers pipeline
    fn build_avtransport(
        pipeline: PipelineHandle,
        state: SharedState,
    ) -> Result<Service, FactoryError> {
        let mut svc = Service::new("AVTransport".to_string());
        let add_var = |svc: &mut Service, var: &Arc<pmoupnp::state_variables::StateVariable>| {
            svc.add_variable(Arc::clone(var))
                .map_err(|e| FactoryError::VariableError(e.to_string()))
        };

        // Ajouter toutes les variables d'état
        add_var(&mut svc, &AVT_INSTANCE_ID)?;
        add_var(&mut svc, &A_ARG_TYPE_PLAY_SPEED)?;
        add_var(&mut svc, &A_ARG_TYPE_SEEKMODE)?;
        add_var(&mut svc, &ABSOLUTETIMEPOSITION)?;
        add_var(&mut svc, &AVTRANSPORTNEXTURI)?;
        add_var(&mut svc, &AVTRANSPORTNEXTURIMETADATA)?;
        add_var(&mut svc, &AVTRANSPORTURI)?;
        add_var(&mut svc, &AVTRANSPORTURIMETADATA)?;
        add_var(&mut svc, &CURRENTMEDIADURATION)?;
        add_var(&mut svc, &CURRENTPLAYMODE)?;
        add_var(&mut svc, &CURRENTTRACK)?;
        add_var(&mut svc, &CURRENTTRACKDURATION)?;
        add_var(&mut svc, &CURRENTTRACKMETADATA)?;
        add_var(&mut svc, &CURRENTTRACKURI)?;
        add_var(&mut svc, &NUMBEROFTRACKS)?;
        add_var(&mut svc, &PLAYBACKSTORAGEMEDIUM)?;
        add_var(&mut svc, &POSSIBLEPLAYBACKSTORAGEMEDIA)?;
        add_var(&mut svc, &RELATIVETIMEPOSITION)?;
        add_var(&mut svc, &SEEKMODE)?;
        add_var(&mut svc, &TRANSPORTPLAYSPEED)?;
        add_var(&mut svc, &TRANSPORTSTATE)?;
        add_var(&mut svc, &TRANSPORTSTATUS)?;

        let add_action = |svc: &mut Service, action: Arc<Action>| {
            svc.add_action(action)
                .map_err(|e| FactoryError::ActionError(e.to_string()))
        };

        // Play
        let mut play = Action::new("Play".to_string());
        play.add_argument(Arc::new(Argument::new_in(
            "InstanceID".to_string(),
            Arc::clone(&AVT_INSTANCE_ID),
        )))
        .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        play.add_argument(Arc::new(Argument::new_in(
            "Speed".to_string(),
            Arc::clone(&TRANSPORTPLAYSPEED),
        )))
        .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        play.set_handler(handlers::play_handler(pipeline.clone(), state.clone()));
        add_action(&mut svc, Arc::new(play))?;

        // Stop
        let mut stop = Action::new("Stop".to_string());
        stop.add_argument(Arc::new(Argument::new_in(
            "InstanceID".to_string(),
            Arc::clone(&AVT_INSTANCE_ID),
        )))
        .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        stop.set_handler(handlers::stop_handler(pipeline.clone(), state.clone()));
        add_action(&mut svc, Arc::new(stop))?;

        // Pause
        let mut pause = Action::new("Pause".to_string());
        pause
            .add_argument(Arc::new(Argument::new_in(
                "InstanceID".to_string(),
                Arc::clone(&AVT_INSTANCE_ID),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        pause.set_handler(handlers::pause_handler(pipeline.clone(), state.clone()));
        add_action(&mut svc, Arc::new(pause))?;

        // Next
        let mut next = Action::new("Next".to_string());
        next.add_argument(Arc::new(Argument::new_in(
            "InstanceID".to_string(),
            Arc::clone(&AVT_INSTANCE_ID),
        )))
        .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        next.set_handler(handlers::next_handler(pipeline.clone()));
        add_action(&mut svc, Arc::new(next))?;

        // Previous
        let mut previous = Action::new("Previous".to_string());
        previous
            .add_argument(Arc::new(Argument::new_in(
                "InstanceID".to_string(),
                Arc::clone(&AVT_INSTANCE_ID),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        previous.set_handler(handlers::previous_handler(pipeline.clone()));
        add_action(&mut svc, Arc::new(previous))?;

        // Seek
        let mut seek = Action::new("Seek".to_string());
        seek.add_argument(Arc::new(Argument::new_in(
            "InstanceID".to_string(),
            Arc::clone(&AVT_INSTANCE_ID),
        )))
        .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        seek.add_argument(Arc::new(Argument::new_in(
            "Unit".to_string(),
            Arc::clone(&A_ARG_TYPE_SEEKMODE),
        )))
        .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        seek.add_argument(Arc::new(Argument::new_in(
            "Target".to_string(),
            Arc::clone(&SEEKMODE),
        )))
        .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        seek.set_handler(handlers::seek_handler(pipeline.clone()));
        add_action(&mut svc, Arc::new(seek))?;

        // SetAVTransportURI
        let mut set_uri = Action::new("SetAVTransportURI".to_string());
        set_uri
            .add_argument(Arc::new(Argument::new_in(
                "InstanceID".to_string(),
                Arc::clone(&AVT_INSTANCE_ID),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        set_uri
            .add_argument(Arc::new(Argument::new_in(
                "CurrentURI".to_string(),
                Arc::clone(&AVTRANSPORTURI),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        set_uri
            .add_argument(Arc::new(Argument::new_in(
                "CurrentURIMetaData".to_string(),
                Arc::clone(&AVTRANSPORTURIMETADATA),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        set_uri.set_handler(handlers::set_uri_handler(pipeline.clone(), state.clone()));
        add_action(&mut svc, Arc::new(set_uri))?;

        // SetNextAVTransportURI
        let mut set_next_uri = Action::new("SetNextAVTransportURI".to_string());
        set_next_uri
            .add_argument(Arc::new(Argument::new_in(
                "InstanceID".to_string(),
                Arc::clone(&AVT_INSTANCE_ID),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        set_next_uri
            .add_argument(Arc::new(Argument::new_in(
                "NextURI".to_string(),
                Arc::clone(&AVTRANSPORTNEXTURI),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        set_next_uri
            .add_argument(Arc::new(Argument::new_in(
                "NextURIMetaData".to_string(),
                Arc::clone(&AVTRANSPORTNEXTURIMETADATA),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        set_next_uri.set_handler(handlers::set_next_uri_handler(
            pipeline.clone(),
            state.clone(),
        ));
        add_action(&mut svc, Arc::new(set_next_uri))?;

        // GetPositionInfo
        let mut get_pos = Action::new("GetPositionInfo".to_string());
        get_pos
            .add_argument(Arc::new(Argument::new_in(
                "InstanceID".to_string(),
                Arc::clone(&AVT_INSTANCE_ID),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_pos
            .add_argument(Arc::new(Argument::new_out(
                "Track".to_string(),
                Arc::clone(&CURRENTTRACK),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_pos
            .add_argument(Arc::new(Argument::new_out(
                "TrackDuration".to_string(),
                Arc::clone(&CURRENTTRACKDURATION),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_pos
            .add_argument(Arc::new(Argument::new_out(
                "TrackURI".to_string(),
                Arc::clone(&CURRENTTRACKURI),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_pos
            .add_argument(Arc::new(Argument::new_out(
                "TrackMetaData".to_string(),
                Arc::clone(&CURRENTTRACKMETADATA),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_pos
            .add_argument(Arc::new(Argument::new_out(
                "RelTime".to_string(),
                Arc::clone(&RELATIVETIMEPOSITION),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_pos
            .add_argument(Arc::new(Argument::new_out(
                "AbsTime".to_string(),
                Arc::clone(&ABSOLUTETIMEPOSITION),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_pos.set_stateful(false);
        get_pos.set_handler(handlers::get_position_info_handler(state.clone()));
        add_action(&mut svc, Arc::new(get_pos))?;

        // GetTransportInfo
        let mut get_info = Action::new("GetTransportInfo".to_string());
        get_info
            .add_argument(Arc::new(Argument::new_in(
                "InstanceID".to_string(),
                Arc::clone(&AVT_INSTANCE_ID),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_info
            .add_argument(Arc::new(Argument::new_out(
                "CurrentTransportState".to_string(),
                Arc::clone(&TRANSPORTSTATE),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_info
            .add_argument(Arc::new(Argument::new_out(
                "CurrentTransportStatus".to_string(),
                Arc::clone(&TRANSPORTSTATUS),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_info
            .add_argument(Arc::new(Argument::new_out(
                "CurrentSpeed".to_string(),
                Arc::clone(&TRANSPORTPLAYSPEED),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_info.set_stateful(false);
        get_info.set_handler(handlers::get_transport_info_handler(state.clone()));
        add_action(&mut svc, Arc::new(get_info))?;

        // GetMediaInfo
        let mut get_media = Action::new("GetMediaInfo".to_string());
        get_media
            .add_argument(Arc::new(Argument::new_in(
                "InstanceID".to_string(),
                Arc::clone(&AVT_INSTANCE_ID),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_media
            .add_argument(Arc::new(Argument::new_out(
                "NrTracks".to_string(),
                Arc::clone(&NUMBEROFTRACKS),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_media
            .add_argument(Arc::new(Argument::new_out(
                "CurrentURI".to_string(),
                Arc::clone(&AVTRANSPORTURI),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_media
            .add_argument(Arc::new(Argument::new_out(
                "CurrentURIMetaData".to_string(),
                Arc::clone(&AVTRANSPORTURIMETADATA),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_media
            .add_argument(Arc::new(Argument::new_out(
                "NextURI".to_string(),
                Arc::clone(&AVTRANSPORTNEXTURI),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_media
            .add_argument(Arc::new(Argument::new_out(
                "NextURIMetaData".to_string(),
                Arc::clone(&AVTRANSPORTNEXTURIMETADATA),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_media.set_stateful(false);
        get_media.set_handler(handlers::get_media_info_handler(state.clone()));
        add_action(&mut svc, Arc::new(get_media))?;

        // GetTransportSettings (passthrough)
        let mut get_settings = Action::new("GetTransportSettings".to_string());
        get_settings
            .add_argument(Arc::new(Argument::new_in(
                "InstanceID".to_string(),
                Arc::clone(&AVT_INSTANCE_ID),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        add_action(&mut svc, Arc::new(get_settings))?;

        // GetDeviceCapabilities (passthrough)
        let mut get_caps = Action::new("GetDeviceCapabilities".to_string());
        get_caps
            .add_argument(Arc::new(Argument::new_in(
                "InstanceID".to_string(),
                Arc::clone(&AVT_INSTANCE_ID),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        add_action(&mut svc, Arc::new(get_caps))?;

        // GetCurrentTransportActions (passthrough)
        let mut get_actions = Action::new("GetCurrentTransportActions".to_string());
        get_actions
            .add_argument(Arc::new(Argument::new_in(
                "InstanceID".to_string(),
                Arc::clone(&AVT_INSTANCE_ID),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        add_action(&mut svc, Arc::new(get_actions))?;

        Ok(svc)
    }

    /// Construit le service RenderingControl avec les handlers pipeline
    fn build_renderingcontrol(
        pipeline: PipelineHandle,
        state: SharedState,
    ) -> Result<Service, FactoryError> {
        let mut svc = Service::new("RenderingControl".to_string());

        svc.add_variable(Arc::clone(&RC_INSTANCE_ID))
            .map_err(|e| FactoryError::VariableError(e.to_string()))?;
        svc.add_variable(Arc::clone(&A_ARG_TYPE_CHANNEL))
            .map_err(|e| FactoryError::VariableError(e.to_string()))?;
        svc.add_variable(Arc::clone(&VOLUME))
            .map_err(|e| FactoryError::VariableError(e.to_string()))?;
        svc.add_variable(Arc::clone(&MUTE))
            .map_err(|e| FactoryError::VariableError(e.to_string()))?;

        // SetVolume
        let mut set_vol = Action::new("SetVolume".to_string());
        set_vol
            .add_argument(Arc::new(Argument::new_in(
                "InstanceID".to_string(),
                Arc::clone(&RC_INSTANCE_ID),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        set_vol
            .add_argument(Arc::new(Argument::new_in(
                "Channel".to_string(),
                Arc::clone(&A_ARG_TYPE_CHANNEL),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        set_vol
            .add_argument(Arc::new(Argument::new_in(
                "DesiredVolume".to_string(),
                Arc::clone(&VOLUME),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        set_vol.set_handler(handlers::set_volume_handler(
            pipeline.clone(),
            state.clone(),
        ));
        svc.add_action(Arc::new(set_vol))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;

        // GetVolume
        let mut get_vol = Action::new("GetVolume".to_string());
        get_vol
            .add_argument(Arc::new(Argument::new_in(
                "InstanceID".to_string(),
                Arc::clone(&RC_INSTANCE_ID),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_vol
            .add_argument(Arc::new(Argument::new_in(
                "Channel".to_string(),
                Arc::clone(&A_ARG_TYPE_CHANNEL),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_vol
            .add_argument(Arc::new(Argument::new_out(
                "CurrentVolume".to_string(),
                Arc::clone(&VOLUME),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_vol.set_stateful(false);
        get_vol.set_handler(handlers::get_volume_handler(state.clone()));
        svc.add_action(Arc::new(get_vol))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;

        // SetMute
        let mut set_mute = Action::new("SetMute".to_string());
        set_mute
            .add_argument(Arc::new(Argument::new_in(
                "InstanceID".to_string(),
                Arc::clone(&RC_INSTANCE_ID),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        set_mute
            .add_argument(Arc::new(Argument::new_in(
                "Channel".to_string(),
                Arc::clone(&A_ARG_TYPE_CHANNEL),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        set_mute
            .add_argument(Arc::new(Argument::new_in(
                "DesiredMute".to_string(),
                Arc::clone(&MUTE),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        set_mute.set_handler(handlers::set_mute_handler(pipeline.clone(), state.clone()));
        svc.add_action(Arc::new(set_mute))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;

        // GetMute
        let mut get_mute = Action::new("GetMute".to_string());
        get_mute
            .add_argument(Arc::new(Argument::new_in(
                "InstanceID".to_string(),
                Arc::clone(&RC_INSTANCE_ID),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_mute
            .add_argument(Arc::new(Argument::new_in(
                "Channel".to_string(),
                Arc::clone(&A_ARG_TYPE_CHANNEL),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_mute
            .add_argument(Arc::new(Argument::new_out(
                "CurrentMute".to_string(),
                Arc::clone(&MUTE),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_mute.set_stateful(false);
        get_mute.set_handler(handlers::get_mute_handler(state.clone()));
        svc.add_action(Arc::new(get_mute))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;

        Ok(svc)
    }

    /// Construit le service ConnectionManager
    fn build_connectionmanager() -> Result<Service, FactoryError> {
        let mut svc = Service::new("ConnectionManager".to_string());

        svc.add_variable(Arc::clone(&A_ARG_TYPE_CONNECTIONID))
            .map_err(|e| FactoryError::VariableError(e.to_string()))?;
        svc.add_variable(Arc::clone(&A_ARG_TYPE_CONNECTIONSTATUS))
            .map_err(|e| FactoryError::VariableError(e.to_string()))?;
        svc.add_variable(Arc::clone(&A_ARG_TYPE_DIRECTION))
            .map_err(|e| FactoryError::VariableError(e.to_string()))?;
        svc.add_variable(Arc::clone(&A_ARG_TYPE_PROTOCOLINFO))
            .map_err(|e| FactoryError::VariableError(e.to_string()))?;
        svc.add_variable(Arc::clone(&A_ARG_TYPE_RCSID))
            .map_err(|e| FactoryError::VariableError(e.to_string()))?;
        svc.add_variable(Arc::clone(&A_ARG_TYPE_AVTRANSPORTID))
            .map_err(|e| FactoryError::VariableError(e.to_string()))?;
        svc.add_variable(Arc::clone(&CURRENTCONNECTIONIDS))
            .map_err(|e| FactoryError::VariableError(e.to_string()))?;
        svc.add_variable(Arc::clone(&SINKPROTOCOLINFO))
            .map_err(|e| FactoryError::VariableError(e.to_string()))?;
        svc.add_variable(Arc::clone(&SOURCEPROTOCOLINFO))
            .map_err(|e| FactoryError::VariableError(e.to_string()))?;

        // GetProtocolInfo
        let mut get_proto = Action::new("GetProtocolInfo".to_string());
        get_proto
            .add_argument(Arc::new(Argument::new_out(
                "Source".to_string(),
                Arc::clone(&SOURCEPROTOCOLINFO),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_proto
            .add_argument(Arc::new(Argument::new_out(
                "Sink".to_string(),
                Arc::clone(&SINKPROTOCOLINFO),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_proto.set_stateful(false);
        get_proto.set_handler(handlers::get_protocol_info_handler());
        svc.add_action(Arc::new(get_proto))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;

        // GetCurrentConnectionIDs
        let mut get_ids = Action::new("GetCurrentConnectionIDs".to_string());
        get_ids
            .add_argument(Arc::new(Argument::new_out(
                "ConnectionIDs".to_string(),
                Arc::clone(&CURRENTCONNECTIONIDS),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        svc.add_action(Arc::new(get_ids))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;

        // GetCurrentConnectionInfo
        let mut get_conn = Action::new("GetCurrentConnectionInfo".to_string());
        get_conn
            .add_argument(Arc::new(Argument::new_in(
                "ConnectionID".to_string(),
                Arc::clone(&A_ARG_TYPE_CONNECTIONID),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_conn
            .add_argument(Arc::new(Argument::new_out(
                "RcsID".to_string(),
                Arc::clone(&A_ARG_TYPE_RCSID),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_conn
            .add_argument(Arc::new(Argument::new_out(
                "AVTransportID".to_string(),
                Arc::clone(&A_ARG_TYPE_AVTRANSPORTID),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_conn
            .add_argument(Arc::new(Argument::new_out(
                "ProtocolInfo".to_string(),
                Arc::clone(&A_ARG_TYPE_PROTOCOLINFO),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_conn
            .add_argument(Arc::new(Argument::new_out(
                "PeerConnectionManager".to_string(),
                Arc::clone(&A_ARG_TYPE_PROTOCOLINFO),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_conn
            .add_argument(Arc::new(Argument::new_out(
                "PeerConnectionID".to_string(),
                Arc::clone(&A_ARG_TYPE_CONNECTIONID),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_conn
            .add_argument(Arc::new(Argument::new_out(
                "Direction".to_string(),
                Arc::clone(&A_ARG_TYPE_DIRECTION),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        get_conn
            .add_argument(Arc::new(Argument::new_out(
                "Status".to_string(),
                Arc::clone(&A_ARG_TYPE_CONNECTIONSTATUS),
            )))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;
        svc.add_action(Arc::new(get_conn))
            .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))?;

        Ok(svc)
    }
}
