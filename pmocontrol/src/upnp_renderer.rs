use std::sync::{Arc, RwLock};

use anyhow::{Result, anyhow};

use crate::capabilities::{PlaybackPositionInfo, PlaybackStatus};
use crate::connection_manager_client::{ConnectionInfo, ConnectionManagerClient, ProtocolInfo};
use crate::music_renderer::op_not_supported;
use crate::rendering_control_client::RenderingControlClient;
use crate::{
    AvTransportClient, DeviceRegistry, PlaybackPosition, PlaybackState, PositionInfo, RendererId,
    RendererInfo, TransportControl, VolumeControl,
};

/// High-level handle representing a renderer and its optional AVTransport client.
#[derive(Clone, Debug)]
pub struct UpnpRenderer {
    pub info: RendererInfo,
    registry: Arc<RwLock<DeviceRegistry>>,
    avtransport: Option<AvTransportClient>,
    rendering_control: Option<RenderingControlClient>,
    connection_manager: Option<ConnectionManagerClient>,
}

impl UpnpRenderer {
    pub fn id(&self) -> &RendererId {
        &self.info.id
    }

    pub fn friendly_name(&self) -> &str {
        &self.info.friendly_name
    }

    pub fn has_avtransport(&self) -> bool {
        self.avtransport.is_some()
    }

    pub fn has_rendering_control(&self) -> bool {
        self.rendering_control.is_some()
    }

    pub fn has_connection_manager(&self) -> bool {
        self.connection_manager.is_some()
    }

    /// Returns true if this renderer is known to support SetNextAVTransportURI.
    pub fn supports_set_next(&self) -> bool {
        self.info.capabilities.supports_set_next()
    }

    pub fn avtransport(&self) -> Result<&AvTransportClient> {
        self.avtransport
            .as_ref()
            .ok_or_else(|| anyhow!("Renderer has no AVTransport service"))
    }

    pub fn rendering_control(&self) -> Result<&RenderingControlClient> {
        self.rendering_control
            .as_ref()
            .ok_or_else(|| anyhow!("Renderer has no RenderingControl service"))
    }

    pub fn connection_manager(&self) -> Result<&ConnectionManagerClient> {
        self.connection_manager
            .as_ref()
            .ok_or_else(|| anyhow!("Renderer has no ConnectionManager service"))
    }

    pub fn play_uri(&self, uri: &str, meta: &str) -> Result<()> {
        let avt = self.avtransport()?;
        avt.set_av_transport_uri(uri, meta)?;
        avt.play(0, "1")
    }

    /// Best-effort attempt to configure the next URI via AVTransport SetNextAVTransportURI.
    pub fn set_next_uri(&self, next_uri: &str, next_meta: &str) -> Result<()> {
        if !self.info.capabilities.has_avtransport {
            return Err(op_not_supported("SetNextAVTransportURI", "AVTransport"));
        }

        let client = self.avtransport()?;
        let result = client.set_next_av_transport_uri(next_uri, next_meta);

        if result.is_ok() {
            let mut reg = self.registry.write().unwrap();
            reg.mark_renderer_supports_set_next(&self.info.id);
        }

        result
    }

    pub fn pause(&self) -> Result<()> {
        let avt = self.avtransport()?;
        avt.pause(0)
    }

    pub fn stop(&self) -> Result<()> {
        let avt = self.avtransport()?;
        avt.stop(0)
    }

    pub fn seek_rel_time(&self, hhmmss: &str) -> Result<()> {
        let avt = self.avtransport()?;
        avt.seek(0, "REL_TIME", hhmmss)
    }

    pub fn get_master_volume(&self) -> Result<u16> {
        let rc = self.rendering_control()?;
        rc.get_volume(0, "Master")
    }

    pub fn set_master_volume(&self, volume: u16) -> Result<()> {
        let rc = self.rendering_control()?;
        rc.set_volume(0, "Master", volume)
    }

    pub fn get_master_mute(&self) -> Result<bool> {
        let rc = self.rendering_control()?;
        rc.get_mute(0, "Master")
    }

    pub fn set_master_mute(&self, mute: bool) -> Result<()> {
        let rc = self.rendering_control()?;
        rc.set_mute(0, "Master", mute)
    }

    pub fn protocol_info(&self) -> Result<ProtocolInfo> {
        let cm = self.connection_manager()?;
        cm.get_protocol_info()
    }

    pub fn connection_ids(&self) -> Result<Vec<i32>> {
        let cm = self.connection_manager()?;
        cm.get_current_connection_ids()
    }

    pub fn connection_info(&self, connection_id: i32) -> Result<ConnectionInfo> {
        let cm = self.connection_manager()?;
        cm.get_current_connection_info(connection_id)
    }

