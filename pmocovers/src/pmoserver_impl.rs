//! Implémentation du trait CoverCacheExt pour le serveur pmoserver
//!
//! Ce module enrichit `pmoserver::Server` avec les fonctionnalités de cache d'images en
//! implémentant le trait [`CoverCacheExt`](crate::CoverCacheExt). Cette implémentation
//! permet d'initialiser facilement le cache et d'enregistrer les routes HTTP.
//!
//! ## Architecture
//!
//! `pmocovers` étend `pmoserver::Server` sans que `pmoserver` connaisse `pmocovers`.
//! C'est le pattern d'extension : `pmocovers` ajoute des fonctionnalités à un type
//! externe via un trait, similaire au pattern utilisé par `pmoapp` pour `WebAppExt`.
//!
//! ## Exemple d'utilisation
//!
//! ```rust,no_run
//! use pmocovers::CoverCacheExt;
//! use pmoserver::ServerBuilder;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let mut server = ServerBuilder::new("MyApp", "http://localhost:3000", 3000).build();
//!
//! // Le trait CoverCacheExt est automatiquement disponible
//! let cache = server.init_cover_cache("./cache", 1000).await?;
//!
//! server.start().await;
//! # Ok(())
//! # }
//! ```

use crate::{api, Cache, CoverCacheExt};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use pmoserver::Server;
use tracing::{info, warn};
use std::sync::Arc;
use utoipa::OpenApi;

/// Handler pour GET /covers/images/{pk}/{size}
/// Génère une variante d'image à la demande
async fn get_cover_variant(
    State(cache): State<Arc<Cache>>,
    Path((pk, size)): Path<(String, String)>,
) -> Response {
    let size = match size.parse::<usize>() {
        Ok(s) => s,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid size").into_response(),
    };

    match crate::webp::generate_variant(&cache, &pk, size).await {
        Ok(data) => (
            StatusCode::OK,
            [("content-type", "image/webp")],
            data,
        )
            .into_response(),
        Err(e) => {
            warn!("Cannot generate variant for {}: {}", pk, e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Cannot generate variant").into_response()
        }
    }
}

/// Handler pour GET /covers/stats
async fn get_cover_stats(State(cache): State<Arc<Cache>>) -> Response {
    match cache.db.get_all() {
        Ok(entries) => Json(entries).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Cannot retrieve stats").into_response(),
    }
}

impl CoverCacheExt for Server {
    async fn init_cover_cache(&mut self, cache_dir: &str, limit: usize) -> anyhow::Result<Arc<Cache>> {
        // Utiliser l'URL du serveur comme base_url
        let base_url = self.info().base_url;
        let cache = Arc::new(Cache::new(cache_dir, limit, &base_url)?);

        // Utiliser le router générique de pmocache pour servir les fichiers
        // Routes: GET /covers/images/{pk} et GET /covers/images/{pk}/{param}
        let file_router = pmocache::pmoserver_ext::create_file_router(
            cache.clone(),
            "image/webp"
        );
        self.add_router("/covers/images", file_router).await;

        // Route pour générer les variantes à la demande (redimensionnement)
        // Note: Cette route est spécifique à pmocovers car elle nécessite generate_variant
        let variant_router = Router::new()
            .route("/{pk}/{size}", get(get_cover_variant))
            .with_state(cache.clone());
        self.add_router("/covers/variants", variant_router).await;

        // Route pour les stats
        self.add_handler_with_state("/covers/stats", get_cover_stats, cache.clone()).await;

        // Router API RESTful qui sera nesté sous /api/covers par add_openapi
        let api_router = Router::new()
            // Liste et ajout
            .route(
                "/",
                get(api::list_images)       // GET /api/covers
                    .post(api::add_image)   // POST /api/covers
                    .delete(api::purge_cache), // DELETE /api/covers
            )
            // Ressource unique
            .route(
                "/{pk}",
                get(api::get_image_info)      // GET /api/covers/{pk}
                    .delete(api::delete_image), // DELETE /api/covers/{pk}
            )
            // Action spécifique
            .route(
                "/consolidate",
                post(api::consolidate_cache), // POST /api/covers/consolidate
            )
            .with_state(cache.clone());

        // Documentation OpenAPI via Utoipa
        let openapi = crate::ApiDoc::openapi();

        // Enregistrer l'API avec Swagger UI
        // Le router sera nesté automatiquement sous /api/covers par add_openapi
        // Routes finales: /api/covers, /api/covers/{pk}, /api/covers/consolidate
        // Swagger UI sera disponible à /swagger-ui/covers
        self.add_openapi(api_router, openapi, "covers").await;

        Ok(cache)
    }

    async fn init_cover_cache_configured(&mut self) -> anyhow::Result<Arc<Cache>> {
        let config = pmoconfig::get_config();

        let cache_dir = config.get_cover_cache_dir()?;
        let limit = config.get_cover_cache_size()?;

        info!("cache directory {}, size {}",cache_dir,limit);

        self.init_cover_cache(&cache_dir, limit).await
    }
}
