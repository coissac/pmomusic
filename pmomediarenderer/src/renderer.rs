//! Factory pour créer des instances MediaRenderer privées

use pmoupnp::actions::{Action, Argument};
use pmoupnp::devices::Device;
use pmoupnp::services::Service;
use std::sync::Arc;
use thiserror::Error;

use crate::handlers;
use crate::pipeline::PipelineHandle;
use crate::state::SharedState;

// ─── Helper functions pour l'ajout d'arguments UPnP ───────────────────────
fn add_arg_in(
    action: &mut Action,
    name: &str,
    var: &Arc<pmoupnp::state_variables::StateVariable>,
) -> Result<(), FactoryError> {
    action
        .add_argument(Arc::new(Argument::new_in(
            name.to_string(),
            Arc::clone(var),
        )))
        .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))
}

fn add_arg_out(
    action: &mut Action,
    name: &str,
    var: &Arc<pmoupnp::state_variables::StateVariable>,
) -> Result<(), FactoryError> {
    action
        .add_argument(Arc::new(Argument::new_out(
            name.to_string(),
            Arc::clone(var),
        )))
        .map_err(|e| FactoryError::ActionError(format!("{:?}", e)))
}

fn add_var(
    svc: &mut Service,
    var: &Arc<pmoupnp::state_variables::StateVariable>,
) -> Result<(), FactoryError> {
    svc.add_variable(Arc::clone(var))
        .map_err(|e| FactoryError::VariableError(e.to_string()))
}

fn add_action(svc: &mut Service, action: Arc<Action>) -> Result<(), FactoryError> {
    svc.add_action(action)
        .map_err(|e| FactoryError::ActionError(e.to_string()))
}

// ─── Réimport des variables statiques de pmomediarenderer ───────────────────
use crate::avtransport::variables::{
    ABSOLUTETIMEPOSITION, AVTRANSPORTNEXTURI, AVTRANSPORTNEXTURIMETADATA, AVTRANSPORTURI,
    AVTRANSPORTURIMETADATA, A_ARG_TYPE_INSTANCE_ID as AVT_INSTANCE_ID, A_ARG_TYPE_PLAY_SPEED,
    A_ARG_TYPE_SEEKMODE, CURRENTMEDIADURATION, CURRENTPLAYMODE, CURRENTTRACK, CURRENTTRACKDURATION,
    CURRENTTRACKMETADATA, CURRENTTRACKURI, NUMBEROFTRACKS, PLAYBACKSTORAGEMEDIUM,
    POSSIBLEPLAYBACKSTORAGEMEDIA, RELATIVETIMEPOSITION, SEEKMODE, TRANSPORTPLAYSPEED,
    TRANSPORTSTATE, TRANSPORTSTATUS,
};

use crate::renderingcontrol::variables::{
    A_ARG_TYPE_CHANNEL, A_ARG_TYPE_INSTANCE_ID as RC_INSTANCE_ID, MUTE, VOLUME,
};

