use std::collections::HashMap;
use std::time::SystemTime;

use crate::avtransport_client::AvTransportClient;
use crate::connection_manager_client::ConnectionManagerClient;
use crate::media_server::{MediaServerInfo, ServerId};
use crate::model::{RendererId, RendererInfo};
use crate::rendering_control_client::RenderingControlClient;
use tracing::debug;

#[derive(Clone, Debug)]
enum DeviceKey {
    Renderer(RendererId),
    Server(ServerId),
}

#[derive(Debug, Default)]
pub struct DeviceRegistry {
    renderers: HashMap<RendererId, RendererInfo>,
    servers: HashMap<ServerId, MediaServerInfo>,
    udn_index: HashMap<String, DeviceKey>,
}

/// Read-only view / trait for registry access.
///
/// Pour l’instant, on ne rajoute pas AVTransport ici, on se contente
/// d’ajouter les helpers dans `impl DeviceRegistry`.
pub trait DeviceRegistryRead {
    fn list_renderers(&self) -> Vec<RendererInfo>;
    fn list_servers(&self) -> Vec<MediaServerInfo>;

    fn get_renderer(&self, id: &RendererId) -> Option<RendererInfo>;
    fn get_server(&self, id: &ServerId) -> Option<MediaServerInfo>;
}

impl DeviceRegistryRead for DeviceRegistry {
    fn list_renderers(&self) -> Vec<RendererInfo> {
        self.renderers.values().cloned().collect()
    }

    fn list_servers(&self) -> Vec<MediaServerInfo> {
        self.servers.values().cloned().collect()
    }

    fn get_renderer(&self, id: &RendererId) -> Option<RendererInfo> {
        self.renderers.get(id).cloned()
    }

    fn get_server(&self, id: &ServerId) -> Option<MediaServerInfo> {
        self.servers.get(id).cloned()
    }
}

#[derive(Debug)]
pub enum DeviceUpdate {
    RendererOnline(RendererInfo),
    RendererOfflineById(RendererId),
    RendererOfflineByUdn(String),

    ServerOnline(MediaServerInfo),
    ServerOfflineById(ServerId),
    ServerOfflineByUdn(String),
}

impl DeviceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply_update(&mut self, update: DeviceUpdate) {
        match update {
            DeviceUpdate::RendererOnline(info) => {
                let udn = info.udn.to_ascii_lowercase();
                let id = info.id.clone();
                let mut info = info;

                info.online = true;
                info.last_seen = SystemTime::now();

                self.renderers.insert(id.clone(), info);
                self.udn_index.insert(udn, DeviceKey::Renderer(id));
            }
            DeviceUpdate::RendererOfflineById(id) => {
                if let Some(info) = self.renderers.get_mut(&id) {
                    info.online = false;
                    info.last_seen = SystemTime::now();
                }
            }
            DeviceUpdate::RendererOfflineByUdn(udn) => {
                let lookup = udn.to_ascii_lowercase();
                if let Some(DeviceKey::Renderer(id)) = self.udn_index.get(&lookup) {
                    if let Some(info) = self.renderers.get_mut(id) {
                        info.online = false;
                        info.last_seen = SystemTime::now();
                    }
                }
            }
            DeviceUpdate::ServerOnline(info) => {
                let udn = info.udn.to_ascii_lowercase();
                let id = info.id.clone();
                let mut info = info;

                info.online = true;
                info.last_seen = SystemTime::now();

                self.servers.insert(id.clone(), info);
                self.udn_index.insert(udn, DeviceKey::Server(id));
            }
            DeviceUpdate::ServerOfflineById(id) => {
                if let Some(info) = self.servers.get_mut(&id) {
                    info.online = false;
                    info.last_seen = SystemTime::now();
                }
            }
            DeviceUpdate::ServerOfflineByUdn(udn) => {
                let lookup = udn.to_ascii_lowercase();
                if let Some(DeviceKey::Server(id)) = self.udn_index.get(&lookup) {
                    if let Some(info) = self.servers.get_mut(id) {
                        info.online = false;
                        info.last_seen = SystemTime::now();
                    }
                }
            }
        }
    }

    /// Helper: get a renderer by UDN (case-insensitive, via udn_index).
    pub fn get_renderer_by_udn(&self, udn: &str) -> Option<RendererInfo> {
        let lookup = udn.to_ascii_lowercase();
        match self.udn_index.get(&lookup) {
            Some(DeviceKey::Renderer(id)) => self.renderers.get(id).cloned(),
            _ => None,
        }
    }

    /// Helper: get a server by UDN (case-insensitive, via udn_index).
    pub fn get_server_by_udn(&self, udn: &str) -> Option<MediaServerInfo> {
        let lookup = udn.to_ascii_lowercase();
        match self.udn_index.get(&lookup) {
            Some(DeviceKey::Server(id)) => self.servers.get(id).cloned(),
            _ => None,
        }
    }

    /// Construct an AvTransportClient for a given renderer id, if possible.
    ///
    /// Returns:
    /// - Some(client) if the renderer exists AND has avtransport_* fields set
    /// - None if renderer not found or no AVTransport service.
    pub fn avtransport_client_for_renderer(&self, id: &RendererId) -> Option<AvTransportClient> {
        let info = self.renderers.get(id)?;

        let service_type = info.avtransport_service_type.as_ref()?;
        let control_url = info.avtransport_control_url.as_ref()?;

        Some(AvTransportClient::new(
            control_url.clone(),
            service_type.clone(),
        ))
    }

    /// Construct an AvTransportClient for a given UDN, if possible.
    pub fn avtransport_client_for_udn(&self, udn: &str) -> Option<AvTransportClient> {
        let info = self.get_renderer_by_udn(udn)?;

        let service_type = info.avtransport_service_type?;
        let control_url = info.avtransport_control_url?;

        Some(AvTransportClient::new(control_url, service_type))
    }

    /// Construct a RenderingControlClient for a given renderer id, if possible.
    pub fn rendering_control_client_for_renderer(
        &self,
        id: &RendererId,
    ) -> Option<RenderingControlClient> {
        let info = self.renderers.get(id)?;

        let service_type = info.rendering_control_service_type.as_ref()?;
        let control_url = info.rendering_control_control_url.as_ref()?;

        Some(RenderingControlClient::new(
            control_url.clone(),
            service_type.clone(),
        ))
    }

    /// Construct a ConnectionManagerClient for a given renderer id, if possible.
    pub fn connection_manager_client_for_renderer(
        &self,
        id: &RendererId,
    ) -> Option<ConnectionManagerClient> {
        let info = self.renderers.get(id)?;

        let service_type = info.connection_manager_service_type.as_ref()?;
        let control_url = info.connection_manager_control_url.as_ref()?;

        Some(ConnectionManagerClient::new(
            control_url.clone(),
            service_type.clone(),
        ))
    }

    pub fn mark_renderer_supports_set_next(&mut self, id: &RendererId) {
        if let Some(info) = self.renderers.get_mut(id) {
            if !info.capabilities.has_avtransport_set_next {
                info.capabilities.has_avtransport_set_next = true;
                debug!(
                    renderer = id.0.as_str(),
                    "Renderer now marked as supporting AVTransport.SetNextAVTransportURI"
                );
            }
        }
    }
}
