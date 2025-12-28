use std::io::BufReader;
use std::time::{Duration, Instant, SystemTime};

use quick_xml::{Error as XmlError, Reader, events::Event};
use thiserror::Error;
use tracing::{debug, warn};

use crate::DeviceId;
use crate::arylic_tcp::detect_arylic_tcp;
use crate::avtransport_client::AvTransportClient;
use crate::discovery::upnp_discovery::DeviceDescriptionProvider;
use crate::linkplay_renderer::detect_linkplay_http;
use crate::media_server::UpnpMediaServer;
use crate::model::{RendererCapabilities, RendererInfo, RendererProtocol};

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
struct ParsedDeviceDescription {
    udn: Option<String>,
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
}

/// HTTP-based XML description provider (UPnP device description.xml)
pub struct HttpXmlDescriptionProvider {
    timeout_secs: u64,
}

impl HttpXmlDescriptionProvider {
    pub fn new(timeout_secs: u64) -> Self {
        Self { timeout_secs }
    }

    /// Fetch and parse the device description.xml at endpoint.location.
    fn fetch_and_parse(
        &self,
        udn: &str,
        location: &str,
        server_header: &str,
    ) -> Result<ParsedDeviceDescription, DescriptionError> {
        debug!("Fetching description for {} at {}", udn, location);

        let config = Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(self.timeout_secs)))
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
                                    parsed.udn = Some(text);
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

    fn build_renderer(
        &self,
        udn: &str,
        location: &str,
        server_header: &str,
        parsed: &ParsedDeviceDescription,
    ) -> Option<RendererInfo> {
        let device_type = parsed.device_type.as_ref()?.to_ascii_lowercase();
        if !device_type.contains("urn:schemas-upnp-org:device:mediarenderer:")
            && !device_type.contains("urn:av-openhome-org:device:mediarenderer:")
            && !device_type.contains("urn:av-openhome-org:device:source:")
        {
            debug!(
                "build_renderer: ignoring deviceType for {}: {}",
                udn, device_type
            );
            return None;
        }

        let raw_udn = parsed.udn.as_deref().unwrap_or_else(|| udn);
        let udn = raw_udn.to_ascii_lowercase();
        let mut caps = detect_renderer_capabilities(&parsed.service_types);
        if detect_linkplay_http(location, Duration::from_secs(self.timeout_secs.max(1))) {
            caps.has_linkplay_http = true;
        }
        if detect_arylic_tcp(location, Duration::from_secs(self.timeout_secs.max(1))) {
            caps.has_arylic_tcp = true;
        }
        let protocol = detect_renderer_protocol(&caps);
        let now = Instant::now();

        Some(RendererInfo::make(
            DeviceId(udn.clone()),
            udn,
            parsed.friendly_name.clone().unwrap_or_default(),
            parsed.model_name.clone().unwrap_or_default(),
            parsed.manufacturer.clone().unwrap_or_default(),
            protocol,
            caps,
            location.to_string(),
            endpoint.server_header.clone(),
            // online: true,
            // last_seen: now,
            // max_age: endpoint.max_age,
            parsed.avtransport_service_type.clone(),
            parsed
                .avtransport_control_url
                .as_ref()
                .map(|ctrl| resolve_control_url(location, ctrl)),
            parsed.rendering_control_service_type.clone(),
            parsed
                .rendering_control_control_url
                .as_ref()
                .map(|ctrl| resolve_control_url(location, ctrl)),
            parsed.connection_manager_service_type.clone(),
            parsed
                .connection_manager_control_url
                .as_ref()
                .map(|ctrl| resolve_control_url(location, ctrl)),
            parsed.oh_playlist_service_type.clone(),
            parsed
                .oh_playlist_control_url
                .as_ref()
                .map(|ctrl| resolve_control_url(location, ctrl)),
            parsed
                .oh_playlist_event_sub_url
                .as_ref()
                .map(|url| resolve_control_url(location, url)),
            parsed.oh_info_service_type.clone(),
            parsed
                .oh_info_control_url
                .as_ref()
                .map(|ctrl| resolve_control_url(location, ctrl)),
            parsed
                .oh_info_event_sub_url
                .as_ref()
                .map(|url| resolve_control_url(location, url)),
            parsed.oh_time_service_type.clone(),
            parsed
                .oh_time_control_url
                .as_ref()
                .map(|ctrl| resolve_control_url(location, ctrl)),
            parsed
                .oh_time_event_sub_url
                .as_ref()
                .map(|url| resolve_control_url(location, url)),
            parsed.oh_volume_service_type.clone(),
            parsed
                .oh_volume_control_url
                .as_ref()
                .map(|ctrl| resolve_control_url(location, ctrl)),
            parsed.oh_radio_service_type.clone(),
            parsed
                .oh_radio_control_url
                .as_ref()
                .map(|ctrl| resolve_control_url(location, ctrl)),
            parsed.oh_product_service_type.clone(),
            parsed
                .oh_product_control_url
                .as_ref()
                .map(|ctrl| resolve_control_url(location, ctrl)),
        ))
    }

    fn build_server(
        &self,
        endpoint: &DiscoveredEndpoint,
        parsed: &ParsedDeviceDescription,
    ) -> Option<UpnpMediaServer> {
        let device_type = parsed.device_type.as_ref()?.to_ascii_lowercase();
        if !device_type.contains("urn:schemas-upnp-org:device:mediaserver:") {
            return None;
        }

        let raw_udn = parsed
            .udn
            .as_deref()
            .unwrap_or_else(|| endpoint.udn.as_str());
        let udn = raw_udn.to_ascii_lowercase();
        let has_content_directory = parsed.service_types.iter().any(|st| {
            st.to_ascii_lowercase()
                .contains("urn:schemas-upnp-org:service:contentdirectory:")
        });
        let now = Instant::now();

        let content_directory_control_url = parsed
            .content_directory_control_url
            .as_ref()
            .map(|ctrl| resolve_control_url(&endpoint.location, ctrl));

        Some(UpnpMediaServer {
            id: DeviceId(udn.clone()),
            udn,
            friendly_name: parsed.friendly_name.clone().unwrap_or_default(),
            model_name: parsed.model_name.clone().unwrap_or_default(),
            manufacturer: parsed.manufacturer.clone().unwrap_or_default(),
            location: endpoint.location.clone(),
            server_header: endpoint.server_header.clone(),
            online: true,
            last_seen: now,
            max_age: endpoint.max_age,
            has_content_directory,
            content_directory_service_type: parsed.content_directory_service_type.clone(),
            content_directory_control_url,
        })
    }

    /// New helper: build an AvTransportClient directly from a discovered endpoint.
    ///
    /// Returns Ok(Some(client)) if an AVTransport service with a controlURL is present,
    /// Ok(None) if no AVTransport service was found.
    pub fn build_avtransport_client(
        &self,
        endpoint: &DiscoveredEndpoint,
    ) -> Result<Option<AvTransportClient>, DescriptionError> {
        let parsed = self.fetch_and_parse(endpoint)?;

        let service_type = match &parsed.avtransport_service_type {
            Some(st) => st.clone(),
            None => return Ok(None),
        };

        let raw_control = match &parsed.avtransport_control_url {
            Some(ctrl) => ctrl.clone(),
            None => return Ok(None),
        };

        let control_url = resolve_control_url(&endpoint.location, &raw_control);
        debug!(
            "AVTransport client for {}: service_type={} control_url={}",
            endpoint.udn, service_type, control_url
        );

        Ok(Some(AvTransportClient::new(control_url, service_type)))
    }
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

