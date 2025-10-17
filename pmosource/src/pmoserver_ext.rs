//! # Music Source Extension Trait
//!
//! Ce module définit le trait d'extension [`MusicSourceExt`] qui permet d'ajouter
//! facilement la gestion des sources musicales à un serveur `pmoserver::Server`.
//!
//! ## Architecture
//!
//! Ce trait suit le pattern d'extension utilisé par les autres crates de l'écosystème
//! PMOMusic (`pmocovers`, `pmoaudiocache`, `pmoqobuz`, etc.). Il permet à `pmosource`
//! d'étendre `pmoserver::Server` sans que `pmoserver` ne connaisse `pmosource`.
//!
//! ## Exemple d'utilisation
//!
//! ```rust,ignore
//! use pmosource::{MusicSourceExt, MusicSource};
//! use pmoserver::ServerBuilder;
//! use std::sync::Arc;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let mut server = ServerBuilder::new_configured().build();
//!
//! // Initialiser le gestionnaire de sources avec API
//! server.init_music_sources().await?;
//!
//! // Enregistrer une source
//! let source: Arc<dyn MusicSource> = Arc::new(MySource::new());
//! server.register_music_source(source).await;
//!
//! // Lister les sources
//! let sources = server.list_music_sources().await;
//! println!("{} sources registered", sources.len());
//!
//! server.start().await;
//! # Ok(())
//! # }
//! ```

use crate::MusicSource;
use anyhow::Result;
use std::sync::Arc;

/// Extension trait pour ajouter la gestion des sources musicales à un serveur
///
/// Ce trait étend `pmoserver::Server` avec des fonctionnalités de gestion de sources
/// musicales, incluant :
/// - Enregistrement de sources implémentant [`MusicSource`]
/// - API REST pour lister et gérer les sources
/// - Documentation OpenAPI automatique
/// - Intégration avec le registre global de sources
///
/// # Thread Safety
///
/// Toutes les opérations sont thread-safe et utilisent un registre partagé
/// accessible via `Arc<SourceRegistry>`.
///
/// # Examples
///
/// ```rust,ignore
/// use pmosource::MusicSourceExt;
/// use pmoserver::ServerBuilder;
///
/// let mut server = ServerBuilder::new_configured().build();
///
/// // Initialiser le système de sources (enregistre les routes API)
/// server.init_music_sources().await?;
///
/// // Le serveur est maintenant prêt à accepter des sources
/// ```
#[cfg_attr(feature = "server", async_trait::async_trait)]
pub trait MusicSourceExt {
    /// Initialise le système de gestion des sources musicales
    ///
    /// Cette méthode :
    /// 1. Initialise le registre global de sources
    /// 2. Enregistre les routes API REST (`/api/sources/*`)
    /// 3. Configure la documentation OpenAPI
    ///
    /// Cette méthode doit être appelée avant d'enregistrer des sources.
    ///
    /// # Returns
    ///
    /// `Ok(())` si l'initialisation réussit.
    ///
    /// # Errors
    ///
    /// Retourne une erreur si le système de sources est déjà initialisé
    /// ou si l'enregistrement des routes échoue.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// server.init_music_sources().await?;
    /// ```
    #[cfg(feature = "server")]
    async fn init_music_sources(&mut self) -> Result<()>;

    /// Enregistre une source musicale
    ///
    /// Ajoute une source au registre global, la rendant disponible pour
    /// les clients UPnP et l'API REST.
    ///
    /// # Arguments
    ///
    /// * `source` - La source musicale à enregistrer (implémente [`MusicSource`])
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let source = Arc::new(QobuzSource::new(client, base_url));
    /// server.register_music_source(source).await;
    /// ```
    #[cfg(feature = "server")]
    async fn register_music_source(&mut self, source: Arc<dyn MusicSource>);

    /// Désenregistre une source musicale par son ID
    ///
    /// Retire la source du registre global.
    ///
    /// # Arguments
    ///
    /// * `source_id` - L'ID unique de la source à retirer
    ///
    /// # Returns
    ///
    /// `true` si la source a été trouvée et retirée, `false` sinon.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// if server.unregister_music_source("qobuz").await {
    ///     println!("Qobuz source removed");
    /// }
    /// ```
    #[cfg(feature = "server")]
    async fn unregister_music_source(&mut self, source_id: &str) -> bool;

    /// Liste toutes les sources enregistrées
    ///
    /// Retourne une copie de toutes les sources actuellement enregistrées
    /// dans le registre global.
    ///
    /// # Returns
    ///
    /// Un vecteur de `Arc<dyn MusicSource>` contenant toutes les sources.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let sources = server.list_music_sources().await;
    /// for source in sources {
    ///     println!("- {} ({})", source.name(), source.id());
    /// }
    /// ```
    #[cfg(feature = "server")]
    async fn list_music_sources(&self) -> Vec<Arc<dyn MusicSource>>;

    /// Récupère une source spécifique par son ID
    ///
    /// # Arguments
    ///
    /// * `source_id` - L'ID unique de la source recherchée
    ///
    /// # Returns
    ///
    /// `Some(Arc<dyn MusicSource>)` si la source existe, `None` sinon.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// if let Some(source) = server.get_music_source("qobuz").await {
    ///     println!("Found: {}", source.name());
    /// }
    /// ```
    #[cfg(feature = "server")]
    async fn get_music_source(&self, source_id: &str) -> Option<Arc<dyn MusicSource>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Les tests fonctionnels nécessitent l'implémentation du trait,
    // voir pmoserver_impl.rs
    #[test]
    fn test_trait_exists() {
        // Ce test vérifie simplement que le trait compile
    }
}
