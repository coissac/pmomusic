use std::collections::{HashMap, HashSet};

use pmoupnp::ssdp::SsdpEvent;

use crate::discovery::manager::UDNRegistry;
use crate::media_server::UpnpMediaServer;
use crate::model::RendererInfo;
use crate::registry::DeviceUpdate;

/// Fournit les descriptions haut niveau à partir d’un endpoint découvert.
/// L’implémentation pourra, plus tard, faire un HTTP GET sur `location`
/// et parser la description pour remplir RendererInfo / MediaServerInfo.
pub trait DeviceDescriptionProvider: Send + Sync {
    /// Construit un RendererInfo pour cet endpoint, ou None s’il
    /// ne correspond pas à un renderer audio intéressant.
    fn build_renderer_info(
        &self,
        udn: &str,
        location: &str,
        server_header: &str,
    ) -> Option<RendererInfo>;

    /// Construit un MediaServerInfo pour cet endpoint, ou None s’il
    /// ne correspond pas à un media server (ou pas intéressant).
    fn build_server_info(
        &self,
        udn: &str,
        location: &str,
        server_header: &str,
    ) -> Option<UpnpMediaServer>;
}

/// Gestionnaire des événements SSDP -> DeviceUpdate.
pub struct DiscoveryManager<P>
where
    P: DeviceDescriptionProvider,
{
    provider: P,
    udn_cache: Arc<Mutex<UDNRegistry>>,
    device_registry: Arc<DeviceRegistry>,
}

pub struct UpnpDiscoveryManager {
    device_registry: Arc<Mutex<DeviceRegistry>>,
    udn_cache: Arc<Mutex<UDNRegistry>>,
    http_client: Agent, // ← intégré
}

impl UpnpDiscoveryManager {
    // Dans handle_ssdp_event (upnp_discovery.rs)
    fn handle_ssdp_event(&mut self, event: SsdpEvent) {
        let (alive, usn, location, max_age, server_header) = match event {
            SsdpEvent::Alive {
                usn,
                location,
                max_age,
                server,
                ..
            }
            | SsdpEvent::SearchResponse {
                usn,
                location,
                max_age,
                server,
                ..
            } => (true, usn, location, max_age, server),
            SsdpEvent::ByeBye { usn, .. } => (false, usn, "".to_string(), 0, "".to_string()),
        };

        if let Some(udn) = extract_udn_from_usn(&usn) {
            if alive {
                // ✅ Check cache
                if UDNRegistry::should_fetch(self.udn_cache, &udn, max_age as u64) {
                    // ✅ Fetch + parse
                    let info = self.provider.build_renderer_info(&location)?;
                } 
            } else {
                    self.device_registry.lock()        
                    .expect("UDNRegistry mutex lock failed")
                    .device_says_byebye(&udn);            
                }
        } 
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

}

impl<P> DiscoveryManager<P>
where
    P: DeviceDescriptionProvider,
{
    pub fn new(provider: P) -> Self {
        Self { provider }
    }

    // Dans handle_ssdp_event (upnp_discovery.rs)
    fn handle_ssdp_event(&mut self, event: SsdpEvent) -> Vec<DeviceUpdate> {
        let (alive, usn, location, max_age, server_header) = match event {
            SsdpEvent::Alive {
                usn,
                location,
                max_age,
                server,
                ..
            }
            | SsdpEvent::SearchResponse {
                usn,
                location,
                max_age,
                server,
                ..
            } => (true, usn, location, max_age, server),
            SsdpEvent::ByeBye { usn, .. } => (false, usn, "".to_string(), 0, "".to_string()),
        };

        if let Some(udn) = extract_udn_from_usn(&usn) {
            if alive {
                // ✅ Check cache
                if !UDNRegistry::should_fetch(&udn, max_age) {
                    return vec![]; // Skip, vu récemment
                }

                // ✅ Fetch + parse
                let info = self.provider.build_renderer_info(&location)?;
                vec![DeviceUpdate::RendererOnline(info)]
            } else {
                self.handle_byebye(udn, nt, &mut updates);
            }
        } else {
            return vec![];
        }
    }

    fn handle_alive(
        &mut self,
        udn: String,
        nt: String,
        location: String,
        server_header: String,
        max_age: u32,
        updates: &mut Vec<DeviceUpdate>,
    ) {
        self.update_endpoint(udn, nt, location, server_header, max_age, updates);
    }

    fn handle_search_response(
        &mut self,
        udn: String,
        st: String,
        location: String,
        server_header: String,
        max_age: u32,
        updates: &mut Vec<DeviceUpdate>,
    ) {
        self.update_endpoint(udn, st, location, server_header, max_age, updates);
    }

    fn handle_byebye(&mut self, udn: String, _nt: String, updates: &mut Vec<DeviceUpdate>) {
        if let Some(endpoint) = self.endpoints.get(&udn) {
            if endpoint.seen_as_renderer {
                updates.push(DeviceUpdate::RendererOfflineByUdn(udn.clone()));
            }
            if endpoint.seen_as_server {
                updates.push(DeviceUpdate::ServerOfflineByUdn(udn));
            }
        }
    }

    fn update_endpoint(
        &mut self,
        udn: String,
        device_type: String,
        location: String,
        server_header: String,
        max_age: u32,
        updates: &mut Vec<DeviceUpdate>,
    ) {
        tracing::debug!(
            "SSDP update: udn={} type={} location={} max_age={}",
            udn,
            device_type,
            location,
            max_age
        );

        let endpoint = self.endpoints.entry(udn.clone()).or_insert_with({
            let udn = udn.clone();
            let location = location.clone();
            let server_header = server_header.clone();
            move || DiscoveredEndpoint::new(udn, location, server_header, max_age)
        });

        endpoint.touch(location, server_header, max_age);
        endpoint.types_seen.insert(device_type);

        // Toujours tenter de classifier et envoyer un événement Online pour maintenir
        // l'état à jour dans le registre (important pour les serveurs qui ré-apparaissent)
        if let Some(info) = self.provider.build_renderer_info(endpoint) {
            if !endpoint.seen_as_renderer {
                tracing::debug!(
                    "Renderer classified: udn={} friendly_name={} model={}",
                    info.udn(),
                    info.friendly_name(),
                    info.model_name()
                );
                endpoint.seen_as_renderer = true;
            }
            updates.push(DeviceUpdate::RendererOnline(info));
        }

        if let Some(info) = self.provider.build_server_info(endpoint) {
            if !endpoint.seen_as_server {
                tracing::debug!(
                    "Server classified: udn={} friendly_name={} model={}",
                    info.udn,
                    info.friendly_name,
                    info.model_name
                );
                endpoint.seen_as_server = true;
            }
            updates.push(DeviceUpdate::ServerOnline(info));
        }
    }
}

fn extract_udn_from_usn(usn: &str) -> Option<String> {
    let lower = usn.trim().to_ascii_lowercase();
    if let Some(idx) = lower.find("uuid:") {
        let sub = &lower[idx..];
        if let Some(end) = sub.find("::") {
            Some(sub[..end].to_string())
        } else {
            Some(sub.to_string())
        }
    } else {
        None
    }
}