/// Resolve a possibly relative controlURL against the description URL.
///
/// - If `control_url` is already absolute (starts with http:// or https://), it is returned as-is.
/// - Otherwise, it is resolved against the scheme://host:port of `description_url`.
pub(crate) fn resolve_control_url(description_url: &str, control_url: &str) -> String {
    if control_url.starts_with("http://") || control_url.starts_with("https://") {
        return control_url.to_string();
    }

    // Extract "scheme://host[:port]" from description_url
    if let Some((scheme, rest)) = description_url.split_once("://") {
        if let Some(pos) = rest.find('/') {
            let authority = &rest[..pos];
            let base = format!("{}://{}", scheme, authority);

            if control_url.starts_with('/') {
                return format!("{}{}", base, control_url);
            } else {
                return format!("{}/{}", base, control_url);
            }
        }
    }

    // Fallback: just return the raw control_url if we cannot parse
    control_url.to_string()
}

#[cfg(test)]
mod tests {
    use super::resolve_control_url;

    #[test]
    fn resolves_relative_path_against_description() {
        let base = "http://192.0.2.10:49152/device.xml";
        let control = "/upnp/control/playlist";
        let resolved = resolve_control_url(base, control);
        assert_eq!(resolved, "http://192.0.2.10:49152/upnp/control/playlist");
    }

    #[test]
    fn leaves_absolute_url_untouched() {
        let url = "http://renderer.local:1400/MediaRenderer/Control";
        let resolved = resolve_control_url("http://example.invalid/device.xml", url);
        assert_eq!(resolved, url);
    }
}

impl DeviceDescriptionProvider for HttpXmlDescriptionProvider {
    fn build_renderer_info(&self, endpoint: &DiscoveredEndpoint) -> Option<RendererInfo> {
        match self.fetch_and_parse(endpoint) {
            Ok(parsed) => {
                let device_type = parsed.device_type.as_deref().unwrap_or("unknown");
                debug!(
                    "Renderer description OK for {} at {} (deviceType={})",
                    endpoint.udn, endpoint.location, device_type
                );
                self.build_renderer(endpoint, &parsed)
            }
            Err(err) => {
                warn!(
                    "Failed to fetch/parse renderer description for {} at {}: {}",
                    endpoint.udn, endpoint.location, err
                );
                None
            }
        }
    }

    fn build_server_info(&self, endpoint: &DiscoveredEndpoint) -> Option<UpnpMediaServer> {
        match self.fetch_and_parse(endpoint) {
            Ok(parsed) => {
                let device_type = parsed.device_type.as_deref().unwrap_or("unknown");
                debug!(
                    "Server description OK for {} at {} (deviceType={})",
                    endpoint.udn, endpoint.location, device_type
                );
                self.build_server(endpoint, &parsed)
            }
            Err(err) => {
                warn!(
                    "Failed to fetch/parse server description for {} at {}: {}",
                    endpoint.udn, endpoint.location, err
                );
                None
            }
        }
    }
}
