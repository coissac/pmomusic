//! Extension de pmoserver::Server pour intégrer le client Qobuz
//!
//! Ce module fournit un trait d'extension permettant d'ajouter facilement
//! le client Qobuz et ses endpoints à un serveur pmoserver.

use crate::client::QobuzClient;
use anyhow::Result;
use std::sync::Arc;

/// Trait d'extension pour ajouter le support Qobuz à un serveur pmoserver
///
/// Ce trait permet à `pmoqobuz` d'ajouter des méthodes d'extension sur
/// `pmoserver::Server` sans que pmoserver dépende de pmoqobuz.
///
/// # Architecture
///
/// Similaire au pattern utilisé par `pmocovers` avec `CoverCacheExt`, ce trait permet
/// une extension propre et découplée :
///
/// - `pmoserver` définit un serveur HTTP générique
/// - `pmoqobuz` étend ce serveur avec des fonctionnalités Qobuz via ce trait
/// - Le serveur n'a pas besoin de connaître `pmoqobuz`
///
/// # Exemple
///
/// ```rust,no_run
/// use pmoqobuz::QobuzServerExt;
/// use pmoserver::ServerBuilder;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let mut server = ServerBuilder::new_configured().build();
///
///     // Initialise le client Qobuz depuis la config
///     server.init_qobuz_client_configured().await?;
///
///     server.start().await;
///     server.wait().await;
///     Ok(())
/// }
/// ```
pub trait QobuzServerExt {
    /// Initialise le client Qobuz et enregistre les routes HTTP
    ///
    /// # Arguments
    ///
    /// * `username` - Email ou nom d'utilisateur Qobuz
    /// * `password` - Mot de passe
    ///
    /// # Returns
    ///
    /// * `Arc<QobuzClient>` - Instance partagée du client
    ///
    /// # Routes enregistrées
    ///
    /// - `GET /qobuz/albums/{id}` - Détails d'un album
    /// - `GET /qobuz/albums/{id}/tracks` - Tracks d'un album
    /// - `GET /qobuz/tracks/{id}` - Détails d'une track
    /// - `GET /qobuz/tracks/{id}/stream` - URL de streaming
    /// - `GET /qobuz/artists/{id}` - Détails d'un artiste
    /// - `GET /qobuz/artists/{id}/albums` - Albums d'un artiste
    /// - `GET /qobuz/playlists/{id}` - Détails d'une playlist
    /// - `GET /qobuz/playlists/{id}/tracks` - Tracks d'une playlist
    /// - `GET /qobuz/search` - Recherche (query params: q, type)
    /// - `GET /qobuz/favorites/albums` - Albums favoris
    /// - `GET /qobuz/favorites/artists` - Artistes favoris
    /// - `GET /qobuz/favorites/tracks` - Tracks favoris
    /// - `GET /qobuz/favorites/playlists` - Playlists utilisateur
    /// - `GET /qobuz/genres` - Liste des genres
    /// - `GET /qobuz/featured/albums` - Albums featured
    /// - `GET /qobuz/featured/playlists` - Playlists featured
    /// - `GET /qobuz/cache/stats` - Statistiques du cache
    /// - `GET /swagger-ui` - Documentation interactive
    async fn init_qobuz_client(
        &mut self,
        username: &str,
        password: &str,
    ) -> Result<Arc<QobuzClient>>;

    /// Initialise le client Qobuz avec la configuration par défaut
    ///
    /// Utilise automatiquement les credentials de `pmoconfig::Config` :
    /// - `accounts.qobuz.username` pour le nom d'utilisateur
    /// - `accounts.qobuz.password` pour le mot de passe
    ///
    /// # Returns
    ///
    /// * `Arc<QobuzClient>` - Instance partagée du client
    ///
    /// # Exemple
    ///
    /// ```rust,no_run
    /// use pmoqobuz::QobuzServerExt;
    /// use pmoserver::ServerBuilder;
    ///
    /// #[tokio::main]
    /// async fn main() -> anyhow::Result<()> {
    ///     let mut server = ServerBuilder::new_configured().build();
    ///
    ///     // Utilise automatiquement la config
    ///     server.init_qobuz_client_configured().await?;
    ///
    ///     server.start().await;
    ///     Ok(())
    /// }
    /// ```
    async fn init_qobuz_client_configured(&mut self) -> Result<Arc<QobuzClient>>;

    /// Initialise le client Qobuz avec intégration pmocovers
    ///
    /// Les images d'albums seront automatiquement ajoutées au cache pmocovers fourni.
    ///
    /// # Arguments
    ///
    /// * `username` - Email ou nom d'utilisateur Qobuz
    /// * `password` - Mot de passe
    /// * `cover_cache` - Instance du cache pmocovers à utiliser
    ///
    /// # Returns
    ///
    /// * `Arc<QobuzClient>` - Instance partagée du client avec cache d'images
    ///
    /// # Exemple
    ///
    /// ```rust,no_run
    /// use pmoqobuz::QobuzServerExt;
    /// use pmocovers::CoverCacheExt;
    /// use pmoserver::ServerBuilder;
    ///
    /// #[tokio::main]
    /// async fn main() -> anyhow::Result<()> {
    ///     let mut server = ServerBuilder::new_configured().build();
    ///
    ///     // D'abord initialiser le cache d'images
    ///     let cache = server.init_cover_cache_configured().await?;
    ///
    ///     // Puis initialiser Qobuz avec le cache
    ///     server.init_qobuz_client_with_covers("user", "pass", cache).await?;
    ///
    ///     server.start().await;
    ///     Ok(())
    /// }
    /// ```
    #[cfg(feature = "covers")]
    async fn init_qobuz_client_with_covers(
        &mut self,
        username: &str,
        password: &str,
        cover_cache: Arc<pmocovers::Cache>,
    ) -> Result<Arc<QobuzClient>>;

    /// Initialise le client Qobuz avec intégration pmocovers depuis la configuration
    ///
    /// # Arguments
    ///
    /// * `cover_cache` - Instance du cache pmocovers à utiliser
    ///
    /// # Exemple
    ///
    /// ```rust,no_run
    /// use pmoqobuz::QobuzServerExt;
    /// use pmocovers::CoverCacheExt;
    /// use pmoserver::ServerBuilder;
    ///
    /// #[tokio::main]
    /// async fn main() -> anyhow::Result<()> {
    ///     let mut server = ServerBuilder::new_configured().build();
    ///
    ///     // D'abord initialiser le cache
    ///     let cache = server.init_cover_cache_configured().await?;
    ///
    ///     // Puis initialiser Qobuz avec le cache
    ///     server.init_qobuz_client_configured_with_covers(cache).await?;
    ///
    ///     server.start().await;
    ///     Ok(())
    /// }
    /// ```
    #[cfg(feature = "covers")]
    async fn init_qobuz_client_configured_with_covers(
        &mut self,
        cover_cache: Arc<pmocovers::Cache>,
    ) -> Result<Arc<QobuzClient>>;
}

// L'implémentation du trait sera dans un module séparé (pmoserver_impl.rs)
// pour éviter les dépendances circulaires
