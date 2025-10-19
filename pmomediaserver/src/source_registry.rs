//! # Source Registry - Gestionnaire de sources musicales
//!
//! Ce module fournit un registre centralisé pour gérer les différentes sources musicales
//! (MusicSource) qui peuvent être diffusées par le MediaServer.
//!
//! ## Fonctionnalités
//!
//! - **Enregistrement de sources** : Ajout de sources musicales au registre
//! - **Accès aux sources** : Récupération des sources enregistrées par ID
//! - **Navigation multi-sources** : Combine les sources dans une hiérarchie unique
//! - **Thread-safe** : Utilise Arc et RwLock pour un accès concurrent

use pmosource::MusicSource;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Registre des sources musicales
///
/// Ce registre maintient une liste de toutes les sources musicales enregistrées
/// et permet de les récupérer par leur ID unique.
///
/// # Thread Safety
///
/// Le registre utilise `Arc<RwLock<...>>` pour permettre un accès concurrent sécurisé.
/// Plusieurs lecteurs peuvent accéder simultanément aux sources, mais l'enregistrement
/// de nouvelles sources nécessite un verrou exclusif.
///
/// # Examples
///
/// ```ignore
/// use pmomediaserver::source_registry::SourceRegistry;
///
/// let registry = SourceRegistry::new();
///
/// // Enregistrer une source
/// let source = Arc::new(MyMusicSource::new());
/// registry.register(source).await;
///
/// // Récupérer une source
/// if let Some(source) = registry.get("my-source-id").await {
///     let root = source.root_container().await?;
/// }
/// ```
#[derive(Clone)]
pub struct SourceRegistry {
    sources: Arc<RwLock<HashMap<String, Arc<dyn MusicSource>>>>,
}

