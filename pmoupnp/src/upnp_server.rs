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
use utoipa::OpenApi;

use crate::devices::errors::DeviceError;
use crate::devices::{Device, DeviceInstance, DeviceRegistry};
use crate::UpnpModel;
use crate::cache_registry::CACHE_REGISTRY;

use pmocovers::Cache as CoverCache;
use pmoaudiocache::Cache as AudioCache;

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
pub trait UpnpServerExt {
    // ========= Device Management (existant) =========

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

    // ========= Cache Management (NOUVEAU) =========

    /// Initialiser le cache de couvertures centralisé
    ///
    /// Crée le cache et enregistre les routes HTTP.
    /// Toutes les sources musicales utiliseront ce cache partagé.
    ///
    /// # Arguments
    ///
    /// * `cache_dir` - Répertoire de stockage
    /// * `limit` - Limite de taille (nombre d'images)
    ///
    /// # Returns
    ///
    /// Instance partagée du cache
    async fn init_cover_cache(&mut self, cache_dir: &str, limit: usize)
        -> Result<Arc<CoverCache>, anyhow::Error>;

    /// Initialiser le cache audio centralisé
    ///
    /// Crée le cache et enregistre les routes HTTP.
    /// Toutes les sources musicales utiliseront ce cache partagé.
    ///
    /// # Arguments
    ///
    /// * `cache_dir` - Répertoire de stockage
    /// * `limit` - Limite de taille (nombre de pistes)
    ///
    /// # Returns
    ///
    /// Instance partagée du cache
    async fn init_audio_cache(&mut self, cache_dir: &str, limit: usize)
        -> Result<Arc<AudioCache>, anyhow::Error>;

    /// Initialiser les caches depuis la configuration
    ///
    /// Utilise pmoconfig pour charger les paramètres et initialiser
    /// automatiquement les deux caches.
    ///
    /// # Returns
    ///
    /// Tuple (cache de couvertures, cache audio)
    async fn init_caches(&mut self)
        -> Result<(Arc<CoverCache>, Arc<AudioCache>), anyhow::Error>;

    /// Récupérer le cache de couvertures
    fn cover_cache(&self) -> Option<Arc<CoverCache>>;

    /// Récupérer le cache audio
    fn audio_cache(&self) -> Option<Arc<AudioCache>>;
}

// Implémentation du trait UpnpServer pour pmoserver::Server
impl UpnpServerExt for Server {
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

    // ========= Cache Management Implementation =========

    async fn init_cover_cache(&mut self, cache_dir: &str, limit: usize)
        -> Result<Arc<CoverCache>, anyhow::Error> {
        use pmocovers::new_cache;
        use pmocache::pmoserver_ext::{create_file_router_with_generator, create_api_router};

        let base_url = self.info().base_url.clone();
        let cache = Arc::new(new_cache(cache_dir, limit)?);

        // Routes de fichiers avec génération de variantes
        // Routes: GET /covers/image/{pk} et GET /covers/image/{pk}/{size}
        let variant_generator: pmocache::pmoserver_ext::ParamGenerator<pmocovers::CoversConfig> =
            Arc::new(|cache, pk, param| {
                Box::pin(async move {
                    // Si le param est numérique, c'est une taille de variante
                    if let Ok(size) = param.parse::<usize>() {
                        match pmocovers::webp::generate_variant(&cache, &pk, size).await {
                            Ok(data) => return Some(data),
                            Err(e) => {
                                tracing::warn!("Cannot generate variant {}x{} for {}: {}", size, size, pk, e);
                                return None;
                            }
                        }
                    }
                    None
                })
            });

        let file_router = create_file_router_with_generator(
            cache.clone(),
            "image/webp",
            Some(variant_generator)
        );
        self.add_router("/", file_router).await;

        // API REST générique (pmocache)
        let api_router = create_api_router(cache.clone());
        let openapi = pmocovers::ApiDoc::openapi();
        self.add_openapi(api_router, openapi, "covers").await;

        // Enregistrer base_url et cache dans le registre global
        {
            let mut registry = CACHE_REGISTRY.write().unwrap();
            registry.set_base_url(base_url);
            registry.set_cover_cache(cache.clone());
        }

        Ok(cache)
    }

    async fn init_audio_cache(&mut self, cache_dir: &str, limit: usize)
        -> Result<Arc<AudioCache>, anyhow::Error> {
        use pmoaudiocache::new_cache;
        use pmocache::pmoserver_ext::{create_file_router, create_api_router};

        let base_url = self.info().base_url.clone();
        let cache = Arc::new(new_cache(cache_dir, limit)?);

        // Routes de fichiers pour servir les pistes FLAC
        let file_router = create_file_router(cache.clone(), "audio/flac");
        self.add_router("/", file_router).await;

        // API REST générique (pmocache)
        let api_router = create_api_router(cache.clone());
        let openapi = pmoaudiocache::ApiDoc::openapi();
        self.add_openapi(api_router, openapi, "audio").await;

        // Enregistrer base_url et cache dans le registre global
        {
            let mut registry = CACHE_REGISTRY.write().unwrap();
            registry.set_base_url(base_url);
            registry.set_audio_cache(cache.clone());
        }

        Ok(cache)
    }

    async fn init_caches(&mut self)
        -> Result<(Arc<CoverCache>, Arc<AudioCache>), anyhow::Error> {
        let config = pmoconfig::get_config();

        let cover_cache = self.init_cover_cache(
            &config.get_cover_cache_dir()?,
            config.get_cover_cache_size()?
        ).await?;

        let audio_cache = self.init_audio_cache(
            &config.get_audio_cache_dir()?,
            config.get_audio_cache_size()?
        ).await?;

        Ok((cover_cache, audio_cache))
    }

    fn cover_cache(&self) -> Option<Arc<CoverCache>> {
        crate::cache_registry::get_cover_cache()
    }

    fn audio_cache(&self) -> Option<Arc<AudioCache>> {
        crate::cache_registry::get_audio_cache()
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
