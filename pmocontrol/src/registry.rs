use std::collections::HashMap;
use std::time::SystemTime;

use crate::model::{MediaServerId, MediaServerInfo, RendererId, RendererInfo};

#[derive(Clone, Debug)]
enum DeviceKey {
    Renderer(RendererId),
    Server(MediaServerId),
}

#[derive(Debug, Default)]
pub struct DeviceRegistry {
    renderers: HashMap<RendererId, RendererInfo>,
    servers: HashMap<MediaServerId, MediaServerInfo>,
    udn_index: HashMap<String, DeviceKey>,
}

pub trait DeviceRegistryRead {
    fn list_renderers(&self) -> Vec<RendererInfo>;
    fn list_servers(&self) -> Vec<MediaServerInfo>;

    fn get_renderer(&self, id: &RendererId) -> Option<RendererInfo>;
    fn get_server(&self, id: &MediaServerId) -> Option<MediaServerInfo>;
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

    fn get_server(&self, id: &MediaServerId) -> Option<MediaServerInfo> {
        self.servers.get(id).cloned()
    }
}

#[derive(Debug)]
pub enum DeviceUpdate {
    RendererOnline(RendererInfo),
    RendererOfflineById(RendererId),
    RendererOfflineByUdn(String),

    ServerOnline(MediaServerInfo),
    ServerOfflineById(MediaServerId),
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
}