impl SourceRegistry {
    /// Crée un nouveau registre vide
    ///
    /// # Examples
    ///
    /// ```
    /// use pmomediaserver::source_registry::SourceRegistry;
    ///
    /// let registry = SourceRegistry::new();
    /// ```
    pub fn new() -> Self {
        Self {
            sources: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Enregistre une nouvelle source musicale
    ///
    /// La source est identifiée par son ID unique (retourné par `source.id()`).
    /// Si une source avec le même ID existe déjà, elle sera remplacée.
    ///
    /// # Arguments
    ///
    /// * `source` - La source musicale à enregistrer (doit implémenter `MusicSource`)
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let source = Arc::new(RadioParadise::new());
    /// registry.register(source).await;
    /// ```
    pub async fn register(&self, source: Arc<dyn MusicSource>) {
        let id = source.id().to_string();
        let mut sources = self.sources.write().await;

        tracing::info!(
            source_id = %id,
            source_name = %source.name(),
            "Registering music source"
        );

        sources.insert(id, source);
    }

    /// Récupère une source par son ID
    ///
    /// # Arguments
    ///
    /// * `id` - L'ID unique de la source
    ///
    /// # Returns
    ///
    /// Un `Arc` vers la source si elle existe, ou `None` si aucune source avec cet ID
    /// n'est enregistrée.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// if let Some(source) = registry.get("radio-paradise").await {
    ///     println!("Found: {}", source.name());
    /// }
    /// ```
    pub async fn get(&self, id: &str) -> Option<Arc<dyn MusicSource>> {
        let sources = self.sources.read().await;
        sources.get(id).cloned()
    }

    /// Liste toutes les sources enregistrées
    ///
    /// # Returns
    ///
    /// Un vecteur contenant des clones de toutes les sources enregistrées.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let all_sources = registry.list_all().await;
    /// for source in all_sources {
    ///     println!("Source: {} ({})", source.name(), source.id());
    /// }
    /// ```
    pub async fn list_all(&self) -> Vec<Arc<dyn MusicSource>> {
        let sources = self.sources.read().await;
        sources.values().cloned().collect()
    }

    /// Compte le nombre de sources enregistrées
    ///
    /// # Returns
    ///
    /// Le nombre total de sources dans le registre.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let count = registry.count().await;
    /// println!("Total sources: {}", count);
    /// ```
    pub async fn count(&self) -> usize {
        let sources = self.sources.read().await;
        sources.len()
    }

    /// Supprime une source du registre
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
    /// if registry.remove("old-source").await {
    ///     println!("Source removed");
    /// }
    /// ```
    pub async fn remove(&self, id: &str) -> bool {
        let mut sources = self.sources.write().await;

        if sources.remove(id).is_some() {
            tracing::info!(source_id = %id, "Removed music source");
            true
        } else {
            false
        }
    }

    /// Vérifie si une source est enregistrée
    ///
    /// # Arguments
    ///
    /// * `id` - L'ID de la source à vérifier
    ///
    /// # Returns
    ///
    /// `true` si la source existe, `false` sinon.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// if registry.contains("qobuz").await {
    ///     // La source Qobuz est disponible
    /// }
    /// ```
    pub async fn contains(&self, id: &str) -> bool {
        let sources = self.sources.read().await;
        sources.contains_key(id)
    }
}

impl Default for SourceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pmodidl::{Container, Item};
    use pmosource::{BrowseResult, MusicSource, Result};
    use std::time::SystemTime;

    #[derive(Debug)]
    struct TestSource {
        id: String,
        name: String,
    }

    impl TestSource {
        fn new(id: &str, name: &str) -> Self {
            Self {
                id: id.to_string(),
                name: name.to_string(),
            }
        }
    }

    #[async_trait::async_trait]
    impl MusicSource for TestSource {
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
    async fn test_register_and_get() {
        let registry = SourceRegistry::new();
        let source = Arc::new(TestSource::new("test-1", "Test Source 1"));

        registry.register(source.clone()).await;

        let retrieved = registry.get("test-1").await;
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id(), "test-1");
        assert_eq!(retrieved.name(), "Test Source 1");
    }

    #[tokio::test]
    async fn test_list_all() {
        let registry = SourceRegistry::new();

        registry
            .register(Arc::new(TestSource::new("test-1", "Test 1")))
            .await;
        registry
            .register(Arc::new(TestSource::new("test-2", "Test 2")))
            .await;
        registry
            .register(Arc::new(TestSource::new("test-3", "Test 3")))
            .await;

        let sources = registry.list_all().await;
        assert_eq!(sources.len(), 3);
    }

    #[tokio::test]
    async fn test_count() {
        let registry = SourceRegistry::new();
        assert_eq!(registry.count().await, 0);

        registry
            .register(Arc::new(TestSource::new("test-1", "Test 1")))
            .await;
        assert_eq!(registry.count().await, 1);

        registry
            .register(Arc::new(TestSource::new("test-2", "Test 2")))
            .await;
        assert_eq!(registry.count().await, 2);
    }

    #[tokio::test]
    async fn test_remove() {
        let registry = SourceRegistry::new();
        registry
            .register(Arc::new(TestSource::new("test-1", "Test 1")))
            .await;

        assert!(registry.contains("test-1").await);
        assert!(registry.remove("test-1").await);
        assert!(!registry.contains("test-1").await);
        assert!(!registry.remove("test-1").await);
    }

    #[tokio::test]
    async fn test_contains() {
        let registry = SourceRegistry::new();

        assert!(!registry.contains("test-1").await);

        registry
            .register(Arc::new(TestSource::new("test-1", "Test 1")))
            .await;

        assert!(registry.contains("test-1").await);
        assert!(!registry.contains("test-2").await);
    }

    #[tokio::test]
    async fn test_replace_source() {
        let registry = SourceRegistry::new();

        registry
            .register(Arc::new(TestSource::new("test-1", "Old Name")))
            .await;
        let old = registry.get("test-1").await.unwrap();
        assert_eq!(old.name(), "Old Name");

        registry
            .register(Arc::new(TestSource::new("test-1", "New Name")))
            .await;
        let new = registry.get("test-1").await.unwrap();
        assert_eq!(new.name(), "New Name");

        assert_eq!(registry.count().await, 1);
    }
}