use crate::connectionmanager::variables::{
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

/// Factory pour créer des Device UPnP MediaRenderer avec un pipeline audio serveur
pub struct MediaRendererFactory;

impl MediaRendererFactory {
    pub fn create_device_with_pipeline(
        device_name: &str,
        device_type: &str,
        friendly_name_suffix: &str,
        pipeline: PipelineHandle,
        state: SharedState,
        stream_url_base: &str,
    ) -> Result<Device, FactoryError> {
        let avtransport = Self::build_avtransport(pipeline.clone(), state.clone(), device_name, stream_url_base)?;
        let renderingcontrol = Self::build_renderingcontrol(state.clone())?;
        let connectionmanager = Self::build_connectionmanager()?;

        let device = Device::new_from_config(
            device_name.to_string(),
            device_type.to_string(),
            friendly_name_suffix.to_string(),
        );
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

    fn build_avtransport(
        pipeline: PipelineHandle,
        state: SharedState,
        instance_id: &str,
        stream_url_base: &str,
    ) -> Result<Service, FactoryError> {
        let mut svc = Service::new("AVTransport".to_string());

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

        let mut play = Action::new("Play".to_string());
        add_arg_in(&mut play, "InstanceID", &AVT_INSTANCE_ID)?;
        add_arg_in(&mut play, "Speed", &TRANSPORTPLAYSPEED)?;
        play.set_handler(handlers::play_handler(
            pipeline.clone(),
            state.clone(),
            instance_id.to_string(),
            stream_url_base.to_string(),
        ));
        add_action(&mut svc, Arc::new(play))?;

        let mut stop = Action::new("Stop".to_string());
        add_arg_in(&mut stop, "InstanceID", &AVT_INSTANCE_ID)?;
        stop.set_handler(handlers::stop_handler(pipeline.clone(), state.clone()));
        add_action(&mut svc, Arc::new(stop))?;

        let mut pause = Action::new("Pause".to_string());
        add_arg_in(&mut pause, "InstanceID", &AVT_INSTANCE_ID)?;
        pause.set_handler(handlers::pause_handler(pipeline.clone(), state.clone()));
        add_action(&mut svc, Arc::new(pause))?;

        let mut next = Action::new("Next".to_string());
        add_arg_in(&mut next, "InstanceID", &AVT_INSTANCE_ID)?;
        next.set_handler(handlers::next_handler(pipeline.clone()));
        add_action(&mut svc, Arc::new(next))?;

        let mut previous = Action::new("Previous".to_string());
        add_arg_in(&mut previous, "InstanceID", &AVT_INSTANCE_ID)?;
        previous.set_handler(handlers::previous_handler(pipeline.clone()));
        add_action(&mut svc, Arc::new(previous))?;

        let mut seek = Action::new("Seek".to_string());
        add_arg_in(&mut seek, "InstanceID", &AVT_INSTANCE_ID)?;
        add_arg_in(&mut seek, "Unit", &A_ARG_TYPE_SEEKMODE)?;
        add_arg_in(&mut seek, "Target", &SEEKMODE)?;
        seek.set_handler(handlers::seek_handler(pipeline.clone()));
        add_action(&mut svc, Arc::new(seek))?;

        let mut set_uri = Action::new("SetAVTransportURI".to_string());
        add_arg_in(&mut set_uri, "InstanceID", &AVT_INSTANCE_ID)?;
        add_arg_in(&mut set_uri, "CurrentURI", &AVTRANSPORTURI)?;
        add_arg_in(&mut set_uri, "CurrentURIMetaData", &AVTRANSPORTURIMETADATA)?;
        set_uri.set_handler(handlers::set_uri_handler(pipeline.clone(), state.clone()));
        add_action(&mut svc, Arc::new(set_uri))?;

        let mut set_next_uri = Action::new("SetNextAVTransportURI".to_string());
        add_arg_in(&mut set_next_uri, "InstanceID", &AVT_INSTANCE_ID)?;
        add_arg_in(&mut set_next_uri, "NextURI", &AVTRANSPORTNEXTURI)?;
        add_arg_in(
            &mut set_next_uri,
            "NextURIMetaData",
            &AVTRANSPORTNEXTURIMETADATA,
        )?;
        set_next_uri.set_handler(handlers::set_next_uri_handler(
            pipeline.clone(),
            state.clone(),
        ));
        add_action(&mut svc, Arc::new(set_next_uri))?;

        let mut get_pos = Action::new("GetPositionInfo".to_string());
        add_arg_in(&mut get_pos, "InstanceID", &AVT_INSTANCE_ID)?;
        add_arg_out(&mut get_pos, "Track", &CURRENTTRACK)?;
        add_arg_out(&mut get_pos, "TrackDuration", &CURRENTTRACKDURATION)?;
        add_arg_out(&mut get_pos, "TrackURI", &CURRENTTRACKURI)?;
        add_arg_out(&mut get_pos, "TrackMetaData", &CURRENTTRACKMETADATA)?;
        add_arg_out(&mut get_pos, "RelTime", &RELATIVETIMEPOSITION)?;
        add_arg_out(&mut get_pos, "AbsTime", &ABSOLUTETIMEPOSITION)?;
        get_pos.set_stateful(false);
        get_pos.set_handler(handlers::get_position_info_handler(state.clone()));
        add_action(&mut svc, Arc::new(get_pos))?;

        let mut get_info = Action::new("GetTransportInfo".to_string());
        add_arg_in(&mut get_info, "InstanceID", &AVT_INSTANCE_ID)?;
        add_arg_out(&mut get_info, "CurrentTransportState", &TRANSPORTSTATE)?;
        add_arg_out(&mut get_info, "CurrentTransportStatus", &TRANSPORTSTATUS)?;
        add_arg_out(&mut get_info, "CurrentSpeed", &TRANSPORTPLAYSPEED)?;
        get_info.set_stateful(false);
        get_info.set_handler(handlers::get_transport_info_handler(state.clone()));
        add_action(&mut svc, Arc::new(get_info))?;

        let mut get_media = Action::new("GetMediaInfo".to_string());
        add_arg_in(&mut get_media, "InstanceID", &AVT_INSTANCE_ID)?;
        add_arg_out(&mut get_media, "NrTracks", &NUMBEROFTRACKS)?;
        add_arg_out(&mut get_media, "CurrentURI", &AVTRANSPORTURI)?;
        add_arg_out(
            &mut get_media,
            "CurrentURIMetaData",
            &AVTRANSPORTURIMETADATA,
        )?;
        add_arg_out(&mut get_media, "NextURI", &AVTRANSPORTNEXTURI)?;
        add_arg_out(
            &mut get_media,
            "NextURIMetaData",
            &AVTRANSPORTNEXTURIMETADATA,
        )?;
        get_media.set_stateful(false);
        get_media.set_handler(handlers::get_media_info_handler(state.clone()));
        add_action(&mut svc, Arc::new(get_media))?;

        let mut get_settings = Action::new("GetTransportSettings".to_string());
        add_arg_in(&mut get_settings, "InstanceID", &AVT_INSTANCE_ID)?;
        add_action(&mut svc, Arc::new(get_settings))?;

        let mut get_caps = Action::new("GetDeviceCapabilities".to_string());
        add_arg_in(&mut get_caps, "InstanceID", &AVT_INSTANCE_ID)?;
        add_action(&mut svc, Arc::new(get_caps))?;

        let mut get_actions = Action::new("GetCurrentTransportActions".to_string());
        add_arg_in(&mut get_actions, "InstanceID", &AVT_INSTANCE_ID)?;
        add_action(&mut svc, Arc::new(get_actions))?;

        Ok(svc)
    }

    fn build_renderingcontrol(state: SharedState) -> Result<Service, FactoryError> {
        let mut svc = Service::new("RenderingControl".to_string());

        add_var(&mut svc, &RC_INSTANCE_ID)?;
        add_var(&mut svc, &A_ARG_TYPE_CHANNEL)?;
        add_var(&mut svc, &VOLUME)?;
        add_var(&mut svc, &MUTE)?;

        let mut set_vol = Action::new("SetVolume".to_string());
        add_arg_in(&mut set_vol, "InstanceID", &RC_INSTANCE_ID)?;
        add_arg_in(&mut set_vol, "Channel", &A_ARG_TYPE_CHANNEL)?;
        add_arg_in(&mut set_vol, "DesiredVolume", &VOLUME)?;
        set_vol.set_handler(handlers::set_volume_handler(state.clone()));
        add_action(&mut svc, Arc::new(set_vol))?;

        let mut get_vol = Action::new("GetVolume".to_string());
        add_arg_in(&mut get_vol, "InstanceID", &RC_INSTANCE_ID)?;
        add_arg_in(&mut get_vol, "Channel", &A_ARG_TYPE_CHANNEL)?;
        add_arg_out(&mut get_vol, "CurrentVolume", &VOLUME)?;
        get_vol.set_stateful(false);
        get_vol.set_handler(handlers::get_volume_handler(state.clone()));
        add_action(&mut svc, Arc::new(get_vol))?;

        let mut set_mute = Action::new("SetMute".to_string());
        add_arg_in(&mut set_mute, "InstanceID", &RC_INSTANCE_ID)?;
        add_arg_in(&mut set_mute, "Channel", &A_ARG_TYPE_CHANNEL)?;
        add_arg_in(&mut set_mute, "DesiredMute", &MUTE)?;
        set_mute.set_handler(handlers::set_mute_handler(state.clone()));
        add_action(&mut svc, Arc::new(set_mute))?;

        let mut get_mute = Action::new("GetMute".to_string());
        add_arg_in(&mut get_mute, "InstanceID", &RC_INSTANCE_ID)?;
        add_arg_in(&mut get_mute, "Channel", &A_ARG_TYPE_CHANNEL)?;
        add_arg_out(&mut get_mute, "CurrentMute", &MUTE)?;
        get_mute.set_stateful(false);
        get_mute.set_handler(handlers::get_mute_handler(state.clone()));
        add_action(&mut svc, Arc::new(get_mute))?;

        Ok(svc)
    }

    fn build_connectionmanager() -> Result<Service, FactoryError> {
        let mut svc = Service::new("ConnectionManager".to_string());

        add_var(&mut svc, &A_ARG_TYPE_CONNECTIONID)?;
        add_var(&mut svc, &A_ARG_TYPE_CONNECTIONSTATUS)?;
        add_var(&mut svc, &A_ARG_TYPE_DIRECTION)?;
        add_var(&mut svc, &A_ARG_TYPE_PROTOCOLINFO)?;
        add_var(&mut svc, &A_ARG_TYPE_RCSID)?;
        add_var(&mut svc, &A_ARG_TYPE_AVTRANSPORTID)?;
        add_var(&mut svc, &CURRENTCONNECTIONIDS)?;
        add_var(&mut svc, &SINKPROTOCOLINFO)?;
        add_var(&mut svc, &SOURCEPROTOCOLINFO)?;

        let mut get_proto = Action::new("GetProtocolInfo".to_string());
        add_arg_out(&mut get_proto, "Source", &SOURCEPROTOCOLINFO)?;
        add_arg_out(&mut get_proto, "Sink", &SINKPROTOCOLINFO)?;
        get_proto.set_stateful(false);
        get_proto.set_handler(handlers::get_protocol_info_handler());
        add_action(&mut svc, Arc::new(get_proto))?;

        let mut get_ids = Action::new("GetCurrentConnectionIDs".to_string());
        add_arg_out(&mut get_ids, "ConnectionIDs", &CURRENTCONNECTIONIDS)?;
        add_action(&mut svc, Arc::new(get_ids))?;

        let mut get_conn = Action::new("GetCurrentConnectionInfo".to_string());
        add_arg_in(&mut get_conn, "ConnectionID", &A_ARG_TYPE_CONNECTIONID)?;
        add_arg_out(&mut get_conn, "RcsID", &A_ARG_TYPE_RCSID)?;
        add_arg_out(&mut get_conn, "AVTransportID", &A_ARG_TYPE_AVTRANSPORTID)?;
        add_arg_out(&mut get_conn, "ProtocolInfo", &A_ARG_TYPE_PROTOCOLINFO)?;
        add_arg_out(
            &mut get_conn,
            "PeerConnectionManager",
            &A_ARG_TYPE_PROTOCOLINFO,
        )?;
        add_arg_out(&mut get_conn, "PeerConnectionID", &A_ARG_TYPE_CONNECTIONID)?;
        add_arg_out(&mut get_conn, "Direction", &A_ARG_TYPE_DIRECTION)?;
        add_arg_out(&mut get_conn, "Status", &A_ARG_TYPE_CONNECTIONSTATUS)?;
        add_action(&mut svc, Arc::new(get_conn))?;

        Ok(svc)
    }
}
