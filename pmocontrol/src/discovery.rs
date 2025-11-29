use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

use pmoupnp::ssdp::SsdpEvent;

use crate::model::{MediaServerInfo, RendererInfo};
use crate::registry::DeviceUpdate;

/// État connu pour un endpoint UPnP identifié par son UDN.
#[derive(Clone, Debug)]
pub struct DiscoveredEndpoint {
    /// UDN normalisé (ex: "uuid:xxxx", en minuscules).
    pub udn: String,
    /// Dernière URL de description device (LOCATION SSDP).
    pub location: String,
    /// Dernier header SERVER vu sur cet endpoint.
    pub server_header: String,
    /// Dernier max-age indiqué (TTL SSDP).
    pub max_age: u32,
    /// Date de dernière vue (Now lors du dernier Alive ou SearchResponse).
    pub last_seen: SystemTime,
    /// Indique si on a vu ce endpoint comme MediaRenderer.
    pub seen_as_renderer: bool,
    /// Indique si on a vu ce endpoint comme MediaServer.
    pub seen_as_server: bool,
    /// ST/NT vus (pour debug/diagnostic si utile).
    pub types_seen: HashSet<String>,
}

impl DiscoveredEndpoint {
    pub fn new(udn: String, location: String, server_header: String, max_age: u32) -> Self {
        Self {
            udn,
            location,
            server_header,
            max_age,
            last_seen: SystemTime::now(),
            seen_as_renderer: false,
            seen_as_server: false,
            types_seen: HashSet::new(),
        }
    }

    pub fn touch(&mut self, location: String, server_header: String, max_age: u32) {
        self.location = location;
        self.server_header = server_header;
        self.max_age = max_age;
        self.last_seen = SystemTime::now();
    }
}

/// Fournit les descriptions haut niveau à partir d’un endpoint découvert.
/// L’implémentation pourra, plus tard, faire un HTTP GET sur `location`
/// et parser la description pour remplir RendererInfo / MediaServerInfo.
pub trait DeviceDescriptionProvider: Send + Sync {
    /// Construit un RendererInfo pour cet endpoint, ou None s’il
    /// ne correspond pas à un renderer audio intéressant.
    fn build_renderer_info(&self, endpoint: &DiscoveredEndpoint) -> Option<RendererInfo>;

    /// Construit un MediaServerInfo pour cet endpoint, ou None s’il
    /// ne correspond pas à un media server (ou pas intéressant).
    fn build_server_info(&self, endpoint: &DiscoveredEndpoint) -> Option<MediaServerInfo>;
}

/// Gestionnaire des événements SSDP -> DeviceUpdate.
pub struct DiscoveryManager<P>
where
    P: DeviceDescriptionProvider,
{
    endpoints: HashMap<String, DiscoveredEndpoint>,
    provider: P,
}

impl<P> DiscoveryManager<P>
where
    P: DeviceDescriptionProvider,
{
    pub fn new(provider: P) -> Self {
        Self {
            endpoints: HashMap::new(),
            provider,
        }
    }

    pub fn handle_ssdp_event(&mut self, event: SsdpEvent) -> Vec<DeviceUpdate> {
        let mut updates = Vec::new();

        match event {
            SsdpEvent::Alive {
                usn,
                nt,
                location,
                server,
                max_age,
                ..
            } => {
                if let Some(udn) = extract_udn_from_usn(&usn) {
                    self.handle_alive(udn, nt, location, server, max_age, &mut updates);
                }
            }
            SsdpEvent::SearchResponse {
                usn,
                st,
                location,
                server,
                max_age,
                ..
            } => {
                if let Some(udn) = extract_udn_from_usn(&usn) {
                    self.handle_search_response(udn, st, location, server, max_age, &mut updates);
                }
            }
            SsdpEvent::ByeBye { usn, nt, .. } => {
                if let Some(udn) = extract_udn_from_usn(&usn) {
                    self.handle_byebye(udn, nt, &mut updates);
                }
            }
        }

        updates
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
        let endpoint = self
            .endpoints
            .entry(udn.clone())
            .or_insert_with({
                let udn = udn.clone();
                let location = location.clone();
                let server_header = server_header.clone();
                move || DiscoveredEndpoint::new(udn, location, server_header, max_age)
            });

        endpoint.touch(location, server_header, max_age);
        endpoint.types_seen.insert(device_type.clone());

        if is_renderer_type(&device_type) {
            endpoint.seen_as_renderer = true;
            if let Some(info) = self.provider.build_renderer_info(endpoint) {
                updates.push(DeviceUpdate::RendererOnline(info));
            }
        }

        if is_server_type(&device_type) {
            endpoint.seen_as_server = true;
            if let Some(info) = self.provider.build_server_info(endpoint) {
                updates.push(DeviceUpdate::ServerOnline(info));
            }
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

fn is_renderer_type(t: &str) -> bool {
    let t = t.to_ascii_lowercase();
    t.contains("urn:schemas-upnp-org:device:mediarenderer:")
}

fn is_server_type(t: &str) -> bool {
    let t = t.to_ascii_lowercase();
    t.contains("urn:schemas-upnp-org:device:mediaserver:")
}
