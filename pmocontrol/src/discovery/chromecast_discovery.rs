//! Chromecast device discovery via mDNS.
//!
//! Chromecast devices advertise themselves using mDNS (Multicast DNS) on the
//! `_googlecast._tcp.local` service, unlike UPnP devices which use SSDP.
//! This module handles the discovery of Chromecast devices and registers them
//! directly into the `DeviceRegistry`.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex, RwLock};

use crate::DeviceId;
use crate::DeviceRegistry;
use crate::discovery::manager::UDNRegistry;
use crate::model::{RendererCapabilities, RendererInfo, RendererProtocol};
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

    /// Traite une réponse mDNS pour un appareil Chromecast.
    ///
    /// Cette fonction parse les réponses de service discovery mDNS pour les appareils
    /// Chromecast et les enregistre directement dans le registre.
    pub fn handle_mdns_response(&mut self, response: mdns::Response) {
        // Extract basic information from the mDNS response
        let service_name = match response.records().find_map(|r| {
            if let mdns::RecordKind::PTR(ref name) = r.kind {
                Some(name.clone())
            } else {
                None
            }
        }) {
            Some(name) => name,
            None => {
                warn!("No PTR record found in mDNS response");
                return;
            }
        };

        debug!("Processing mDNS response for service: {}", service_name);

        // Extract IP addresses
        let addresses: Vec<IpAddr> = response
            .records()
            .filter_map(|r| match r.kind {
                mdns::RecordKind::A(addr) => Some(IpAddr::V4(addr)),
                mdns::RecordKind::AAAA(addr) => Some(IpAddr::V6(addr)),
                _ => None,
            })
            .collect();

        if addresses.is_empty() {
            warn!("No IP address found for Chromecast device: {}", service_name);
            return;
        }

        // Prefer IPv4 addresses
        let host = match addresses
            .iter()
            .find(|addr| matches!(addr, IpAddr::V4(_)))
            .or_else(|| addresses.first())
        {
            Some(addr) => addr.to_string(),
            None => {
                warn!("Could not extract host from addresses");
                return;
            }
        };

        // Extract port from SRV record
        let port = response
            .records()
            .find_map(|r| {
                if let mdns::RecordKind::SRV { port, .. } = r.kind {
                    Some(port)
                } else {
                    None
                }
            })
            .unwrap_or(8009); // Default Chromecast port

        // Extract TXT records for additional metadata
        let txt_records: HashMap<String, String> = response
            .records()
            .filter_map(|r| {
                if let mdns::RecordKind::TXT(ref data) = r.kind {
                    Some(data.clone())
                } else {
                    None
                }
            })
            .flat_map(|data| {
                // data is Vec<String>, each string is "key=value"
                data.into_iter().filter_map(|s| {
                    let parts: Vec<&str> = s.splitn(2, '=').collect();
                    if parts.len() == 2 {
                        Some((parts[0].to_string(), parts[1].to_string()))
                    } else {
                        None
                    }
                })
            })
            .collect();

        // Extract metadata from TXT records
        let model = txt_records.get("md").cloned();
        let uuid = txt_records
            .get("id")
            .cloned()
            .unwrap_or_else(|| format!("chromecast-{}-{}", host, port));
        let manufacturer = Some("Google Inc.".to_string());

        // Extract friendly name from TXT record "fn" if available
        // Otherwise, extract from service instance name (PTR record)
        let friendly_name = txt_records
            .get("fn")
            .cloned()
            .unwrap_or_else(|| {
                // Fallback: extract from service name, removing the UUID suffix if present
                service_name
                    .split("._googlecast._tcp.local")
                    .next()
                    .unwrap_or("Unknown Chromecast")
                    .split('-')
                    .take_while(|part| part.len() != 32) // Skip 32-char hex UUID
                    .collect::<Vec<_>>()
                    .join("-")
                    .trim()
                    .to_string()
            });

        debug!(
            "Discovered Chromecast: {} at {}:{} (UUID: {}, Model: {:?})",
            friendly_name, host, port, uuid, model
        );

        // Build UDN and check cache
        let udn = format!("uuid:{}", uuid);

        // Pour Chromecast, on utilise un max_age par défaut car mDNS n'a pas ce concept
        let default_max_age = 1800u64; // 30 minutes

        // Check cache to avoid redundant updates
        if !UDNRegistry::should_fetch(self.udn_cache.clone(), &udn, default_max_age) {
            debug!("Chromecast {} recently seen, skipping", udn);
            return;
        }

        // Create RendererInfo for the registry
        let renderer_info = build_renderer_info(
            &uuid,
            &friendly_name,
            &host,
            port,
            model.as_deref(),
            manufacturer.as_deref(),
        );

        // Register the renderer
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