    pub fn from_registry(info: RendererInfo, registry: &Arc<RwLock<DeviceRegistry>>) -> Self {
        let (avtransport, rendering_control, connection_manager) = {
            let reg = registry.read().unwrap();
            (
                reg.avtransport_client_for_renderer(&info.id),
                reg.rendering_control_client_for_renderer(&info.id),
                reg.connection_manager_client_for_renderer(&info.id),
            )
        };
        Self {
            info,
            registry: Arc::clone(registry),
            avtransport,
            rendering_control,
            connection_manager,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RendererCapabilities, RendererProtocol};
    use crate::registry::{DeviceRegistry, DeviceUpdate};
    use std::sync::{Arc, RwLock};
    use std::time::SystemTime;

    fn renderer_info(id_suffix: &str, with_avtransport: bool) -> RendererInfo {
        RendererInfo {
            id: RendererId(format!("renderer-{id_suffix}")),
            udn: format!("uuid:renderer-{id_suffix}"),
            friendly_name: format!("Renderer {id_suffix}"),
            model_name: "Model".into(),
            manufacturer: "Manufacturer".into(),
            protocol: RendererProtocol::UpnpAvOnly,
            capabilities: RendererCapabilities {
                has_avtransport: with_avtransport,
                ..RendererCapabilities::default()
            },
            location: "http://127.0.0.1/device.xml".into(),
            server_header: "TestServer/1.0".into(),
            online: true,
            last_seen: SystemTime::now(),
            max_age: 1800,
            avtransport_service_type: with_avtransport
                .then(|| "urn:schemas-upnp-org:service:AVTransport:1".into()),
            avtransport_control_url: with_avtransport
                .then(|| "http://127.0.0.1/avtransport".into()),
            rendering_control_service_type: None,
            rendering_control_control_url: None,
            connection_manager_service_type: None,
            connection_manager_control_url: None,
            oh_playlist_service_type: None,
            oh_playlist_control_url: None,
            oh_playlist_event_sub_url: None,
            oh_info_service_type: None,
            oh_info_control_url: None,
            oh_info_event_sub_url: None,
            oh_time_service_type: None,
            oh_time_control_url: None,
            oh_time_event_sub_url: None,
            oh_volume_service_type: None,
            oh_volume_control_url: None,
            oh_radio_service_type: None,
            oh_radio_control_url: None,
        }
    }

    fn registry_with_renderer(info: RendererInfo) -> Arc<RwLock<DeviceRegistry>> {
        let mut registry = DeviceRegistry::new();
        registry.apply_update(DeviceUpdate::RendererOnline(info));
        Arc::new(RwLock::new(registry))
    }

    #[test]
    fn renderer_without_avtransport() {
        let info = renderer_info("no-avt", false);
        let registry = registry_with_renderer(info.clone());
        let renderer = UpnpRenderer::from_registry(info, &registry);

        assert_eq!(renderer.has_avtransport(), false);
        assert_eq!(renderer.id().0, "renderer-no-avt");
    }

    #[test]
    fn renderer_with_avtransport() {
        let info = renderer_info("with-avt", true);
        let registry = registry_with_renderer(info.clone());
        let renderer = UpnpRenderer::from_registry(info, &registry);

        assert!(renderer.has_avtransport());
        assert_eq!(renderer.friendly_name(), "Renderer with-avt");
    }
}

/// Implémentation UPnP AV de `TransportControl` pour [`UpnpRenderer`].
///
/// Cette impl se base sur AVTransport (InstanceID = 0).
impl TransportControl for UpnpRenderer {
    fn play_uri(&self, uri: &str, meta: &str) -> Result<()> {
        let avt = self.avtransport()?;
        avt.set_av_transport_uri(uri, meta)?;
        avt.play(0, "1")
    }

    fn play(&self) -> Result<()> {
        let avt = self.avtransport()?;
        avt.play(0, "1")
    }

    fn pause(&self) -> Result<()> {
        let avt = self.avtransport()?;
        avt.pause(0)
    }

    fn stop(&self) -> Result<()> {
        let avt = self.avtransport()?;
        avt.stop(0)
    }

    fn seek_rel_time(&self, hhmmss: &str) -> Result<()> {
        let avt = self.avtransport()?;
        avt.seek(0, "REL_TIME", hhmmss)
    }
}

/// Implémentation UPnP RenderingControl de `VolumeControl` pour [`UpnpRenderer`].
///
/// Cette impl se base sur le channel "Master" (InstanceID = 0).
impl VolumeControl for UpnpRenderer {
    fn volume(&self) -> Result<u16> {
        self.get_master_volume()
    }

    fn set_volume(&self, v: u16) -> Result<()> {
        self.set_master_volume(v)
    }

    fn mute(&self) -> Result<bool> {
        self.get_master_mute()
    }

    fn set_mute(&self, m: bool) -> Result<()> {
        self.set_master_mute(m)
    }
}

/// Implémentation UPnP AV de `PlaybackStatus` pour [`UpnpRenderer`].
///
/// Utilise AVTransport::GetTransportInfo(InstanceID=0).
impl PlaybackStatus for UpnpRenderer {
    fn playback_state(&self) -> Result<PlaybackState> {
        let avt = self.avtransport()?;
        let info = avt.get_transport_info(0)?;
        Ok(PlaybackState::from_upnp_state(
            &info.current_transport_state,
        ))
    }
}

impl PlaybackPosition for UpnpRenderer {
    fn playback_position(&self) -> Result<PlaybackPositionInfo> {
        let avt = self.avtransport()?;
        let raw: PositionInfo = avt.get_position_info(0)?;

        Ok(PlaybackPositionInfo {
            track: Some(raw.track),
            rel_time: raw.rel_time,
            abs_time: raw.abs_time,
            track_duration: raw.track_duration,
            track_metadata: raw.track_metadata,
            track_uri: raw.track_uri,
        })
    }
}
