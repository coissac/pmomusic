use crate::{DeviceRegistry, discovery::upnp_provider::ParsedDeviceDescription};
use pmoupnp::ssdp::SsdpEvent;
use std::sync::{Arc, Mutex, RwLock};

use crate::discovery::manager::UDNRegistry;

/// Gestionnaire des événements SSDP -> DeviceUpdate.

pub struct UpnpDiscoveryManager {
    device_registry: Arc<RwLock<DeviceRegistry>>,
    udn_cache: Arc<Mutex<UDNRegistry>>,
}

impl UpnpDiscoveryManager {
    pub fn new(
        device_registry: Arc<RwLock<DeviceRegistry>>,
        udn_cache: Arc<Mutex<UDNRegistry>>,
    ) -> Self {
        Self {
            device_registry,
            udn_cache,
        }
    }

    pub fn handle_ssdp_event(&mut self, event: SsdpEvent) {
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
                // Check if we should fetch the full device description
                let should_fetch =
                    UDNRegistry::should_fetch(self.udn_cache.clone(), &udn, max_age as u64);

                if should_fetch {
                    // Fetch + parse the device description
                    if let Ok(info) =
                        ParsedDeviceDescription::new(&udn, &location, &server_header, 5)
                    {
                        if let Some(renderer_info) = info.build_renderer() {
                            if let Ok(mut reg) = self.device_registry.write() {
                                reg.push_renderer(&renderer_info, max_age);
                            }
                        } else if let Some(server_info) = info.build_server() {
                            if let Ok(mut reg) = self.device_registry.write() {
                                reg.push_server(&server_info, max_age);
                            }
                        }
                    }
                } else {
                    // Even if we don't fetch, we MUST update last_seen to prevent timeout
                    // This is critical: SSDP Alive messages arrive more frequently than max_age/2,
                    // and we need to acknowledge them to keep the device online
                    if let Ok(mut reg) = self.device_registry.write() {
                        reg.refresh_device_presence(&udn, max_age);
                    }
                }
            } else {
                if let Ok(mut reg) = self.device_registry.write() {
                    reg.device_says_byebye(&udn);
                }
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
