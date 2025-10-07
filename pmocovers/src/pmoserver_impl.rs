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
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use pmoserver::Server;
use tracing::{debug, info, warn};
use std::sync::Arc;
use utoipa::OpenApi;



/// Handler pour GET /covers/images/{pk}
async fn get_cover_image(
    State(cache): State<Arc<Cache>>,
    req: Request<Body>,
) -> Response {
    // Extraire pk du path
    let path = req.uri().path();
    let parts: Vec<&str> = path.split('/').collect();

    warn!("{:?}",parts);

    if parts.len() != 2 {
        return (StatusCode::BAD_REQUEST, "Invalid path").into_response();
    }

    let pk = parts[1];

    match cache.get(pk).await {
        Ok(file_path) => {
            match tokio::fs::read(&file_path).await {
                Ok(data) => (
                    StatusCode::OK,
                    [("content-type", "image/webp")],
                    data,
                )
                    .into_response(),
                Err(_) => (StatusCode::NOT_FOUND, "File not found").into_response(),
            }
        }
        Err(_) => (StatusCode::NOT_FOUND, "Image not found").into_response(),
    }
}

/// Handler pour GET /covers/images/{pk}/{size}
async fn get_cover_variant(
    State(cache): State<Arc<Cache>>,
    req: Request<Body>,
) -> Response {
    // Extraire pk et size du path
    let path = req.uri().path();
    let parts: Vec<&str> = path.split('/').collect();

    if parts.len() != 3 {
        return (StatusCode::BAD_REQUEST, "Invalid path").into_response();
    }

    let pk = parts[1];
    let size = match parts[2].parse::<usize>() {
        Ok(s) => s,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid size").into_response(),
    };

    match crate::webp::generate_variant(&cache, pk, size).await {
        Ok(data) => (
            StatusCode::OK,
            [("content-type", "image/webp")],
            data,
        )
            .into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Cannot generate variant").into_response(),
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
        let cache = Arc::new(Cache::new(cache_dir, limit)?);

        // Enregistrer les routes HTTP classiques pour servir les images
        let image_router = Router::new()
            .route("/{pk}", get(get_cover_image))
            .route("/{pk}/{size}", get(get_cover_variant))
            .with_state(cache.clone());

        self.add_router("/covers/images", image_router).await;
        self.add_handler_with_state("/covers/stats", get_cover_stats, cache.clone()).await;

        // Router API RESTful
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
