use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use crate::errors::ControlPointError;
use crate::events::{MediaServerEventBus, RendererEventBus};
use crate::media_server::MusicServer;
use crate::model::RendererInfo;
use crate::music_renderer::MusicRenderer;
use crate::{
    DeviceId, DeviceIdentity, DeviceOnline, MediaServerEvent, RendererEvent, UpnpMediaServer,
};

const DEFAULT_MAX_AGE: u32 = 1800;

#[derive(Debug, Clone)]
pub struct DeviceItem {
    music_renderer: Option<Arc<MusicRenderer>>,
    music_server: Option<Arc<MusicServer>>,
}

pub struct DeviceRegistry {
    devices: HashMap<DeviceId, DeviceItem>,
    udn_index: HashMap<String, DeviceId>,
    renderer_bus: RendererEventBus,
    server_bus: MediaServerEventBus,
}

#[derive(Debug, Clone)]
pub enum DeviceUpdate {
    OfflineById(DeviceId),
    OfflineByUdn(String),
}

impl DeviceItem {
    pub fn as_music_renderer(&self) -> Result<Arc<MusicRenderer>, ControlPointError> {
        self.music_renderer
            .clone() // Clone l'Option<Arc<...>>
            .ok_or_else(|| ControlPointError::IsNotAMediaRender(format!("{:#?}", self)))
    }

    pub fn is_a_music_renderer(&self) -> bool {
        self.music_renderer.is_some()
    }

    pub fn as_music_server(&self) -> Result<Arc<MusicServer>, ControlPointError> {
        self.music_server
            .clone() // Clone l'Option<Arc<...>>
            .ok_or_else(|| ControlPointError::IsNotAMediaRender(format!("{:#?}", self)))
    }

    pub fn is_a_music_server(&self) -> bool {
        self.music_server.is_some()
    }
}

impl DeviceOnline for DeviceItem {
    fn is_online(&self) -> bool {
        self.music_renderer
            .as_ref()
            .map_or(false, |r| r.is_online())
            || self.music_server.as_ref().map_or(false, |s| s.is_online())
    }

    fn last_seen(&self) -> SystemTime {
        let renderer_time = self.music_renderer.as_ref().map(|r| r.last_seen());
        let server_time = self.music_server.as_ref().map(|s| s.last_seen());

        match (renderer_time, server_time) {
            (Some(r), Some(s)) => r.max(s), // Le plus récent
            (Some(r), None) => r,
            (None, Some(s)) => s,
            (None, None) => unreachable!("DeviceEntry must have at least one component"),
        }
    }

    fn has_been_seen_now(&self, max_age: u32) {
        if let Some(r) = &self.music_renderer {
            r.has_been_seen_now(max_age);
        }
        if let Some(s) = &self.music_server {
            s.has_been_seen_now(max_age);
        }
    }

    fn mark_as_offline(&self) {
        if let Some(r) = &self.music_renderer {
            r.mark_as_offline();
        }
        if let Some(s) = &self.music_server {
            s.mark_as_offline();
        }
    }

    fn max_age(&self) -> u32 {
        let renderer_max_age = self.music_renderer.as_ref().map(|r| r.max_age());
        let server_max_age = self.music_server.as_ref().map(|s| s.max_age());

        match (renderer_max_age, server_max_age) {
            (Some(r), Some(s)) => r.max(s), // Le plus récent
            (Some(r), None) => r,
            (None, Some(s)) => s,
            (None, None) => unreachable!("DeviceEntry must have at least one component"),
        }
    }
}

impl DeviceRegistry {
    pub fn new(renderer_bus: &RendererEventBus, server_bus: &MediaServerEventBus) -> Self {
        Self {
            devices: HashMap::new(),
            udn_index: HashMap::new(),
            renderer_bus: renderer_bus.clone(),
            server_bus: server_bus.clone(),
        }
    }

