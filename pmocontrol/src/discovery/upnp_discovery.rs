use crate::{DeviceRegistry, discovery::upnp_provider::ParsedDeviceDescription};
use pmoupnp::ssdp::SsdpEvent;
use std::sync::{Arc, Mutex};

use crate::discovery::manager::UDNRegistry;

/// Gestionnaire des événements SSDP -> DeviceUpdate.

pub struct UpnpDiscoveryManager {
    device_registry: Arc<Mutex<DeviceRegistry>>,
    udn_cache: Arc<Mutex<UDNRegistry>>,
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
                if UDNRegistry::should_fetch(self.udn_cache.clone(), &udn, max_age as u64) {
                    // ✅ Fetch + parse
                    if let Ok(info) = ParsedDeviceDescription::new(&udn, &location, &server_header,5) {
                        if let Some(renderer_info) = info.build_renderer() {
                        self.device_registry
                        .lock()
                         .expect("UDNRegistry mutex lock failed")
                          .push_renderer(&renderer_info,max_age);
                    } else {
                        if let Some(server_info) = info.build_server() {
                            self.device_registry
                                 .lock()
                                  .expect("UDNRegistry mutex lock failed")
                                   .push_server(&server_info,max_age);
                        }
                    }
                }}
            } else {
                self.device_registry
                    .lock()
                    .expect("UDNRegistry mutex lock failed")
                    .device_says_byebye(&udn);
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
