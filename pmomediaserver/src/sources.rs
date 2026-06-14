//! # Source Helpers - Helpers pour l'initialisation simplifiée de sources
//!
//! Ce module fournit des helpers pour créer et enregistrer facilement des sources
//! musicales préconfigurées à partir de la configuration système.

use crate::contentdirectory::state;
use pmoserver::Server;
use pmosource::MusicSourceExt;
use std::sync::Arc;

/// Erreur lors de l'initialisation d'une source
#[derive(Debug, thiserror::Error)]
pub enum SourceInitError {
    #[cfg(feature = "qobuz")]
    #[error("Failed to initialize Qobuz: {0}")]
    QobuzError(String),

    #[cfg(feature = "paradise")]
    #[error("Failed to initialize Radio Paradise: {0}")]
    ParadiseError(String),

    #[cfg(feature = "radiofrance")]
    #[error("Failed to initialize Radio France: {0}")]
    RadioFranceError(String),

    #[cfg(feature = "urlsource")]
    #[error("Failed to initialize URL source: {0}")]
    UrlSourceError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Source not available: {0}")]
    NotAvailable(String),
}

/// Result type pour les opérations d'initialisation de sources
pub type Result<T> = std::result::Result<T, SourceInitError>;

/// Extension trait pour faciliter l'enregistrement de sources préconfigurées
///
/// Ce trait ajoute des méthodes pratiques à `Server` pour enregistrer des sources
/// musicales préconfigurées à partir de la configuration système.
///
/// # Examples
///
/// ```ignore
/// use pmomediaserver::sources::SourcesExt;
/// use pmoserver::ServerBuilder;
///
/// let mut server = ServerBuilder::new_configured().build();
///
/// // Enregistrer Qobuz depuis la config
/// server.register_qobuz_from_config().await?;
///
/// // Lister toutes les sources
/// let sources = server.list_music_sources().await;
/// println!("{} sources registered", sources.len());
/// ```
#[async_trait::async_trait]
pub trait SourcesExt {
    /// Enregistre la source Qobuz
    ///
    /// Cette méthode lit les credentials Qobuz depuis `pmoconfig` et crée
    /// automatiquement un `QobuzSource` avec cache activé.
    ///
    /// # Configuration requise
    ///
    /// Le fichier de configuration doit contenir :
    /// ```yaml
    /// accounts:
    ///   qobuz:
    ///     username: "votre@email.com"
    ///     password: "votrepassword"
    /// ```
    ///
    /// # Erreurs
    ///
    /// Retourne une erreur si :
    /// - La configuration Qobuz n'est pas trouvée
    /// - L'authentification échoue
    /// - La feature "qobuz" n'est pas activée
    ///
    /// # Examples
    ///
    /// ```ignore
    /// server.register_qobuz().await?;
    /// ```
    #[cfg(feature = "qobuz")]
    async fn register_qobuz(&mut self) -> Result<()>;

    /// Enregistre la source Qobuz avec des credentials explicites
    ///
    /// # Arguments
    ///
    /// * `username` - Nom d'utilisateur Qobuz
    /// * `password` - Mot de passe Qobuz
    ///
    /// # Examples
    ///
    /// ```ignore
    /// server.register_qobuz_with_credentials("user@example.com", "password").await?;
    /// ```
    #[cfg(feature = "qobuz")]
    async fn register_qobuz_with_credentials(
        &mut self,
        username: &str,
        password: &str,
    ) -> Result<()>;

    /// Enregistre la source Radio Paradise
    ///
    /// Cette méthode crée automatiquement un `RadioParadiseSource` avec cache activé.
    /// Radio Paradise ne nécessite pas d'authentification.
    ///
    /// # Erreurs
    ///
    /// Retourne une erreur si :
    /// - La connexion au client Radio Paradise échoue
    /// - La feature "paradise" n'est pas activée
    ///
    /// # Examples
    ///
    /// ```ignore
    /// server.register_paradise().await?;
    /// ```
    #[cfg(feature = "paradise")]
    async fn register_paradise(&mut self) -> Result<()>;

    /// Enregistre la source Radio France
    ///
    /// Cette méthode crée automatiquement un `RadioFranceSource` avec cache activé.
    /// Radio France ne nécessite pas d'authentification.
    ///
    /// # Erreurs
    ///
    /// Retourne une erreur si :
    /// - La connexion au client Radio France échoue
    /// - La feature "radiofrance" n'est pas activée
    ///
    /// # Examples
    ///
    /// ```ignore
    /// server.register_radiofrance().await?;
    /// ```
    #[cfg(feature = "radiofrance")]
    async fn register_radiofrance(&mut self) -> Result<()>;

    /// Enregistre la source URL / Partage
    ///
    /// Cette source permet de coller n'importe quelle URL (lien de partage Qobuz,
    /// flux audio, playlist M3U…) dans la barre de recherche et de lancer la lecture
    /// directement. Aucune authentification requise.
    #[cfg(feature = "urlsource")]
    async fn register_urlsource(&mut self) -> Result<()>;
}

#[async_trait::async_trait]
impl SourcesExt for Server {
    #[cfg(feature = "qobuz")]
    async fn register_qobuz(&mut self) -> Result<()> {
        use pmoqobuz::{QobuzClient, QobuzSource};

        tracing::info!("Initializing Qobuz source...");

        // Obtenir l'URL de base du serveur
        let base_url = self.base_url();

        // Créer le client depuis la config
        let client = QobuzClient::from_config()
            .await
            .map_err(|e| SourceInitError::QobuzError(format!("Failed to create client: {}", e)))?;

        // Créer la source depuis le registry
        let source = QobuzSource::from_registry(client, base_url)
            .map_err(|e| SourceInitError::QobuzError(format!("Failed to create source: {}", e)))?;

        // Enregistrer la source
        self.register_music_source(Arc::new(source)).await;

        tracing::info!("✅ Qobuz source registered successfully");

        Ok(())
    }

