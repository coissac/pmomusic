//! # Extension trait pour le serveur MediaServer
//!
//! Ce module fournit un trait d'extension pour `pmoserver::Server` permettant
//! d'enregistrer facilement des sources musicales et de configurer le MediaServer.

use crate::source_registry::SourceRegistry;
use async_trait::async_trait;
use pmosource::MusicSource;
use pmoserver::Server;
use std::sync::Arc;
use tokio::sync::OnceCell;

/// Extension pour le registre de sources au niveau global
///
/// Ce registre est partagé par toutes les instances du serveur et permet
/// d'accéder aux sources musicales depuis n'importe où dans l'application.
static GLOBAL_REGISTRY: OnceCell<SourceRegistry> = OnceCell::const_new();

/// Initialise le registre global
///
/// Cette fonction est appelée automatiquement lors de la première utilisation.
async fn init_global_registry() -> &'static SourceRegistry {
    GLOBAL_REGISTRY
        .get_or_init(|| async { SourceRegistry::new() })
        .await
}

/// Récupère le registre global de sources
///
/// # Examples
///
/// ```ignore
/// use pmomediaserver::server_ext::get_source_registry;
///
/// let registry = get_source_registry().await;
/// if let Some(source) = registry.get("qobuz").await {
///     // Utiliser la source
/// }
/// ```
pub async fn get_source_registry() -> &'static SourceRegistry {
    init_global_registry().await
}

/// Trait d'extension pour le serveur permettant l'enregistrement de sources musicales
///
/// Ce trait ajoute des méthodes pratiques à `Server` pour enregistrer des sources
/// musicales et les rendre disponibles via le MediaServer.
///
/// # Examples
///
/// ```ignore
/// use pmomediaserver::server_ext::MediaServerExt;
/// use pmoserver::ServerBuilder;
///
/// let mut server = ServerBuilder::new_configured().build();
///
/// // Enregistrer une source
/// let qobuz = Arc::new(QobuzSource::new());
/// server.register_music_source(qobuz).await;
///
/// // Lister toutes les sources
/// let sources = server.list_music_sources().await;
/// ```
#[async_trait]
pub trait MediaServerExt {
    /// Enregistre une source musicale dans le MediaServer
    ///
    /// La source devient immédiatement disponible via le service ContentDirectory
    /// et peut être parcourue par les clients UPnP.
    ///
    /// # Arguments
    ///
    /// * `source` - La source musicale à enregistrer (Arc<dyn MusicSource>)
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let qobuz = Arc::new(QobuzSource::new(credentials));
    /// server.register_music_source(qobuz).await;
    /// ```
    async fn register_music_source(&mut self, source: Arc<dyn MusicSource>);

    /// Récupère une source musicale par son ID
    ///
    /// # Arguments
    ///
    /// * `id` - L'ID unique de la source
    ///
    /// # Returns
    ///
    /// Un `Arc` vers la source si elle existe, ou `None`.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// if let Some(source) = server.get_music_source("qobuz").await {
    ///     println!("Found: {}", source.name());
    /// }
    /// ```
    async fn get_music_source(&self, id: &str) -> Option<Arc<dyn MusicSource>>;

    /// Liste toutes les sources musicales enregistrées
    ///
    /// # Returns
    ///
    /// Un vecteur contenant toutes les sources enregistrées.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let sources = server.list_music_sources().await;
    /// for source in sources {
    ///     println!("- {} ({})", source.name(), source.id());
    /// }
    /// ```
    async fn list_music_sources(&self) -> Vec<Arc<dyn MusicSource>>;

    /// Compte le nombre de sources musicales enregistrées
    ///
    /// # Returns
    ///
    /// Le nombre total de sources.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let count = server.count_music_sources().await;
    /// println!("Total sources: {}", count);
    /// ```
    async fn count_music_sources(&self) -> usize;

    /// Supprime une source musicale du registre
    ///
    /// # Arguments
    ///
    /// * `id` - L'ID de la source à supprimer
    ///
    /// # Returns
    ///
    /// `true` si la source a été supprimée, `false` si elle n'existait pas.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// if server.remove_music_source("old-radio").await {
    ///     println!("Source removed");
    /// }
    /// ```
    async fn remove_music_source(&mut self, id: &str) -> bool;
}

#[async_trait]
impl MediaServerExt for Server {
    async fn register_music_source(&mut self, source: Arc<dyn MusicSource>) {
        let registry = get_source_registry().await;

        tracing::info!(
            source_id = %source.id(),
            source_name = %source.name(),
            "Registering music source to MediaServer"
        );

        registry.register(source).await;
    }

    async fn get_music_source(&self, id: &str) -> Option<Arc<dyn MusicSource>> {
        let registry = get_source_registry().await;
        registry.get(id).await
    }

    async fn list_music_sources(&self) -> Vec<Arc<dyn MusicSource>> {
        let registry = get_source_registry().await;
        registry.list_all().await
    }

    async fn count_music_sources(&self) -> usize {
        let registry = get_source_registry().await;
        registry.count().await
    }

    async fn remove_music_source(&mut self, id: &str) -> bool {
        let registry = get_source_registry().await;
        registry.remove(id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pmosource::{MusicSource, Result, BrowseResult};
    use pmodidl::{Container, Item};
    use std::time::SystemTime;

    #[derive(Debug)]
    struct DummySource {
        id: String,
        name: String,
    }

    impl DummySource {
        fn new(id: &str, name: &str) -> Self {
            Self {
                id: id.to_string(),
                name: name.to_string(),
            }
        }
    }

    #[async_trait::async_trait]
    impl MusicSource for DummySource {
        fn name(&self) -> &str {
            &self.name
        }

        fn id(&self) -> &str {
            &self.id
        }

        fn default_image(&self) -> &[u8] {
            &[]
        }

        async fn root_container(&self) -> Result<Container> {
            Ok(Container {
                id: self.id.clone(),
                parent_id: "0".to_string(),
                restricted: Some("1".to_string()),
                child_count: Some("0".to_string()),
                title: self.name.clone(),
                class: "object.container".to_string(),
                containers: vec![],
                items: vec![],
            })
        }

        async fn browse(&self, _object_id: &str) -> Result<BrowseResult> {
            Ok(BrowseResult::Items(vec![]))
        }

        async fn resolve_uri(&self, object_id: &str) -> Result<String> {
            Ok(format!("http://example.com/{}", object_id))
        }

        fn supports_fifo(&self) -> bool {
            false
        }

        async fn append_track(&self, _track: Item) -> Result<()> {
            Err(pmosource::MusicSourceError::FifoNotSupported)
        }

        async fn remove_oldest(&self) -> Result<Option<Item>> {
            Err(pmosource::MusicSourceError::FifoNotSupported)
        }

        async fn update_id(&self) -> u32 {
            0
        }

        async fn last_change(&self) -> Option<SystemTime> {
            None
        }

        async fn get_items(&self, _offset: usize, _count: usize) -> Result<Vec<Item>> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn test_global_registry_singleton() {
        // Vérifier que le registre global est bien un singleton
        let registry1 = get_source_registry().await;
        let registry2 = get_source_registry().await;

        // Les deux références devraient pointer vers le même registre
        assert!(std::ptr::eq(registry1, registry2));
    }
}