    pub fn list_renderers(&self) -> Result<Vec<Arc<MusicRenderer>>, ControlPointError> {
        self.devices
            .iter()
            .filter(|(_, item)| item.is_a_music_renderer())
            .map(|(_, item)| item.as_music_renderer())
            .collect()
    }

    pub fn list_servers(&self) -> Result<Vec<Arc<MusicServer>>, ControlPointError> {
        self.devices
            .iter()
            .filter(|(_, item)| item.is_a_music_server())
            .map(|(_, item)| item.as_music_server())
            .collect()
    }

    pub fn get_renderer(&self, id: &DeviceId) -> Option<Arc<MusicRenderer>> {
        self.devices.get(id)?.as_music_renderer().ok()
    }

    pub fn get_server(&self, id: &DeviceId) -> Option<Arc<MusicServer>> {
        self.devices.get(id)?.as_music_server().ok()
    }

    pub fn get_device_by_udn(&self, udn: &str) -> Option<DeviceItem> {
        let lookup = udn.to_ascii_lowercase();
        self.udn_index
            .get(&lookup)
            .and_then(|id| self.devices.get(id).cloned())
    }

    /// Helper: get a renderer by UDN (case-insensitive, via udn_index).
    pub fn get_renderer_by_udn(&self, udn: &str) -> Option<Arc<MusicRenderer>> {
        self.get_device_by_udn(udn)
            .and_then(|item| item.as_music_renderer().ok())
    }

    /// Helper: get a server by UDN (case-insensitive, via udn_index).
    pub fn get_server_by_udn(&self, udn: &str) -> Option<Arc<MusicServer>> {
        self.get_device_by_udn(udn)
            .and_then(|item| item.as_music_server().ok())
    }

    pub fn push_renderer(&mut self, info: &RendererInfo, max_age: u32) {
        let device_id = info.id();

        if let Some(entry) = self.devices.get_mut(&device_id) {
            if let Some(renderer) = &entry.music_renderer {
                let was_online = renderer.is_online();
                // has_been_seen_now() automatically calls start_watching() if offline→online
                renderer.has_been_seen_now(max_age);

                if !was_online {
                    self.renderer_bus.broadcast(RendererEvent::Online {
                        id: device_id.clone(),
                        info: info.basic_info(),
                    });
                }
                return;
            }
            // Entry existe mais pas de renderer -> on l'ajoute
            // Constructor automatically calls start_watching()
            if let Ok(new_renderer) =
                MusicRenderer::from_renderer_info_with_bus(info, Some(self.renderer_bus.clone()))
            {
                entry.music_renderer = Some(Arc::new(new_renderer));
                self.udn_index
                    .insert(info.udn().to_ascii_lowercase(), device_id.clone());

                self.renderer_bus.broadcast(RendererEvent::Online {
                    id: device_id.clone(),
                    info: info.basic_info(),
                });
            }
        } else {
            // Entry n'existe pas -> on crée
            // Constructor automatically calls start_watching()
            if let Ok(new_renderer) =
                MusicRenderer::from_renderer_info_with_bus(info, Some(self.renderer_bus.clone()))
            {
                let new_entry = DeviceItem {
                    music_renderer: Some(Arc::new(new_renderer)),
                    music_server: None,
                };

                self.devices.insert(device_id.clone(), new_entry);
                self.udn_index
                    .insert(info.udn().to_ascii_lowercase(), device_id.clone());

                self.renderer_bus.broadcast(RendererEvent::Online {
                    id: device_id,
                    info: info.basic_info(),
                });
            }
        }
    }

