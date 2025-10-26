//! Radio Paradise UPnP Media Server implementation

use crate::error::{Error, Result};
use crate::RadioParadiseClient;
use pmoserver::Server;
use pmoupnp::devices::Device;
use pmoupnp::UpnpServer;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Radio Paradise UPnP Media Server
///
/// Exposes Radio Paradise blocks and songs as a browsable UPnP media library.
pub struct RadioParadiseMediaServer {
    server: Server,
    client: Arc<RwLock<RadioParadiseClient>>,
    device_udn: String,
}

impl RadioParadiseMediaServer {
    /// Create a new builder for the media server
    pub fn builder() -> MediaServerBuilder {
        MediaServerBuilder::default()
    }

    /// Create a new media server with default settings
    pub async fn new() -> Result<Self> {
        Self::builder().build().await
    }

    /// Run the media server
    ///
    /// This will start the HTTP server and SSDP announcements.
    pub async fn run(self) -> Result<()> {
        self.server
            .run()
            .await
            .map_err(|e| Error::other(format!("Server error: {}", e)))
    }

    /// Get the device UDN
    pub fn udn(&self) -> &str {
        &self.device_udn
    }

    /// Get the Radio Paradise client
    pub fn client(&self) -> Arc<RwLock<RadioParadiseClient>> {
        self.client.clone()
    }
}

/// Builder for RadioParadiseMediaServer
pub struct MediaServerBuilder {
    friendly_name: String,
    manufacturer: String,
    model_name: String,
    channel: u8,
    port: u16,
}

impl Default for MediaServerBuilder {
    fn default() -> Self {
        Self {
            friendly_name: "Radio Paradise Media Server".to_string(),
            manufacturer: "PMOMusic".to_string(),
            model_name: "Radio Paradise Adapter".to_string(),
            channel: 0,
            port: 8080,
        }
    }
}

impl MediaServerBuilder {
    /// Create a new builder with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the friendly name for the device
    pub fn with_friendly_name(mut self, name: impl Into<String>) -> Self {
        self.friendly_name = name.into();
        self
    }

    /// Set the manufacturer name
    pub fn with_manufacturer(mut self, name: impl Into<String>) -> Self {
        self.manufacturer = name.into();
        self
    }

    /// Set the model name
    pub fn with_model_name(mut self, name: impl Into<String>) -> Self {
        self.model_name = name.into();
        self
    }

    /// Set the Radio Paradise channel (0=main, 1=mellow, 2=rock, 3=world)
    pub fn with_channel(mut self, channel: u8) -> Self {
        self.channel = channel;
        self
    }

    /// Set the HTTP server port
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Build the media server
    pub async fn build(self) -> Result<RadioParadiseMediaServer> {
        // Create Radio Paradise client
        let client = RadioParadiseClient::builder()
            .channel(self.channel)
            .build()
            .await?;

        let client = Arc::new(RwLock::new(client));

        // Create HTTP server
        let mut server = pmoserver::ServerBuilder::new()
            .with_port(self.port)
            .build()
            .map_err(|e| Error::other(format!("Failed to create server: {}", e)))?;

        // Create UPnP device
        let device_udn = format!("uuid:{}", uuid::Uuid::new_v4());

        let mut device = Device::new(
            "MediaServer".to_string(),
            "MediaServer".to_string(),
            self.friendly_name.clone(),
        );

        device.set_manufacturer(self.manufacturer);
        device.set_model_name(self.model_name);
        device.set_udn(device_udn.clone());

        // Add ContentDirectory service
        let content_directory =
            super::content_directory::create_content_directory_service(client.clone());
        device
            .add_service(Arc::new(content_directory))
            .map_err(|e| Error::other(format!("Failed to add ContentDirectory: {:?}", e)))?;

        // Add ConnectionManager service
        let connection_manager = super::connection_manager::create_connection_manager_service();
        device
            .add_service(Arc::new(connection_manager))
            .map_err(|e| Error::other(format!("Failed to add ConnectionManager: {:?}", e)))?;

        // Register device with server
        server
            .register_device(Arc::new(device))
            .await
            .map_err(|e| Error::other(format!("Failed to register device: {:?}", e)))?;

        Ok(RadioParadiseMediaServer {
            server,
            client,
            device_udn,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_defaults() {
        let builder = MediaServerBuilder::default();
        assert_eq!(builder.friendly_name, "Radio Paradise Media Server");
        assert_eq!(builder.channel, 0);
        assert_eq!(builder.port, 8080);
    }

    #[test]
    fn test_builder_customization() {
        let builder = MediaServerBuilder::new()
            .with_friendly_name("Custom Server")
            .with_channel(1)
            .with_port(9090);

        assert_eq!(builder.friendly_name, "Custom Server");
        assert_eq!(builder.channel, 1);
        assert_eq!(builder.port, 9090);
    }
}
