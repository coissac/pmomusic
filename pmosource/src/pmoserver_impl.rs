//! # Implémentation du trait MusicSourceExt pour pmoserver::Server
//!
//! Ce module enrichit `pmoserver::Server` avec les fonctionnalités de gestion
//! de sources musicales en implémentant le trait [`MusicSourceExt`](crate::MusicSourceExt).
//!
//! ## Architecture
//!
//! `pmosource` étend `pmoserver::Server` sans que `pmoserver` connaisse `pmosource`.
//! C'est le pattern d'extension utilisé par tous les crates de l'écosystème PMOMusic.
//!
//! ## Exemple d'utilisation
//!
//! ```rust,no_run
//! use pmosource::MusicSourceExt;
//! use pmoserver::ServerBuilder;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let mut server = ServerBuilder::new("MyApp", "http://localhost:3000", 3000).build();
//!
//! // Initialiser le système de sources (enregistre l'API)
//! server.init_music_sources().await?;
//!
//! // Le trait MusicSourceExt est automatiquement disponible
//! let source = Arc::new(MySource::new());
//! server.register_music_source(source).await;
//!
//! server.start().await;
//! # Ok(())
//! # }
//! ```

#[cfg(feature = "server")]
use crate::api::{register_source, unregister_source, list_all_sources, get_source, create_sources_router, SourcesApiDoc};
#[cfg(feature = "server")]
use crate::pmoserver_ext::MusicSourceExt;
#[cfg(feature = "server")]
use crate::MusicSource;
#[cfg(feature = "server")]
use anyhow::Result;
#[cfg(feature = "server")]
use pmoserver::Server;
#[cfg(feature = "server")]
use std::sync::Arc;
#[cfg(feature = "server")]
use tracing::info;
#[cfg(feature = "server")]
use utoipa::OpenApi;

#[cfg(feature = "server")]
#[async_trait::async_trait]
impl MusicSourceExt for Server {
    async fn init_music_sources(&mut self) -> Result<()> {
        info!("Initializing music sources management system...");

        // Créer le router pour l'API des sources
        let router = create_sources_router();

        // Créer la documentation OpenAPI
        let openapi = SourcesApiDoc::openapi();

        // Enregistrer l'API avec Swagger UI
        // Le router sera nesté automatiquement sous /api/sources par add_openapi
        // Routes finales: /api/sources, /api/sources/{id}, etc.
        // Swagger UI sera disponible à /swagger-ui/sources
        self.add_openapi(router, openapi, "sources").await;

        info!("✅ Music sources API registered at /api/sources");
        info!("   Swagger UI available at /swagger-ui/sources");

        Ok(())
    }

    async fn register_music_source(&mut self, source: Arc<dyn MusicSource>) {
        let source_id = source.id().to_string();
        let source_name = source.name().to_string();

        info!("Registering music source: {} ({})", source_name, source_id);

        register_source(source).await;

        info!("✅ Source '{}' registered successfully", source_name);
    }

    async fn unregister_music_source(&mut self, source_id: &str) -> bool {
        info!("Unregistering music source: {}", source_id);

        let result = unregister_source(source_id).await;

        if result {
            info!("✅ Source '{}' unregistered successfully", source_id);
        } else {
            tracing::warn!("⚠️  Source '{}' not found", source_id);
        }

        result
    }

    async fn list_music_sources(&self) -> Vec<Arc<dyn MusicSource>> {
        list_all_sources().await
    }

    async fn get_music_source(&self, source_id: &str) -> Option<Arc<dyn MusicSource>> {
        get_source(source_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_implemented() {
        // Ce test vérifie simplement que le trait est bien implémenté
        // Les tests fonctionnels nécessiteraient un serveur et des sources réelles
    }
}
