//! Représentation d'un device SSDP

/// Device SSDP avec ses métadonnées pour les annonces
#[derive(Debug, Clone)]
pub struct SsdpDevice {
    /// UUID du device (sans le préfixe "uuid:")
    pub uuid: String,

    /// Type du device (ex: "urn:schemas-upnp-org:device:MediaRenderer:1")
    pub device_type: String,

    /// URL de la description du device
    pub location: String,

    /// Identifiant du serveur (ex: "Linux/5.0 UPnP/1.1 PMOMusic/1.0")
    pub server: String,

    /// Liste des types de notification (NT) à annoncer
    /// Typiquement: [uuid:xxx, device_type, services...]
    pub notification_types: Vec<String>,
}

impl SsdpDevice {
    /// Crée un nouveau device SSDP
    pub fn new(
        uuid: String,
        device_type: String,
        location: String,
        server: String,
    ) -> Self {
        // Construction automatique des NTs standards
        let notification_types = vec![
            format!("uuid:{}", uuid),
            "upnp:rootdevice".to_string(),
            device_type.clone(),
        ];

        Self {
            uuid,
            device_type,
            location,
            server,
            notification_types,
        }
    }

    /// Ajoute un type de notification (ex: pour un service)
    pub fn add_notification_type(&mut self, nt: String) {
        if !self.notification_types.contains(&nt) {
            self.notification_types.push(nt);
        }
    }

    /// Retourne la liste des types de notification
    pub fn get_notification_types(&self) -> &[String] {
        &self.notification_types
    }
}
