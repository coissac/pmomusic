//! Chromecast device discovery via mDNS.
//!
//! Chromecast devices advertise themselves using mDNS (Multicast DNS) on the
//! `_googlecast._tcp.local` service, unlike UPnP devices which use SSDP.
//! This module handles the discovery of Chromecast devices and converts them
//! into `DeviceUpdate` events that can be processed by the `DeviceRegistry`.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::SystemTime;

use crate::registry::DeviceUpdate;
use crate::model::{RendererCapabilities, RendererInfo, RendererProtocol, RendererId};
use tracing::{debug, warn};

/// Information about a discovered Chromecast device from mDNS.
#[derive(Clone, Debug)]
pub struct DiscoveredChromecast {
    pub friendly_name: String,
    pub host: String,
    pub port: u16,
    pub model: Option<String>,
    pub uuid: String,
    pub manufacturer: Option<String>,
    pub last_seen: SystemTime,
}

/// Manages the discovery and tracking of Chromecast devices via mDNS.
pub struct ChromecastDiscoveryManager {
    discovered_devices: HashMap<String, DiscoveredChromecast>,
}

impl ChromecastDiscoveryManager {
    pub fn new() -> Self {
        Self {
            discovered_devices: HashMap::new(),
        }
    }

    /// Adds or updates a discovered Chromecast device.
    pub fn update_device(&mut self, device: DiscoveredChromecast) {
        let uuid = device.uuid.clone();
        self.discovered_devices.insert(uuid, device);
    }

    /// Retrieves a discovered device by UUID.
    pub fn get_device(&self, uuid: &str) -> Option<&DiscoveredChromecast> {
        self.discovered_devices.get(uuid)
    }

    /// Lists all discovered devices.
    pub fn list_devices(&self) -> Vec<&DiscoveredChromecast> {
        self.discovered_devices.values().collect()
    }
}

/// Processes an mDNS response and converts it into a `DeviceUpdate` event.
///
/// This function parses mDNS service discovery responses for Chromecast
/// devices and creates the appropriate update event for the device registry.
pub fn process_mdns_response(response: mdns::Response) -> Option<DeviceUpdate> {
    // Extract basic information from the mDNS response
    let service_name = response.records().filter_map(|r| {
        if let mdns::RecordKind::PTR(ref name) = r.kind {
            Some(name.clone())
        } else {
            None
        }
    }).next()?;

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
        return None;
    }

    // Prefer IPv4 addresses
    let host = addresses
        .iter()
        .find(|addr| matches!(addr, IpAddr::V4(_)))
        .or_else(|| addresses.first())
        .map(|addr| addr.to_string())?;

    // Extract port from SRV record
    let port = response
        .records()
        .filter_map(|r| {
            if let mdns::RecordKind::SRV { port, .. } = r.kind {
                Some(port)
            } else {
                None
            }
        })
        .next()
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

    // Create RendererInfo for the registry
    let renderer_info = build_renderer_info(
        &uuid,
        &friendly_name,
        &host,
        port,
        model.as_deref(),
        manufacturer.as_deref(),
    );

    Some(DeviceUpdate::RendererOnline(renderer_info))
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
    let id = RendererId(udn.clone());

    // Build Chromecast capabilities
    let mut capabilities = RendererCapabilities::default();
    capabilities.has_chromecast = true;

    // The location URL for Chromecast is just the host:port
    // (not a real HTTP endpoint like UPnP, but useful for identification)
    let location = format!("chromecast://{}:{}", host, port);

    RendererInfo {
        id,
        udn,
        friendly_name: friendly_name.to_string(),
        model_name: model.unwrap_or("Chromecast").to_string(),
        manufacturer: manufacturer.unwrap_or("Google Inc.").to_string(),
        protocol: RendererProtocol::ChromecastOnly,
        capabilities,
        location,
        server_header: "Chromecast".to_string(),
        online: true,
        last_seen: SystemTime::now(),
        max_age: 1800, // 30 minutes
        // All UPnP/OpenHome fields are None for Chromecast
        avtransport_service_type: None,
        avtransport_control_url: None,
        rendering_control_service_type: None,
        rendering_control_control_url: None,
        connection_manager_service_type: None,
        connection_manager_control_url: None,
        oh_playlist_service_type: None,
        oh_playlist_control_url: None,
        oh_playlist_event_sub_url: None,
        oh_info_service_type: None,
        oh_info_control_url: None,
        oh_info_event_sub_url: None,
        oh_time_service_type: None,
        oh_time_control_url: None,
        oh_time_event_sub_url: None,
        oh_volume_service_type: None,
        oh_volume_control_url: None,
        oh_radio_service_type: None,
        oh_radio_control_url: None,
        oh_product_service_type: None,
        oh_product_control_url: None,
    }
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
