use std::io::BufReader;
use std::time::Duration;

use quick_xml::{Error as XmlError, Reader, events::Event};
use thiserror::Error;
use tracing::{debug};

use crate::DeviceId;
use crate::discovery::arylic::detect_arylic_tcp;
use crate::linkplay_client::{extract_linkplay_host, fetch_status_for_host};
use crate::media_server::UpnpMediaServer;
use crate::model::{RendererCapabilities, RendererInfo, RendererProtocol};
use crate::upnp_clients::{AvTransportClient,resolve_control_url};

use ureq::Agent;

#[derive(Debug, Error)]
pub enum DescriptionError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] ureq::Error),

    #[error("Failed to read HTTP body: {0}")]
    HttpIo(#[from] std::io::Error),

    #[error("XML parsing error: {0}")]
    Xml(#[from] quick_xml::Error),

    #[error("Missing required device element: {0}")]
    MissingField(&'static str),
}

/// Parsed device description, plus (optionally) AVTransport endpoint.
#[derive(Debug, Default)]
pub struct ParsedDeviceDescription {
    timeout_secs: u64,
    udn: String,
    location: String,
    server_header: String,
    device_type: Option<String>,
    friendly_name: Option<String>,
    manufacturer: Option<String>,
    model_name: Option<String>,
    service_types: Vec<String>,

    // New: AVTransport endpoint (if present in serviceList)
    avtransport_service_type: Option<String>,
    avtransport_control_url: Option<String>,

    // RenderingControl endpoint (if present in serviceList)
    rendering_control_service_type: Option<String>,
    rendering_control_control_url: Option<String>,

    // ConnectionManager endpoint (if present in serviceList)
    connection_manager_service_type: Option<String>,
    connection_manager_control_url: Option<String>,

    // ContentDirectory endpoint (if present in serviceList)
    content_directory_service_type: Option<String>,
    content_directory_control_url: Option<String>,

    // OpenHome endpoints (if present in serviceList)
    oh_playlist_service_type: Option<String>,
    oh_playlist_control_url: Option<String>,
    oh_playlist_event_sub_url: Option<String>,
    oh_info_service_type: Option<String>,
    oh_info_control_url: Option<String>,
    oh_info_event_sub_url: Option<String>,
    oh_time_service_type: Option<String>,
    oh_time_control_url: Option<String>,
    oh_time_event_sub_url: Option<String>,
    oh_volume_service_type: Option<String>,
    oh_volume_control_url: Option<String>,
    oh_radio_service_type: Option<String>,
    oh_radio_control_url: Option<String>,
    oh_product_service_type: Option<String>,
    oh_product_control_url: Option<String>,
}

impl ParsedDeviceDescription {

    /// Fetch and parse the device description.xml at endpoint.location.
    pub fn new(
        udn: &str,
        location: &str,
        server_header: &str,
        timeout_secs: u64,
    ) -> Result<Self, DescriptionError> {
        debug!("Fetching description for {} at {}", udn, location);

        let config = Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(timeout_secs)))
            .build();

        let agent: Agent = config.into();

        let response = agent.get(location).call()?;

        // response: http::Response<ureq::Body>
        let (_parts, body) = response.into_parts();

        // body.into_reader() -> impl Read + 'static
        let body_reader = body.into_reader();

        let mut reader = Reader::from_reader(BufReader::new(body_reader));
        reader.config_mut().trim_text(true);
        debug!("Parsing description XML for {} at {}", udn, location);

        let mut buf = Vec::new();
        let mut parsed = ParsedDeviceDescription::default();

        parsed.timeout_secs=timeout_secs;
        parsed.location = location.to_string();
        parsed.udn = udn.to_string();
        parsed.server_header = server_header.to_string();

        let mut in_device = false;
        let mut in_service = false;
        let mut current_tag: Option<String> = None;

        // New: track current serviceType + controlURL while inside <service>...</service>
        let mut current_service_type: Option<String> = None;
        let mut current_control_url: Option<String> = None;
        let mut current_event_sub_url: Option<String> = None;

        loop {
            match reader.read_event_into(&mut buf)? {
                Event::Start(e) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match name.as_str() {
                        "device" => {
                            in_device = true;
                            current_tag = None;
                        }
                        "service" => {
                            if in_device {
                                in_service = true;
                                current_tag = None;
                                current_service_type = None;
                                current_control_url = None;
                            }
                        }
                        _ => {
                            if in_device {
                                current_tag = Some(name);
                            }
                        }
                    }
                }
                Event::End(e) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match name.as_str() {
                        "device" => {
                            in_device = false;
                        }
                        "service" => {
                            if in_device && in_service {
                                // We just finished a <service> block: if this is AVTransport,
                                // store its endpoint in parsed.*
                                if let (Some(st), Some(ctrl)) =
                                    (&current_service_type, &current_control_url)
                                {
                                    let lower = st.to_ascii_lowercase();
                                    if lower.contains("urn:schemas-upnp-org:service:avtransport:") {
                                        // Only set once; if multiple AVTransport services exist,
                                        // we keep the first one.
                                        if parsed.avtransport_service_type.is_none() {
                                            parsed.avtransport_service_type = Some(st.clone());
                                            parsed.avtransport_control_url = Some(ctrl.clone());
                                            debug!(
                                                "Found AVTransport service for {}: type={} controlURL={}",
                                                udn, st, ctrl
                                            );
                                        }
                                    }

                                    if lower
                                        .contains("urn:schemas-upnp-org:service:renderingcontrol:")
                                    {
                                        if parsed.rendering_control_service_type.is_none() {
                                            parsed.rendering_control_service_type =
                                                Some(st.clone());
                                            parsed.rendering_control_control_url =
                                                Some(ctrl.clone());
                                            debug!(
                                                "Found RenderingControl service for {}: type={} controlURL={}",
                                                udn, st, ctrl
                                            );
                                        }
                                    }

                                    if lower
                                        .contains("urn:schemas-upnp-org:service:connectionmanager:")
                                    {
                                        if parsed.connection_manager_service_type.is_none() {
                                            parsed.connection_manager_service_type =
                                                Some(st.clone());
                                            parsed.connection_manager_control_url =
                                                Some(ctrl.clone());
                                            debug!(
                                                "Found ConnectionManager service for {}: type={} controlURL={}",
                                                udn, st, ctrl
                                            );
                                        }
                                    }

                                    if lower
                                        .contains("urn:schemas-upnp-org:service:contentdirectory:")
                                    {
                                        if parsed.content_directory_service_type.is_none() {
                                            parsed.content_directory_service_type =
                                                Some(st.clone());
                                            parsed.content_directory_control_url =
                                                Some(ctrl.clone());
                                            debug!(
                                                "Found ContentDirectory service for {}: type={} controlURL={}",
                                                udn, st, ctrl
                                            );
                                        }
                                    }

                                    if lower.contains("urn:av-openhome-org:service:playlist:") {
                                        if parsed.oh_playlist_service_type.is_none() {
                                            parsed.oh_playlist_service_type = Some(st.clone());
                                            parsed.oh_playlist_control_url = Some(ctrl.clone());
                                            if parsed.oh_playlist_event_sub_url.is_none() {
                                                parsed.oh_playlist_event_sub_url =
                                                    current_event_sub_url.clone();
                                            }
                                            debug!(
                                                "Found OpenHome Playlist for {}: type={} controlURL={}",
                                                udn, st, ctrl
                                            );
                                        }
                                    }

                                    if lower.contains("urn:av-openhome-org:service:info:") {
                                        if parsed.oh_info_service_type.is_none() {
                                            parsed.oh_info_service_type = Some(st.clone());
                                            parsed.oh_info_control_url = Some(ctrl.clone());
                                            if parsed.oh_info_event_sub_url.is_none() {
                                                parsed.oh_info_event_sub_url =
                                                    current_event_sub_url.clone();
                                            }
                                            debug!(
                                                "Found OpenHome Info for {}: type={} controlURL={}",
                                                udn, st, ctrl
                                            );
                                        }
                                    }

                                    if lower.contains("urn:av-openhome-org:service:time:") {
                                        if parsed.oh_time_service_type.is_none() {
                                            parsed.oh_time_service_type = Some(st.clone());
                                            parsed.oh_time_control_url = Some(ctrl.clone());
                                            if parsed.oh_time_event_sub_url.is_none() {
                                                parsed.oh_time_event_sub_url =
                                                    current_event_sub_url.clone();
                                            }
                                            debug!(
                                                "Found OpenHome Time for {}: type={} controlURL={}",
                                                udn, st, ctrl
                                            );
                                        }
                                    }

                                    if lower.contains("urn:av-openhome-org:service:volume:") {
                                        if parsed.oh_volume_service_type.is_none() {
                                            parsed.oh_volume_service_type = Some(st.clone());
                                            parsed.oh_volume_control_url = Some(ctrl.clone());
                                            debug!(
                                                "Found OpenHome Volume for {}: type={} controlURL={}",
                                                udn, st, ctrl
                                            );
                                        }
                                    }

                                    if lower.contains("urn:av-openhome-org:service:radio:") {
                                        if parsed.oh_radio_service_type.is_none() {
                                            parsed.oh_radio_service_type = Some(st.clone());
                                            parsed.oh_radio_control_url = Some(ctrl.clone());
                                            debug!(
                                                "Found OpenHome Radio for {}: type={} controlURL={}",
                                                udn, st, ctrl
                                            );
                                        }
                                    }

                                    if lower.contains("urn:av-openhome-org:service:product:") {
                                        if parsed.oh_product_service_type.is_none() {
                                            parsed.oh_product_service_type = Some(st.clone());
                                            parsed.oh_product_control_url = Some(ctrl.clone());
                                            debug!(
                                                "Found OpenHome Product for {}: type={} controlURL={}",
                                                udn, st, ctrl
                                            );
                                        }
                                    }
                                }

                                in_service = false;
                                current_service_type = None;
                                current_control_url = None;
                                current_event_sub_url = None;
                            }
                        }
                        _ => {}
                    }
                    current_tag = None;
                }
                Event::Text(e) => {
                    if in_device {
                        if let Some(tag) = &current_tag {
                            // quick-xml ≥ 0.37 : unescape() → decode()
                            let text = e.decode().map_err(XmlError::Encoding)?.into_owned();

                            match tag.as_str() {
                                "UDN" => {
                                    parsed.udn = text;
                                }
                                "deviceType" => {
                                    parsed.device_type = Some(text);
                                }
                                "friendlyName" => {
                                    parsed.friendly_name = Some(text);
                                }
                                "manufacturer" => {
                                    parsed.manufacturer = Some(text);
                                }
                                "modelName" => {
                                    parsed.model_name = Some(text);
                                }
                                "serviceType" if in_service => {
                                    parsed.service_types.push(text.clone());
                                    current_service_type = Some(text);
                                }
                                "controlURL" if in_service => {
                                    current_control_url = Some(text);
                                }
                                "eventSubURL" if in_service => {
                                    current_event_sub_url = Some(text);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Event::Eof => break,
                _ => {}
            }

            buf.clear();
        }

        parsed.require_fields()
    }

    pub fn build_renderer(
        &self,
    ) -> Option<RendererInfo> {
        let device_type = self.device_type.as_ref()?.to_ascii_lowercase();
        if !device_type.contains("urn:schemas-upnp-org:device:mediarenderer:")
            && !device_type.contains("urn:av-openhome-org:device:mediarenderer:")
            && !device_type.contains("urn:av-openhome-org:device:source:")
        {
            debug!(
                "build_renderer: ignoring deviceType for {}: {}",
                self.udn, device_type
            );
            return None;
        }

        let udn = self.udn.to_ascii_lowercase();
        let mut caps = detect_renderer_capabilities(&self.service_types);
        if detect_linkplay_http(&self.location, Duration::from_secs(self.timeout_secs.max(1))) {
            caps.has_linkplay_http = true;
        }
        if detect_arylic_tcp(&self.location, Duration::from_secs(self.timeout_secs.max(1))) {
            caps.has_arylic_tcp = true;
        }
        let protocol = detect_renderer_protocol(&caps);

        Some(RendererInfo::make(
            DeviceId(udn.clone()),
            udn,
            self.friendly_name.clone().unwrap_or_default(),
            self.model_name.clone().unwrap_or_default(),
            self.manufacturer.clone().unwrap_or_default(),
            protocol,
            caps,
            self.location.clone(),
            self.server_header.clone(),
            self.avtransport_service_type.clone(),
            self
                .avtransport_control_url
                .as_ref()
                .map(|ctrl| resolve_control_url(&self.location, ctrl)),
            self.rendering_control_service_type.clone(),
            self
                .rendering_control_control_url
                .as_ref()
                .map(|ctrl| resolve_control_url(&self.location, ctrl)),
            self.connection_manager_service_type.clone(),
            self
                .connection_manager_control_url
                .as_ref()
                .map(|ctrl| resolve_control_url(&self.location, ctrl)),
            self.oh_playlist_service_type.clone(),
            self
                .oh_playlist_control_url
                .as_ref()
                .map(|ctrl| resolve_control_url(&self.location, ctrl)),
            self
                .oh_playlist_event_sub_url
                .as_ref()
                .map(|url| resolve_control_url(&self.location, url)),
            self.oh_info_service_type.clone(),
            self
                .oh_info_control_url
                .as_ref()
                .map(|ctrl| resolve_control_url(&self.location, ctrl)),
            self
                .oh_info_event_sub_url
                .as_ref()
                .map(|url| resolve_control_url(&self.location, url)),
            self.oh_time_service_type.clone(),
            self
                .oh_time_control_url
                .as_ref()
                .map(|ctrl| resolve_control_url(&self.location, ctrl)),
            self
                .oh_time_event_sub_url
                .as_ref()
                .map(|url| resolve_control_url(&self.location, url)),
            self.oh_volume_service_type.clone(),
            self
                .oh_volume_control_url
                .as_ref()
                .map(|ctrl| resolve_control_url(&self.location, ctrl)),
            self.oh_radio_service_type.clone(),
            self
                .oh_radio_control_url
                .as_ref()
                .map(|ctrl| resolve_control_url(&self.location, ctrl)),
            self.oh_product_service_type.clone(),
            self
                .oh_product_control_url
                .as_ref()
                .map(|ctrl| resolve_control_url(&self.location, ctrl)),
        ))
    }

    pub fn build_server(
        &self,
    ) -> Option<UpnpMediaServer> {
        let device_type = self.device_type.as_ref()?.to_ascii_lowercase();
        if !device_type.contains("urn:schemas-upnp-org:device:mediaserver:") {
            return None;
        }

        let udn = self.udn.to_ascii_lowercase();
        let has_content_directory = self.service_types.iter().any(|st| {
            st.to_ascii_lowercase()
                .contains("urn:schemas-upnp-org:service:contentdirectory:")
        });

        let content_directory_control_url = self
            .content_directory_control_url
            .as_ref()
            .map(|ctrl| resolve_control_url(&self.location, ctrl));

        Some(UpnpMediaServer::new(
            DeviceId(udn.clone()),
            udn,
            self.friendly_name.clone().unwrap_or_default(),
            self.model_name.clone().unwrap_or_default(),
            self.manufacturer.clone().unwrap_or_default(),
            self.location.clone(),
            self.server_header.clone(),
            has_content_directory,
            self.content_directory_service_type.clone(),
            content_directory_control_url,
        ))

    }

    /// Returns Ok(Some(client)) if an AVTransport service with a controlURL is present,
    /// Ok(None) if no AVTransport service was found.
    pub fn build_avtransport_client(
        &self,
    ) -> Result<Option<AvTransportClient>, DescriptionError> {
        let service_type = match &self.avtransport_service_type {
            Some(st) => st.clone(),
            None => return Ok(None),
        };

        let raw_control = match &self.avtransport_control_url {
            Some(ctrl) => ctrl.clone(),
            None => return Ok(None),
        };

        let control_url = resolve_control_url(&self.location, &raw_control);
        debug!(
            "AVTransport client for {}: service_type={} control_url={}",
            &self.udn, service_type, control_url
        );

        Ok(Some(AvTransportClient::new(control_url, service_type)))
    }

    fn require_fields(self) -> Result<Self, DescriptionError> {
        if self.device_type.is_none() {
            return Err(DescriptionError::MissingField("deviceType"));
        }
        if self.friendly_name.is_none() {
            return Err(DescriptionError::MissingField("friendlyName"));
        }
        if self.model_name.is_none() {
            return Err(DescriptionError::MissingField("modelName"));
        }
        Ok(self)
    }

    pub fn udn(&self) -> Option<String> {
        Some(self.udn.clone())
    }
    pub fn device_type(&self) -> Option<String> {
        self.device_type.clone()
    }
    pub fn friendly_name(&self) -> Option<String> {
        self.friendly_name.clone()
    }
    pub fn manufacturer(&self) -> Option<String> {
        self.manufacturer.clone()
    }
    pub fn model_name(&self) -> Option<String> {
        self.model_name.clone()
    }
    pub fn service_types(&self) -> Vec<String> {
        self.service_types.clone()
    }

    // New: AVTransport endpoint (if present in serviceList)
    pub fn avtransport_service_type(&self) -> Option<String> {
        self.avtransport_service_type.clone()
    }
    pub fn avtransport_control_url(&self) -> Option<String> {
        self.avtransport_control_url.clone()
    }

    // RenderingControl endpoint (if present in serviceList)
    pub fn rendering_control_service_type(&self) -> Option<String> {
        self.rendering_control_service_type.clone()
    }
    pub fn rendering_control_control_url(&self) -> Option<String> {
        self.rendering_control_control_url.clone()
    }

    // ConnectionManager endpoint (if present in serviceList)
    pub fn connection_manager_service_type(&self) -> Option<String> {
        self.connection_manager_service_type.clone()
    }
    pub fn connection_manager_control_url(&self) -> Option<String> {
        self.connection_manager_control_url.clone()
    }

    // ContentDirectory endpoint (if present in serviceList)
    pub fn content_directory_service_type(&self) -> Option<String> {
        self.content_directory_service_type.clone()
    }
    pub fn content_directory_control_url(&self) -> Option<String> {
        self.content_directory_control_url.clone()
    }

    // OpenHome endpoints (if present in serviceList)
    pub fn oh_playlist_service_type(&self) -> Option<String> {
        self.oh_playlist_service_type.clone()
    }
    pub fn oh_playlist_control_url(&self) -> Option<String> {
        self.oh_playlist_control_url.clone()
    }
    pub fn oh_playlist_event_sub_url(&self) -> Option<String> {
        self.oh_playlist_event_sub_url.clone()
    }
    pub fn oh_info_service_type(&self) -> Option<String> {
        self.oh_info_service_type.clone()
    }
    pub fn oh_info_control_url(&self) -> Option<String> {
        self.oh_info_control_url.clone()
    }
    pub fn oh_info_event_sub_url(&self) -> Option<String> {
        self.oh_info_event_sub_url.clone()
    }
    pub fn oh_time_service_type(&self) -> Option<String> {
        self.oh_time_service_type.clone()
    }
    pub fn oh_time_control_url(&self) -> Option<String> {
        self.oh_time_control_url.clone()
    }
    pub fn oh_time_event_sub_url(&self) -> Option<String> {
        self.oh_time_event_sub_url.clone()
    }
    pub fn oh_volume_service_type(&self) -> Option<String> {
        self.oh_volume_service_type.clone()
    }
    pub fn oh_volume_control_url(&self) -> Option<String> {
        self.oh_volume_control_url.clone()
    }
    pub fn oh_radio_service_type(&self) -> Option<String> {
        self.oh_radio_service_type.clone()
    }
    pub fn oh_radio_control_url(&self) -> Option<String> {
        self.oh_radio_control_url.clone()
    }
    pub fn oh_product_service_type(&self) -> Option<String> {
        self.oh_product_service_type.clone()
    }
    pub fn oh_product_control_url(&self) -> Option<String> {
        self.oh_product_control_url.clone()
    }
}

/// HTTP-based XML description provider (UPnP device description.xml)
pub struct HttpXmlDescriptionProvider {
    timeout_secs: u64,
}

// --- capabilities detection unchanged ---

fn detect_renderer_capabilities(service_types: &[String]) -> RendererCapabilities {
    let mut caps = RendererCapabilities::default();

    for st in service_types {
        let lower = st.to_ascii_lowercase();

        if lower.contains("urn:schemas-upnp-org:service:avtransport:") {
            caps.has_avtransport = true;
        }
        if lower.contains("urn:schemas-upnp-org:service:renderingcontrol:") {
            caps.has_rendering_control = true;
        }
        if lower.contains("urn:schemas-upnp-org:service:connectionmanager:") {
            caps.has_connection_manager = true;
        }
        if lower.contains("urn:av-openhome-org:service:playlist:") {
            caps.has_oh_playlist = true;
        }
        if lower.contains("urn:av-openhome-org:service:volume:") {
            caps.has_oh_volume = true;
        }
        if lower.contains("urn:av-openhome-org:service:info:") {
            caps.has_oh_info = true;
        }
        if lower.contains("urn:av-openhome-org:service:time:") {
            caps.has_oh_time = true;
        }
        if lower.contains("urn:av-openhome-org:service:radio:") {
            caps.has_oh_radio = true;
        }
    }

    caps
}

fn detect_renderer_protocol(caps: &RendererCapabilities) -> RendererProtocol {
    let has_upnp_av =
        caps.has_avtransport || caps.has_rendering_control || caps.has_connection_manager;
    let has_openhome = caps.has_oh_playlist
        || caps.has_oh_volume
        || caps.has_oh_info
        || caps.has_oh_time
        || caps.has_oh_radio;

    match (has_upnp_av, has_openhome) {
        (true, true) => RendererProtocol::OpenHomeHybrid,
        (true, false) => RendererProtocol::UpnpAvOnly,
        (false, true) => RendererProtocol::OpenHomeOnly,
        (false, false) => RendererProtocol::UpnpAvOnly,
    }
}




/// Detect whether a renderer exposes the LinkPlay HTTP API.
pub fn detect_linkplay_http(location: &str, timeout: Duration) -> bool {
    let Some(host) = extract_linkplay_host(location) else {
        return false;
    };

    match fetch_status_for_host(&host, timeout) {
        Ok(_) => true,
        Err(err) => {
            debug!(
                "LinkPlay detection failed for {} (host={}): {}",
                location, host, err
            );
            false
        }
    }
}