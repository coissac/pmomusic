use crate::model::RendererInfo;
use crate::openhome_client::{
    OhInfoClient, OhPlaylistClient, OhProductClient, OhRadioClient, OhTimeClient, OhVolumeClient,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OhServiceKind {
    Playlist,
    Info,
    Time,
    Volume,
    Product,
}

impl OhServiceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            OhServiceKind::Playlist => "playlist",
            OhServiceKind::Info => "info",
            OhServiceKind::Time => "time",
            OhServiceKind::Volume => "volume",
            OhServiceKind::Product => "product",
        }
    }
}

pub struct OhServiceEndpoint {
    pub control_url: String,
    pub service_type: String,
}

fn endpoint_for(info: &RendererInfo, kind: OhServiceKind) -> Option<OhServiceEndpoint> {
    let (control_url, service_type) = match kind {
        OhServiceKind::Playlist => (
            info.oh_playlist_control_url()?,
            info.oh_playlist_service_type()?,
        ),
        OhServiceKind::Info => (
            info.oh_info_control_url()?,
            info.oh_info_service_type()?,
        ),
        OhServiceKind::Time => (
            info.oh_time_control_url()?,
            info.oh_time_service_type()?,
        ),
        OhServiceKind::Volume => (
            info.oh_volume_control_url()?,
            info.oh_volume_service_type()?,
        ),
        OhServiceKind::Product => (
            info.oh_product_control_url()?,
            info.oh_product_service_type()?,
        ),
    };

    Some(OhServiceEndpoint {
        control_url: control_url,
        service_type: service_type,
    })
}

pub fn control_url_for(info: &RendererInfo, kind: OhServiceKind) -> Option<String> {
    endpoint_for(info, kind).map(|endpoint| endpoint.control_url)
}

pub fn service_type_for(info: &RendererInfo, kind: OhServiceKind) -> Option<String> {
    endpoint_for(info, kind).map(|endpoint| endpoint.service_type)
}

pub fn build_playlist_client(info: &RendererInfo) -> Option<OhPlaylistClient> {
    let endpoint = endpoint_for(info, OhServiceKind::Playlist)?;
    Some(OhPlaylistClient::new(
        endpoint.control_url.to_string(),
        endpoint.service_type.to_string(),
    ))
}

pub fn build_info_client(info: &RendererInfo) -> Option<OhInfoClient> {
    let endpoint = endpoint_for(info, OhServiceKind::Info)?;
    Some(OhInfoClient::new(
        endpoint.control_url.to_string(),
        endpoint.service_type.to_string(),
    ))
}

pub fn build_time_client(info: &RendererInfo) -> Option<OhTimeClient> {
    let endpoint = endpoint_for(info, OhServiceKind::Time)?;
    Some(OhTimeClient::new(
        endpoint.control_url.to_string(),
        endpoint.service_type.to_string(),
    ))
}

pub fn build_volume_client(info: &RendererInfo) -> Option<OhVolumeClient> {
    let endpoint = endpoint_for(info, OhServiceKind::Volume)?;
    Some(OhVolumeClient::new(
        endpoint.control_url.to_string(),
        endpoint.service_type.to_string(),
    ))
}

pub fn build_product_client(info: &RendererInfo) -> Option<OhProductClient> {
    let endpoint = endpoint_for(info, OhServiceKind::Product)?;
    Some(OhProductClient::new(
        endpoint.control_url.to_string(),
        endpoint.service_type.to_string(),
    ))
}

pub fn build_radio_client(info: &RendererInfo) -> Option<OhRadioClient> {
    let control_url = info.oh_radio_control_url()?;
    let service_type = info.oh_radio_service_type()?;
    Some(OhRadioClient::new(
        control_url,
        service_type,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DeviceId, model::{RendererCapabilities, RendererInfo, RendererProtocol}};

    fn sample_renderer_info() -> RendererInfo {
        RendererInfo::make(
            DeviceId("renderer".into()),
            "renderer".into(),
            "Renderer".into(),
            "Model".into(),
            "Maker".into(),
            RendererProtocol::OpenHomeOnly,
            RendererCapabilities::default(),
            "http://host:1234/description.xml".into(),
            "test".into(),
            None,
            None,
            None,
            None,
            None,
            None,
            Some("urn:av-openhome-org:service:Playlist:1".into()),
            Some("http://host/oh/playlist".into()),
            Some("http://host/events/playlist".into()),
            Some("urn:av-openhome-org:service:Info:1".into()),
            Some("http://host/oh/info".into()),
            Some("http://host/events/info".into()),
            Some("urn:av-openhome-org:service:Time:1".into()),
            Some("http://host/oh/time".into()),
            Some("http://host/events/time".into()),
            Some("urn:av-openhome-org:service:Volume:1".into()),
            Some("http://host/oh/volume".into()),
            None,
            None,
            Some("urn:av-openhome-org:service:Product:1".into()),
            Some("http://host/oh/product".into()),
        )
    }

    #[test]
    fn selects_correct_playlist_endpoint() {
        let info = sample_renderer_info();
        let endpoint = endpoint_for(&info, OhServiceKind::Playlist).unwrap();
        assert_eq!(endpoint.control_url, "http://host/oh/playlist");
        assert_eq!(
            endpoint.service_type,
            "urn:av-openhome-org:service:Playlist:1"
        );
    }

    #[test]
    fn info_and_playlist_use_different_urls() {
        let info = sample_renderer_info();
        let playlist = control_url_for(&info, OhServiceKind::Playlist).unwrap();
        let info_url = control_url_for(&info, OhServiceKind::Info).unwrap();
        assert_ne!(playlist, info_url);
    }
}
