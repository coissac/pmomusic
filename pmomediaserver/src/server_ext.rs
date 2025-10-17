//! # Extension trait pour le serveur MediaServer
//!
//! Ce module fournit un trait d'extension pour `pmoserver::Server` permettant
//! d'enregistrer facilement des sources musicales et de configurer le MediaServer.
//!
//! **Note**: Ce module réexporte `MusicSourceExt` de `pmosource` et ajoute des
//! méthodes spécifiques au MediaServer UPnP.

use async_trait::async_trait;
use pmosource::MusicSource;
use pmoserver::Server;
use std::sync::Arc;

// Réexporter le trait de base de pmosource
pub use pmosource::MusicSourceExt;

/// Récupère le registre global de sources (délègue à pmosource)
///
/// # Examples
///
/// ```ignore
/// use pmomediaserver::server_ext::get_source_registry;
///
/// let sources = pmosource::api::list_all_sources().await;
/// ```
#[deprecated(since = "0.2.0", note = "Use pmosource::api::list_all_sources() directly")]
pub async fn get_source_registry() -> Vec<Arc<dyn MusicSource>> {
    pmosource::api::list_all_sources().await
}

/// Trait d'extension pour le serveur MediaServer UPnP
///
/// Ce trait ajoute des méthodes spécifiques au MediaServer UPnP.
/// Pour l'enregistrement de sources, utilisez le trait `MusicSourceExt` de `pmosource`.
///
/// **Note**: Ce trait est maintenant deprecated. Utilisez directement `MusicSourceExt`
/// de `pmosource` pour l'enregistrement et la gestion des sources.
///
/// # Migration
///
/// Ancien code :
/// ```ignore
/// use pmomediaserver::server_ext::MediaServerExt;
/// server.register_music_source(source).await;
/// ```
///
/// Nouveau code :
/// ```ignore
/// use pmosource::MusicSourceExt;
/// server.register_music_source(source).await;
/// ```
#[async_trait]
pub trait MediaServerExt {
    /// Compte le nombre de sources musicales enregistrées
    ///
    /// # Returns
    ///
    /// Le nombre total de sources.
    async fn count_music_sources(&self) -> usize {
        pmosource::api::list_all_sources().await.len()
    }
}

#[async_trait]
impl MediaServerExt for Server {
    // Implementation par défaut fournie dans le trait
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_exists() {
        // Ce test vérifie simplement que le module compile
        // Les tests fonctionnels sont maintenant dans pmosource
    }
}
