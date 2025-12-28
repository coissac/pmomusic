use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use tracing::warn;

use crate::errors::ControlPointError;
use crate::media_server::MusicServer;
use crate::model::RendererInfo;
use crate::music_renderer::MusicRenderer;
use crate::{DeviceId, DeviceIdentity, DeviceOnline, UpnpMediaServer};

const DEFAULT_MAX_AGE: u32 = 1800;

#[derive(Debug, Clone)]
pub struct DeviceItem {
    music_rendrer : Option<Arc<MusicRenderer>>,
    music_server: Option<Arc<MusicServer>>,
}

#[derive(Debug, Default)]
pub struct DeviceRegistry {
    devices: HashMap<DeviceId, DeviceItem>,
    udn_index: HashMap<String, DeviceId>,
}

#[derive(Debug, Clone)]
pub enum DeviceUpdate {
    OfflineById(DeviceId),
    OfflineByUdn(String),
}

impl DeviceItem {
    pub fn as_music_renderer(&self) -> Result<Arc<MusicRenderer>, ControlPointError> {
        match self {
            DeviceItem::MusicRenderer(renderer) => Ok(Arc::clone(renderer)),
            _ => Err(ControlPointError::IsNotAMediaRender(format!("{:#?}", self))),
        }
    }

    pub fn is_a_music_renderer(&self) -> bool {
        match self {
            DeviceItem::MusicRenderer(_) => true,
            _ => false,
        }
    }

    pub fn as_music_server(&self) -> Result<Arc<MusicServer>, ControlPointError> {
        match self {
            DeviceItem::MusicServer(server) => Ok(Arc::clone(server)),
            _ => Err(ControlPointError::IsNotAMediaServer(format!("{:#?}", self))),
        }
    }

    pub fn is_a_music_server(&self) -> bool {
        match self {
            DeviceItem::MusicServer(_) => true,
            _ => false,
        }
    }
}

impl DeviceOnline for DeviceItem {
    fn is_online(&self) -> bool {
        match self {
            DeviceItem::MusicRenderer(r) => r.is_online(),
            DeviceItem::MusicServer(s) => s.is_online(),
        }
    }

    fn last_seen(&self) -> SystemTime {
        match self {
            DeviceItem::MusicRenderer(r) => r.last_seen(),
            DeviceItem::MusicServer(s) => s.last_seen(),
        }
    }

    fn has_been_seen_now(&self, max_age: u32) {
        match self {
            DeviceItem::MusicRenderer(r) => r.has_been_seen_now(max_age),
            DeviceItem::MusicServer(s) => s.has_been_seen_now(max_age),
        }
    }

    fn mark_as_offline(&self) {
        match self {
            DeviceItem::MusicRenderer(r) => r.mark_as_offline(),
            DeviceItem::MusicServer(s) => s.mark_as_offline(),
        }
    }

    fn max_age(&self) -> u32 {
        match self {
            DeviceItem::MusicRenderer(r) => r.max_age(),
            DeviceItem::MusicServer(s) => s.max_age(),
        }
    }
}

impl DeviceRegistry {
    pub fn new() -> Self {
        Self::default()
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

    pub fn push_renderer(&mut self, info: RendererInfo, max_age: u32) {
        if let Some(existing) = self.devices.get(&info.id()) {
            existing.has_been_seen_now(max_age);
        } else {
            //    let renderer = MusicRenderer::from_renderer_info(info);
            match MusicRenderer::from_renderer_info(info.clone()) {
                Ok(renderer) => {
                    self.devices
                        .insert(info.id(), DeviceItem::MusicRenderer(renderer));
                    self.udn_index.insert(info.udn().to_ascii_lowercase(), info.id());
                }
                Err(err) => {
                    warn!("Failed to create renderer: {:#?}\n", err)
                }
            }
        }
    }

    pub fn push_server(&mut self, info: UpnpMediaServer, max_age: u32) {
        if let Some(existing) = self.devices.get(&info.id()) {
            existing.has_been_seen_now(max_age);
        } else {
            let server = MusicServer::Upnp(info.clone());
            self.devices
                .insert(info.id(), DeviceItem::MusicServer(Arc::new(server)));
            self.udn_index.insert(info.udn().to_ascii_lowercase(), info.id());
        }
    }

    pub fn device_says_byebye(&mut self, udn: &str) {
        if let Some(device) =  self.get_device_by_udn(udn) {
            device.mark_as_offline();
        }
    }

    pub fn apply_update(&mut self, update: DeviceUpdate) {
        match update {
            DeviceUpdate::OfflineById(id) => {
                if let Some(renderer) = self.devices.get(&id) {
                    renderer.mark_as_offline();
                }
            }
            DeviceUpdate::OfflineByUdn(udn) => {
                let lookup = udn.to_ascii_lowercase();
                if let Some(id) = self.udn_index.get(&lookup) {
                    if let Some(renderer) = self.devices.get(id) {
                        renderer.mark_as_offline();
                    }
                }
            }
        }
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
}
