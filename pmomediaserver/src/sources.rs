//! # Source Helpers - Helpers pour l'initialisation simplifiée de sources
//!
//! Ce module fournit des helpers pour créer et enregistrer facilement des sources
//! musicales préconfigurées à partir de la configuration système.

use crate::server_ext::MediaServerExt;
use pmoserver::Server;
use std::sync::Arc;

/// Erreur lors de l'initialisation d'une source
#[derive(Debug, thiserror::Error)]
pub enum SourceInitError {
    #[cfg(feature = "qobuz")]
    #[error("Failed to initialize Qobuz: {0}")]
    QobuzError(String),

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
    /// Enregistre la source Qobuz depuis la configuration
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
    /// server.register_qobuz_from_config().await?;
    /// ```
    #[cfg(feature = "qobuz")]
    async fn register_qobuz_from_config(&mut self) -> Result<()>;

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
    /// server.register_qobuz("user@example.com", "password").await?;
    /// ```
    #[cfg(feature = "qobuz")]
    async fn register_qobuz(&mut self, username: &str, password: &str) -> Result<()>;
}

#[async_trait::async_trait]
impl SourcesExt for Server {
    #[cfg(feature = "qobuz")]
    async fn register_qobuz_from_config(&mut self) -> Result<()> {
        use pmoqobuz::{QobuzClient, QobuzSource};

        tracing::info!("Initializing Qobuz source from configuration...");

        // Créer le client depuis la config
        let client = QobuzClient::from_config()
            .await
            .map_err(|e| SourceInitError::QobuzError(format!("Failed to create client: {}", e)))?;

        // Récupérer l'URL de base du serveur depuis la config
        let config = pmoconfig::get_config();
        let port = config.get_http_port();
        let base_url = format!("http://localhost:{}", port);

        // Créer la source
        let source = QobuzSource::new(client, &base_url);

        // Enregistrer la source
        self.register_music_source(Arc::new(source)).await;

        tracing::info!("✅ Qobuz source registered successfully");

        Ok(())
    }

    #[cfg(feature = "qobuz")]
    async fn register_qobuz(&mut self, username: &str, password: &str) -> Result<()> {
        use pmoqobuz::{QobuzClient, QobuzSource};

        tracing::info!("Initializing Qobuz source with explicit credentials...");

        // Créer le client avec credentials
        let client = QobuzClient::new(username, password)
            .await
            .map_err(|e| SourceInitError::QobuzError(format!("Failed to authenticate: {}", e)))?;

        // Récupérer l'URL de base du serveur depuis la config
        let config = pmoconfig::get_config();
        let port = config.get_http_port();
        let base_url = format!("http://localhost:{}", port);

        // Créer la source
        let source = QobuzSource::new(client, &base_url);

        // Enregistrer la source
        self.register_music_source(Arc::new(source)).await;

        tracing::info!("✅ Qobuz source registered successfully");

        Ok(())
    }
}

// Placeholder pour d'autres sources
// TODO: Ajouter Radio Paradise lorsque la crate sera disponible
// #[cfg(feature = "radioparadise")]
// async fn register_radioparadise_from_config(&mut self) -> Result<()> { ... }

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