    #[cfg(feature = "qobuz")]
    async fn register_qobuz_with_credentials(
        &mut self,
        username: &str,
        password: &str,
    ) -> Result<()> {
        use pmoqobuz::{QobuzClient, QobuzSource};

        tracing::info!("Initializing Qobuz source with explicit credentials...");

        // Obtenir l'URL de base du serveur
        let base_url = self.base_url();

        // Créer le client avec credentials
        let client = QobuzClient::new(username, password)
            .await
            .map_err(|e| SourceInitError::QobuzError(format!("Failed to authenticate: {}", e)))?;

        // Créer la source depuis le registry
        let source = QobuzSource::from_registry(client, base_url)
            .map_err(|e| SourceInitError::QobuzError(format!("Failed to create source: {}", e)))?;

        // Enregistrer la source
        self.register_music_source(Arc::new(source)).await;

        tracing::info!("✅ Qobuz source registered successfully");

        Ok(())
    }

    #[cfg(feature = "paradise")]
    async fn register_paradise(&mut self) -> Result<()> {
        use crate::contentdirectory::state;
        use pmoparadise::{RadioParadiseExt, RadioParadiseSource};

        tracing::info!("Initializing Radio Paradise source...");

        // Obtenir l'URL de base du serveur
        let base_url = self.base_url();

        // Créer la source Radio Paradise (utilise le singleton PlaylistManager)
        let notifier = Arc::new(|containers: &[String]| {
            let refs: Vec<&str> = containers.iter().map(|s| s.as_str()).collect();
            state::notify_containers_updated(&refs);
        });
        let source = Arc::new(
            RadioParadiseSource::new(base_url.to_string()).with_container_notifier(notifier),
        );

        // Brancher les callbacks de playlists (live/history) pour signaler les updates
        source.attach_playlist_callbacks();

        // Enregistrer la source
        self.register_music_source(source.clone()).await;

        tracing::info!("✅ Radio Paradise source registered successfully");

        // Initialiser l'API REST Radio Paradise
        #[cfg(feature = "paradise-api")]
        {
            tracing::info!("📻 Initializing Radio Paradise API...");
            if let Err(e) = self.init_radioparadise().await {
                tracing::warn!("⚠️ Failed to initialize Radio Paradise API: {}", e);
            } else {
                tracing::info!("✅ Radio Paradise API initialized");
            }
        }

        Ok(())
    }

    #[cfg(feature = "radiofrance")]
    async fn register_radiofrance(&mut self) -> Result<()> {
        use pmoradiofrance::{RadioFranceExt, RadioFranceSource};

        tracing::info!("Initializing Radio France source...");

        // Obtenir l'URL de base du serveur
        let base_url = self.base_url();

        // Créer la source depuis le registry (avec cache)
        let config = pmoconfig::get_config();
        let source = RadioFranceSource::from_registry(config, base_url)
            .await
            .map_err(|e| {
                SourceInitError::RadioFranceError(format!("Failed to create source: {}", e))
            })?;

        // Configurer le notifier pour les événements UPnP GENA
        let notifier = Arc::new(|containers: &[String]| {
            let refs: Vec<&str> = containers.iter().map(|s| s.as_str()).collect();
            state::notify_containers_updated(&refs);
        });
        let source = source.with_container_notifier(notifier);

        // Enregistrer la source (Arc pour partage avec l'API)
        let source_arc = Arc::new(source);
        self.register_music_source(source_arc.clone()).await;

        // Initialiser les routes API Radio France avec la source
        self.init_radiofrance_with_source(source_arc)
            .await
            .map_err(|e| {
                SourceInitError::RadioFranceError(format!("Failed to init API routes: {}", e))
            })?;

        tracing::info!("✅ Radio France source registered successfully");

        Ok(())
    }

    #[cfg(feature = "urlsource")]
    async fn register_urlsource(&mut self) -> Result<()> {
        use pmourlsource::{GenericUrlHandler, QobuzUrlHandler, RadioFranceUrlHandler, UrlResolver, UrlSource};

        tracing::info!("Initializing URL source...");

        let mut resolver = UrlResolver::new();
        // Handlers spécialisés (priorité haute) — résolution sans I/O ou API dédiée
        resolver.register(Box::new(QobuzUrlHandler::new()));
        match RadioFranceUrlHandler::new() {
            Ok(h) => resolver.register(Box::new(h)),
            Err(e) => tracing::warn!("Failed to build RadioFranceUrlHandler HTTP client: {}", e),
        }
        // Handler générique (priorité basse) — HTTP GET + scraping HTML/RSS
        match GenericUrlHandler::new() {
            Ok(h) => resolver.register(Box::new(h)),
            Err(e) => tracing::warn!("Failed to build GenericUrlHandler HTTP client: {}", e),
        }

        let source = Arc::new(UrlSource::new(resolver));
        self.register_music_source(source).await;

        tracing::info!("✅ URL source registered successfully");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_init_error() {
        #[cfg(feature = "qobuz")]
        {
            let err = SourceInitError::QobuzError("test error".to_string());
            assert!(err.to_string().contains("Qobuz"));
        }

        let err = SourceInitError::ConfigError("test".to_string());
        assert!(err.to_string().contains("Configuration"));
    }
}
