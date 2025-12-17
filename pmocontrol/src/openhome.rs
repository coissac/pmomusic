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

pub struct OhServiceEndpoint<'a> {
    pub control_url: &'a str,
    pub service_type: &'a str,
}

fn endpoint_for<'a>(info: &'a RendererInfo, kind: OhServiceKind) -> Option<OhServiceEndpoint<'a>> {
    let (control_url, service_type) = match kind {
        OhServiceKind::Playlist => (
            info.oh_playlist_control_url.as_deref()?,
            info.oh_playlist_service_type.as_deref()?,
        ),
        OhServiceKind::Info => (
            info.oh_info_control_url.as_deref()?,
            info.oh_info_service_type.as_deref()?,
        ),
        OhServiceKind::Time => (
            info.oh_time_control_url.as_deref()?,
            info.oh_time_service_type.as_deref()?,
        ),
        OhServiceKind::Volume => (
            info.oh_volume_control_url.as_deref()?,
            info.oh_volume_service_type.as_deref()?,
        ),
        OhServiceKind::Product => (
            info.oh_product_control_url.as_deref()?,
            info.oh_product_service_type.as_deref()?,
        ),
    };

    Some(OhServiceEndpoint {
        control_url,
        service_type,
    })
}

pub fn control_url_for<'a>(info: &'a RendererInfo, kind: OhServiceKind) -> Option<&'a str> {
    endpoint_for(info, kind).map(|endpoint| endpoint.control_url)
}

pub fn service_type_for<'a>(info: &'a RendererInfo, kind: OhServiceKind) -> Option<&'a str> {
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
    let control_url = info.oh_radio_control_url.as_ref()?;
    let service_type = info.oh_radio_service_type.as_ref()?;
    Some(OhRadioClient::new(
        control_url.clone(),
        service_type.clone(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RendererCapabilities, RendererId, RendererInfo, RendererProtocol};

    fn sample_renderer_info() -> RendererInfo {
        RendererInfo {
            id: RendererId("renderer".into()),
            udn: "renderer".into(),
            friendly_name: "Renderer".into(),
            model_name: "Model".into(),
            manufacturer: "Maker".into(),
            protocol: RendererProtocol::OpenHomeOnly,
            capabilities: RendererCapabilities::default(),
            location: "http://host:1234/description.xml".into(),
            server_header: "test".into(),
            online: true,
            last_seen: std::time::SystemTime::now(),
            max_age: 1800,
            avtransport_service_type: None,
            avtransport_control_url: None,
            rendering_control_service_type: None,
            rendering_control_control_url: None,
            connection_manager_service_type: None,
            connection_manager_control_url: None,
            oh_playlist_service_type: Some("urn:av-openhome-org:service:Playlist:1".into()),
            oh_playlist_control_url: Some("http://host/oh/playlist".into()),
            oh_playlist_event_sub_url: Some("http://host/events/playlist".into()),
            oh_info_service_type: Some("urn:av-openhome-org:service:Info:1".into()),
            oh_info_control_url: Some("http://host/oh/info".into()),
            oh_info_event_sub_url: Some("http://host/events/info".into()),
            oh_time_service_type: Some("urn:av-openhome-org:service:Time:1".into()),
            oh_time_control_url: Some("http://host/oh/time".into()),
            oh_time_event_sub_url: Some("http://host/events/time".into()),
            oh_volume_service_type: Some("urn:av-openhome-org:service:Volume:1".into()),
            oh_volume_control_url: Some("http://host/oh/volume".into()),
            oh_radio_service_type: None,
            oh_radio_control_url: None,
            oh_product_service_type: Some("urn:av-openhome-org:service:Product:1".into()),
            oh_product_control_url: Some("http://host/oh/product".into()),
        }
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
    fn returns_none_when_service_missing() {
        let mut info = sample_renderer_info();
        info.oh_playlist_control_url = None;
        assert!(endpoint_for(&info, OhServiceKind::Playlist).is_none());
    }

    #[test]
    fn info_and_playlist_use_different_urls() {
        let info = sample_renderer_info();
        let playlist = control_url_for(&info, OhServiceKind::Playlist).unwrap();
        let info_url = control_url_for(&info, OhServiceKind::Info).unwrap();
        assert_ne!(playlist, info_url);
    }
}
