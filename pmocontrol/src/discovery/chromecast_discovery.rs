//! Chromecast device discovery via mDNS.
//!
//! Chromecast devices advertise themselves using mDNS (Multicast DNS) on the
//! `_googlecast._tcp.local` service, unlike UPnP devices which use SSDP.
//! This module handles the discovery of Chromecast devices and registers them
//! directly into the `DeviceRegistry`.

use std::sync::{Arc, Mutex, RwLock};

use mdns_sd::ResolvedService;
use mdns_sd::ServiceInfo;

use crate::discovery::manager::UDNRegistry;
use crate::model::{RendererCapabilities, RendererInfo, RendererProtocol};
use crate::DeviceId;
use crate::DeviceRegistry;
use tracing::{debug, warn};

/// Gestionnaire des événements mDNS pour Chromecast.
pub struct ChromecastDiscoveryManager {
    device_registry: Arc<RwLock<DeviceRegistry>>,
    udn_cache: Arc<Mutex<UDNRegistry>>,
}

impl ChromecastDiscoveryManager {
    pub fn new(
        device_registry: Arc<RwLock<DeviceRegistry>>,
        udn_cache: Arc<Mutex<UDNRegistry>>,
    ) -> Self {
        Self {
            device_registry,
            udn_cache,
        }
    }

    /// Traite un service Chromecast résolu par mDNS-SD.
    ///
    /// `ServiceInfo` arrive pré-assemblé : plus besoin de jointure manuelle
    /// des enregistrements PTR / A / SRV / TXT.
    pub fn handle_service_resolved(&mut self, info: &ResolvedService) {
        let fullname = info.get_fullname().to_string();

        debug!("Processing resolved Chromecast service: {}", fullname);

        let host = match info
            .get_addresses()
            .iter()
            .find(|a| a.is_ipv4())
            .or_else(|| info.get_addresses().iter().next())
        {
            Some(addr) => addr.to_string(),
            None => {
                warn!("No IP address for Chromecast service: {}", fullname);
                return;
            }
        };

        let port = info.get_port();

        let uuid = info
            .get_property_val_str("id")
            .unwrap_or_default()
            .to_string();
        let uuid = if uuid.is_empty() {
            format!("chromecast-{}-{}", host, port)
        } else {
            uuid
        };

        let model = info.get_property_val_str("md").map(|s| s.to_string());

        let friendly_name = info
            .get_property_val_str("fn")
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                fullname
                    .split("._googlecast._tcp.local")
                    .next()
                    .unwrap_or("Unknown Chromecast")
                    .split('-')
                    .take_while(|part| part.len() != 32)
                    .collect::<Vec<_>>()
                    .join("-")
                    .trim()
                    .to_string()
            });

        debug!(
            "Discovered Chromecast: {} at {}:{} (UUID: {}, Model: {:?})",
            friendly_name, host, port, uuid, model
        );

        let udn = format!("uuid:{}", uuid);
        let default_max_age = 1800u64;

        if !UDNRegistry::should_fetch(self.udn_cache.clone(), &udn, default_max_age) {
            debug!("Chromecast {} recently seen, skipping", udn);
            return;
        }

        let renderer_info = build_renderer_info(
            &uuid,
            &friendly_name,
            &host,
            port,
            model.as_deref(),
            Some("Google Inc."),
        );

        self.device_registry
            .write()
            .expect("DeviceRegistry mutex lock failed")
            .push_renderer(&renderer_info, default_max_age as u32);
    }
}

/// Builds a `RendererInfo` structure for a Chromecast device.
fn build_renderer_info(
    uuid: &str,
    friendly_name: &str,
    host: &str,
    port: u16,
    model: Option<&str>,
    manufacturer: Option<&str>,
) -> RendererInfo {
    let udn = format!("uuid:{}", uuid);
    let id = DeviceId(udn.clone());

    // Build Chromecast capabilities
    let mut capabilities = RendererCapabilities::default();
    capabilities.has_chromecast = true;

    // The location URL for Chromecast is just the host:port
    // (not a real HTTP endpoint like UPnP, but useful for identification)
    let location = format!("chromecast://{}:{}", host, port);

    RendererInfo::make(
        id,
        udn,
        friendly_name.to_string(),
        model.unwrap_or("Chromecast").to_string(),
        manufacturer.unwrap_or("Google Inc.").to_string(),
        RendererProtocol::ChromecastOnly,
        capabilities,
        location,
        "Chromecast".to_string(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
}

/// Extracts the host (IP address) from a Chromecast location URL.
///
/// The location format is `chromecast://host:port`.
pub fn extract_host_from_location(location: &str) -> Option<String> {
    if let Some(stripped) = location.strip_prefix("chromecast://") {
        let host = stripped.split(':').next()?;
        Some(host.to_string())
    } else {
        None
    }
}

/// Extracts the port from a Chromecast location URL.
///
/// The location format is `chromecast://host:port`.
pub fn extract_port_from_location(location: &str) -> Option<u16> {
    if let Some(stripped) = location.strip_prefix("chromecast://") {
        let port_str = stripped.split(':').nth(1)?;
        port_str.parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_host_from_location() {
        assert_eq!(
            extract_host_from_location("chromecast://192.168.1.100:8009"),
            Some("192.168.1.100".to_string())
        );
        assert_eq!(
            extract_host_from_location("http://192.168.1.100:8009"),
            None
        );
    }

    #[test]
    fn test_extract_port_from_location() {
        assert_eq!(
            extract_port_from_location("chromecast://192.168.1.100:8009"),
            Some(8009)
        );
        assert_eq!(
            extract_port_from_location("chromecast://192.168.1.100"),
            None
        );
    }
}
