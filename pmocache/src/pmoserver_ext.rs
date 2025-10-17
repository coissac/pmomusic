//! Extension pmoserver pour servir les fichiers du cache via HTTP
//!
//! Ce module fournit des handlers génériques pour servir les fichiers
//! d'un cache via des routes HTTP structurées.
//!
//! ## Routes générées
//!
//! Format: `/{name}/{type}/{pk}[/{param}]`
//!
//! Exemples:
//! - `/covers/images/abc123` - Image avec param par défaut (orig)
//! - `/covers/images/abc123/thumb` - Image avec param spécifique
//! - `/audio/tracks/def456/stream` - Piste audio
//!
//! ## Utilisation
//!
//! ```rust,no_run
//! use pmocache::pmoserver_ext;
//! use axum::Router;
//!
//! # async fn example(cache: std::sync::Arc<pmocache::Cache<CoversConfig>>) {
//! // Créer un router pour servir les fichiers
//! let router = pmoserver_ext::create_file_router(
//!     cache.clone(),
//!     "image/webp"  // Content-Type
//! );
//!
//! // Le router peut être monté sur n'importe quel chemin
//! // Exemple: /covers/images -> GET /covers/images/{pk}
//! //                         -> GET /covers/images/{pk}/{param}
//! # }
//! ```

#[cfg(feature = "pmoserver")]
use crate::{Cache, CacheConfig};
#[cfg(feature = "pmoserver")]
use axum::{
    body::Body,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
#[cfg(feature = "pmoserver")]
use std::sync::Arc;
#[cfg(feature = "pmoserver")]
use tracing::warn;

/// Handler générique pour GET /{pk}
/// Sert un fichier avec le param par défaut
#[cfg(feature = "pmoserver")]
async fn get_file<C: CacheConfig + 'static>(
    State((cache, content_type)): State<(Arc<Cache<C>>, &'static str)>,
    Path(pk): Path<String>,
) -> Response {
    match cache.get(&pk).await {
        Ok(file_path) => match tokio::fs::read(&file_path).await {
            Ok(data) => (
                StatusCode::OK,
                [("content-type", content_type)],
                data,
            )
                .into_response(),
            Err(_) => (StatusCode::NOT_FOUND, "File not found").into_response(),
        },
        Err(e) => {
            warn!("Error getting file {}: {}", pk, e);
            (StatusCode::NOT_FOUND, "Item not found").into_response()
        }
    }
}

/// Handler générique pour GET /{pk}/{param}
/// Sert un fichier avec un param spécifique
#[cfg(feature = "pmoserver")]
async fn get_file_with_param<C: CacheConfig + 'static>(
    State((cache, content_type)): State<(Arc<Cache<C>>, &'static str)>,
    Path((pk, param)): Path<(String, String)>,
) -> Response {
    let file_path = cache.file_path_with_qualifier(&pk, &param);

    if !file_path.exists() {
        warn!("File not found: {:?}", file_path);
        return (StatusCode::NOT_FOUND, "File not found").into_response();
    }

    // Mettre à jour les stats d'utilisation
    if let Err(e) = cache.db.update_hit(&pk) {
        warn!("Error updating hit count for {}: {}", pk, e);
    }

    match tokio::fs::read(&file_path).await {
        Ok(data) => (
            StatusCode::OK,
            [("content-type", content_type)],
            data,
        )
            .into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "File not found").into_response(),
    }
}

/// Crée un router pour servir les fichiers d'un cache
///
/// # Arguments
///
/// * `cache` - Instance du cache
/// * `content_type` - Type MIME des fichiers (ex: "image/webp", "audio/flac")
///
/// # Routes créées
///
/// - `GET /{pk}` - Fichier avec param par défaut
/// - `GET /{pk}/{param}` - Fichier avec param spécifique
///
/// # Exemple
///
/// ```rust,no_run
/// use pmocache::pmoserver_ext;
/// use axum::Router;
/// use pmoserver::Server;
///
/// # async fn example(server: &mut Server, cache: std::sync::Arc<pmocache::Cache<CoversConfig>>) {
/// let router = pmoserver_ext::create_file_router(
///     cache.clone(),
///     "image/webp"
/// );
///
/// // Monter le router sur /covers/images
/// server.add_router("/covers/images", router).await;
/// # }
/// ```
#[cfg(feature = "pmoserver")]
pub fn create_file_router<C: CacheConfig + 'static>(
    cache: Arc<Cache<C>>,
    content_type: &'static str,
) -> Router {
    Router::new()
        .route("/:pk", get(get_file::<C>))
        .route("/:pk/:param", get(get_file_with_param::<C>))
        .with_state((cache, content_type))
}
