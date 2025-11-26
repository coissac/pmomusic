///! Extension trait pour initialiser le PMO Music MediaServer UPnP

use pmoupnp::devices::DeviceInstance;
use pmoupnp::variable_types::StateValue;
use std::sync::Arc;
use tracing::{info, warn};

/// Extension trait pour initialiser les variables UPnP du MediaServer
pub trait MediaServerDeviceExt {
    /// Initialise les ProtocolInfo du ConnectionManager pour PMO Music.
    ///
    /// PMO Music convertit tous les flux audio en FLAC (et OGG-FLAC).
    /// Cette méthode configure le `SourceProtocolInfo` avec les formats supportés:
    /// - `http-get:*:audio/flac:*` - FLAC standard
    /// - `http-get:*:application/ogg:*` - OGG-FLAC
    /// - `http-get:*:audio/ogg:*` - OGG-FLAC (format alternatif)
    ///
    /// # Arguments
    ///
    /// * `device_instance` - L'instance du MediaServer device
    ///
    /// # Returns
    ///
    /// `Ok(())` si l'initialisation réussit, `Err` sinon.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use pmomediaserver::MediaServerDeviceExt;
    /// use pmomediaserver::MEDIA_SERVER;
    ///
    /// let server_instance = server
    ///     .write().await
    ///     .register_device(MEDIA_SERVER.clone())
    ///     .await?;
    ///
    /// server_instance.init_protocol_info();
    /// ```
    fn init_protocol_info(&self);
}

impl MediaServerDeviceExt for Arc<DeviceInstance> {
    fn init_protocol_info(&self) {
        // Liste des formats que PMO Music peut servir
        // PMO Music convertit tout au vol en FLAC
        let protocol_info = vec![
            // FLAC standard (format principal)
            "http-get:*:audio/flac:*",
            "http-get:*:audio/x-flac:*",
            "http-get:*:application/flac:*",
            "http-get:*:application/x-flac:*",
            // OGG-FLAC
            "http-get:*:application/ogg:*",
            "http-get:*:audio/ogg:*",
            "http-get:*:audio/x-ogg:*",
        ];

        let source_protocol_info = protocol_info.join(",");

        info!("🔧 Initializing MediaServer ProtocolInfo:");
        info!("   Source: {}", source_protocol_info);

        // Accéder au service ConnectionManager
        if let Some(conn_mgr) = self.get_service("ConnectionManager") {
            // Initialiser SourceProtocolInfo (formats que le serveur peut fournir)
            if let Some(source_var) = conn_mgr.get_variable("SourceProtocolInfo") {
                tokio::spawn(async move {
                    if let Err(e) = source_var
                        .set_value(StateValue::String(source_protocol_info.clone()))
                        .await
                    {
                        warn!("⚠️ Failed to set SourceProtocolInfo: {}", e);
                    } else {
                        info!("✅ SourceProtocolInfo initialized");
                    }
                });
            } else {
                warn!("⚠️ SourceProtocolInfo variable not found in ConnectionManager");
            }

            // SinkProtocolInfo reste vide pour un MediaServer (il ne consomme pas de contenu)
        } else {
            warn!("⚠️ ConnectionManager service not found in MediaServer");
        }
    }
}
