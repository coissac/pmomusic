//! Extension UPnP pour pmoserver.
//!
//! Ce module fournit le trait `UpnpServer` qui étend `pmoserver::Server`
//! avec des fonctionnalités UPnP spécifiques.
//!
//! # Design Pattern
//!
//! Suit le pattern d'extension utilisé dans PMOMusic :
//! - `pmoserver::Server` reste agnostique d'UPnP
//! - Le trait `UpnpServer` ajoute les méthodes UPnP spécifiques
//! - Un `DeviceRegistry` est associé au serveur pour l'introspection
//!
//! # Architecture
//!
//! ```text
//! pmoserver::Server
//!     + UpnpServer trait
//!     + DeviceRegistry (thread_local storage)
//! ```

use std::sync::Arc;
use std::sync::RwLock;
use once_cell::sync::Lazy;

use pmoserver::Server;

use crate::devices::errors::DeviceError;
use crate::devices::{Device, DeviceInstance, DeviceRegistry};
use crate::UpnpModel;

/// Registre de devices global et thread-safe.
///
/// Utilise Lazy pour une initialisation paresseuse et RwLock pour le partage entre threads.
/// Ceci permet aux API handlers (qui s'exécutent dans des threads différents) d'accéder
/// au même registre de devices.
static DEVICE_REGISTRY: Lazy<RwLock<DeviceRegistry>> = Lazy::new(|| {
    RwLock::new(DeviceRegistry::new())
});

/// Trait pour étendre un serveur avec des fonctionnalités UPnP.
///
/// Ce trait ajoute :
/// - Enregistrement de devices UPnP
/// - Accès au registre centralisé de devices
///
/// # Design Pattern
///
/// Ce trait suit le pattern d'extension utilisé dans PMOMusic,
/// permettant d'ajouter des fonctionnalités UPnP sans modifier `pmoserver`.
///
/// # Examples
///
/// ```rust,ignore
/// use pmoupnp::UpnpServer;
/// use pmoupnp::devices::Device;
/// use pmoserver::ServerBuilder;
/// use std::sync::Arc;
///
/// let mut server = ServerBuilder::new_configured().build();
///
/// // Enregistrement de devices via le trait UpnpServer
/// let device = Arc::new(Device::new(
///     "MediaRenderer".to_string(),
///     "MediaRenderer".to_string(),
///     "My Renderer".to_string()
/// ));
/// server.register_device(device).await?;
///
/// // Introspection via le trait UpnpServer
/// let devices = server.device_registry().list_devices();
/// ```
pub trait UpnpServer {
    /// Enregistre un device UPnP et toutes ses URLs.
    ///
    /// # Arguments
    ///
    /// * `device` - Le modèle du device à enregistrer
    ///
    /// # Returns
    ///
    /// L'instance du device créée et enregistrée.
    async fn register_device(&mut self, device: Arc<Device>) -> Result<Arc<DeviceInstance>, DeviceError>;

    /// Retourne le nombre de devices enregistrés.
    fn device_count(&self) -> usize;

    /// Liste tous les devices enregistrés.
    fn list_devices(&self) -> Vec<Arc<DeviceInstance>>;

    /// Récupère un device par son UDN.
    fn get_device(&self, udn: &str) -> Option<Arc<DeviceInstance>>;
}

// Implémentation du trait UpnpServer pour pmoserver::Server
impl UpnpServer for Server {
    async fn register_device(&mut self, device: Arc<Device>) -> Result<Arc<DeviceInstance>, DeviceError> {
        // Créer l'instance (retourne déjà un Arc<DeviceInstance>)
        let di = device.create_instance();

        // Enregistrer les URLs dans le serveur web
        di.register_urls(self).await?;

        // Ajouter au registre pour l'introspection
        DEVICE_REGISTRY.write()
            .unwrap()
            .register(di.clone())
            .map_err(|e| DeviceError::UrlRegistrationError(e))?;

        Ok(di)
    }

    fn device_count(&self) -> usize {
        DEVICE_REGISTRY.read().unwrap().count()
    }

    fn list_devices(&self) -> Vec<Arc<DeviceInstance>> {
        DEVICE_REGISTRY.read().unwrap().list_devices()
    }

    fn get_device(&self, udn: &str) -> Option<Arc<DeviceInstance>> {
        DEVICE_REGISTRY.read().unwrap().get_device(udn)
    }
}

/// Fonctions helper pour accéder au registre depuis les handlers.
///
/// Ces fonctions permettent d'accéder au registre global depuis
/// n'importe où dans le code, notamment depuis les handlers Axum.

/// Exécute une closure avec un accès en lecture seule aux devices.
///
/// # Examples
///
/// ```rust,ignore
/// use pmoupnp::upnp_server::with_devices;
///
/// let device_count = with_devices(|devices| devices.len());
/// ```
pub fn with_devices<F, R>(f: F) -> R
where
    F: FnOnce(&Vec<Arc<DeviceInstance>>) -> R,
{
    let devices = DEVICE_REGISTRY.read().unwrap().list_devices();
    f(&devices)
}

/// Récupère un device par son UDN.
///
/// # Examples
///
/// ```rust,ignore
/// use pmoupnp::upnp_server::get_device_by_udn;
///
/// if let Some(device) = get_device_by_udn("uuid:...") {
///     println!("Found device: {}", device.get_name());
/// }
/// ```
pub fn get_device_by_udn(udn: &str) -> Option<Arc<DeviceInstance>> {
    DEVICE_REGISTRY.read().unwrap().get_device(udn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pmoserver::ServerBuilder;

    #[tokio::test]
    async fn test_device_registration() {
        let mut server = ServerBuilder::new("TestServer", "http://localhost:8080", 8080).build();

        let device = Arc::new(Device::new(
            "TestDevice".to_string(),
            "MediaRenderer".to_string(),
            "Test Renderer".to_string(),
        ));

        let instance = server.register_device(device).await.unwrap();

        // Vérifier que le device est dans le registre
        assert_eq!(server.device_count(), 1);

        // Vérifier qu'on peut le retrouver par UDN
        let retrieved = server.get_device(instance.udn());
        assert!(retrieved.is_some());
    }
}