    pub fn push_server(&mut self, info: &UpnpMediaServer, max_age: u32) {
        let device_id = info.id();

        if let Some(entry) = self.devices.get_mut(&device_id) {
            if let Some(server) = &entry.music_server {
                let was_online = server.is_online();
                server.has_been_seen_now(max_age);

                if !was_online {
                    self.server_bus.broadcast(MediaServerEvent::Online {
                        server_id: device_id.clone(),
                        info: info.basic_info(),
                    });
                }

                return;
            }
            // Entry existe mais pas de renderer -> on l'ajoute
            if let Ok(new_server) = MusicServer::from_server_info(info) {
                entry.music_server = Some(Arc::new(new_server));
                self.udn_index
                    .insert(info.udn().to_ascii_lowercase(), device_id.clone());

                // Broadcast sur le bon bus
                self.server_bus.broadcast(MediaServerEvent::Online {
                    server_id: device_id.clone(),
                    info: info.basic_info(),
                });
            }
        } else {
            // Entry n'existe pas -> on crée
            if let Ok(new_server) = MusicServer::from_server_info(info) {
                let new_entry = DeviceItem {
                    music_renderer: None,
                    music_server: Some(Arc::new(new_server)),
                };

                self.devices.insert(device_id.clone(), new_entry);
                self.udn_index
                    .insert(info.udn().to_ascii_lowercase(), device_id.clone());

                // Broadcast sur le bon bus
                self.server_bus.broadcast(MediaServerEvent::Online {
                    server_id: device_id,
                    info: info.basic_info(),
                });
            }
        }
    }

    /// Updates the last_seen timestamp for a device without fetching its full description.
    ///
    /// This is critical for keeping devices online when SSDP Alive messages arrive
    /// more frequently than the UDN cache refresh interval (max_age/2).
    ///
    /// Note: has_been_seen_now() automatically calls start_watching() for renderers
    /// when transitioning from offline to online.
    pub fn refresh_device_presence(&mut self, udn: &str, max_age: u32) {
        let lookup = udn.to_ascii_lowercase();

        if let Some(id) = self.udn_index.get(&lookup) {
            if let Some(device) = self.devices.get(id) {
                let was_online = device.is_online();
                // has_been_seen_now() automatically calls start_watching() if offline→online
                device.has_been_seen_now(max_age);

                // Broadcast Online event if device came back online
                if !was_online {
                    if device.is_a_music_renderer() {
                        if let Ok(renderer) = device.as_music_renderer() {
                            self.renderer_bus.broadcast(RendererEvent::Online {
                                id: id.clone(),
                                info: renderer.info().basic_info(),
                            });
                        }
                    }
                    if device.is_a_music_server() {
                        if let Ok(server) = device.as_music_server() {
                            self.server_bus.broadcast(MediaServerEvent::Online {
                                server_id: id.clone(),
                                info: server.basic_info(),
                            });
                        }
                    }
                }
            }
        }
    }

    pub fn device_says_byebye(&mut self, udn: &str) {
        let lookup = udn.to_ascii_lowercase();

        if let Some(id) = self.udn_index.get(&lookup) {
            if let Some(device) = self.devices.get(id) {
                // mark_as_offline() automatically calls stop_watching() for renderers
                device.mark_as_offline();

                if device.is_a_music_renderer() {
                    self.renderer_bus
                        .broadcast(RendererEvent::Offline { id: id.clone() });
                }
                if device.is_a_music_server() {
                    self.server_bus.broadcast(MediaServerEvent::Offline {
                        server_id: id.clone(),
                    });
                }
            }
        }
    }

    pub fn check_timeouts(&mut self) {
        let now = SystemTime::now();

        for (id, device) in &self.devices {
            if let Ok(elapsed) = now.duration_since(device.last_seen()) {
                if elapsed.as_secs() > device.max_age() as u64 {
                    // mark_as_offline() automatically calls stop_watching() for renderers
                    device.mark_as_offline();

                    if device.is_a_music_renderer() {
                        self.renderer_bus
                            .broadcast(RendererEvent::Offline { id: id.clone() });
                    }
                    if device.is_a_music_server() {
                        self.server_bus.broadcast(MediaServerEvent::Offline {
                            server_id: id.clone(),
                        });
                    }
                }
            }
        }
    }
}
