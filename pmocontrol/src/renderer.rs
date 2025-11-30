use anyhow::{anyhow, Result};

use crate::rendering_control_client::RenderingControlClient;
use crate::{AvTransportClient, DeviceRegistry, RendererId, RendererInfo};

/// High-level handle representing a renderer and its optional AVTransport client.
pub struct Renderer {
    pub info: RendererInfo,
    avtransport: Option<AvTransportClient>,
    rendering_control: Option<RenderingControlClient>,
}

impl Renderer {
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

    pub fn play_uri(&self, uri: &str, meta: &str) -> Result<()> {
        let avt = self.avtransport()?;
        avt.set_av_transport_uri(uri, meta)?;
        avt.play(0, "1")
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

    pub fn from_registry(info: RendererInfo, registry: &DeviceRegistry) -> Self {
        let avtransport = registry.avtransport_client_for_renderer(&info.id);
        let rendering_control = registry.rendering_control_client_for_renderer(&info.id);
        Self {
            info,
            avtransport,
            rendering_control,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RendererCapabilities, RendererProtocol};
    use crate::registry::{DeviceRegistry, DeviceUpdate};
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
        }
    }

    fn registry_with_renderer(info: RendererInfo) -> DeviceRegistry {
        let mut registry = DeviceRegistry::new();
        registry.apply_update(DeviceUpdate::RendererOnline(info));
        registry
    }

    #[test]
    fn renderer_without_avtransport() {
        let info = renderer_info("no-avt", false);
        let registry = registry_with_renderer(info.clone());
        let renderer = Renderer::from_registry(info, &registry);

        assert_eq!(renderer.has_avtransport(), false);
        assert_eq!(renderer.id().0, "renderer-no-avt");
    }

    #[test]
    fn renderer_with_avtransport() {
        let info = renderer_info("with-avt", true);
        let registry = registry_with_renderer(info.clone());
        let renderer = Renderer::from_registry(info, &registry);

        assert!(renderer.has_avtransport());
        assert_eq!(renderer.friendly_name(), "Renderer with-avt");
    }
}
