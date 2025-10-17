//! Implémentation du trait AudioCacheExt pour le serveur pmoserver
use crate::{api, AudioCache, AudioCacheExt};

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use pmoserver::Server;
use std::sync::Arc;
use tracing::{info, warn};
use utoipa::OpenApi;

/// Handler pour GET /audio/tracks/{pk}/stream
/// Sert le fichier FLAC (attend la conversion si nécessaire)
async fn stream_audio(State(cache): State<Arc<AudioCache>>, req: Request<Body>) -> Response {
    let path = req.uri().path();
    let parts: Vec<&str> = path.split('/').collect();

    if parts.len() < 2 {
        return (StatusCode::BAD_REQUEST, "Invalid path").into_response();
    }

    let pk = parts[parts.len() - 2]; // Avant /stream

    match cache.get_file(pk).await {
        Ok(file_path) => match tokio::fs::read(&file_path).await {
            Ok(data) => (
                StatusCode::OK,
                [
                    ("content-type", "audio/flac"),
                    ("accept-ranges", "bytes"),
                ],
                data,
            )
                .into_response(),
            Err(_) => (StatusCode::NOT_FOUND, "File not found").into_response(),
        },
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not completed") {
                (StatusCode::ACCEPTED, "Conversion in progress").into_response()
            } else {
                (StatusCode::NOT_FOUND, msg).into_response()
            }
        }
    }
}

/// Handler pour GET /audio/tracks/{pk}/metadata
/// Retourne les métadonnées immédiatement (même pendant conversion)
async fn get_metadata(State(cache): State<Arc<AudioCache>>, req: Request<Body>) -> Response {
    let path = req.uri().path();
    let parts: Vec<&str> = path.split('/').collect();

    if parts.len() < 2 {
        return (StatusCode::BAD_REQUEST, "Invalid path").into_response();
    }

    let pk = parts[parts.len() - 2]; // Avant /metadata

    match cache.get_metadata(pk).await {
        Ok(metadata) => Json(metadata).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "Metadata not found").into_response(),
    }
}

/// Handler pour GET /audio/tracks/{pk}/didl
/// Retourne le DIDL-Lite XML immédiatement (même pendant conversion)
async fn get_didl(State(cache): State<Arc<AudioCache>>, req: Request<Body>) -> Response {
    let path = req.uri().path();
    let parts: Vec<&str> = path.split('/').collect();

    if parts.len() < 2 {
        return (StatusCode::BAD_REQUEST, "Invalid path").into_response();
    }

    let pk = parts[parts.len() - 2]; // Avant /didl

    // TODO: Récupérer base_url depuis la config
    let base_url = "http://localhost:8080"; // Placeholder

    match cache.get_didl(pk, base_url).await {
        Ok(didl_xml) => (
            StatusCode::OK,
            [("content-type", "application/xml")],
            didl_xml,
        )
            .into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "Track not found").into_response(),
    }
}

/// Handler pour GET /audio/tracks/{pk}/status
/// Retourne le statut de conversion
async fn get_status(State(cache): State<Arc<AudioCache>>, req: Request<Body>) -> Response {
    let path = req.uri().path();
    let parts: Vec<&str> = path.split('/').collect();

    if parts.len() < 2 {
        return (StatusCode::BAD_REQUEST, "Invalid path").into_response();
    }

    let pk = parts[parts.len() - 2]; // Avant /status

    match cache.get_entry(pk).await {
        Ok(entry) => Json(serde_json::json!({
            "pk": entry.pk,
            "conversion_status": entry.conversion_status,
            "hits": entry.hits,
            "last_used": entry.last_used,
        }))
        .into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "Track not found").into_response(),
    }
}

/// Handler pour GET /audio/stats
async fn get_audio_stats(State(cache): State<Arc<AudioCache>>) -> Response {
    match cache.db.get_all() {
        Ok(entries) => Json(entries).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Cannot retrieve stats",
        )
            .into_response(),
    }
}

/// Handler pour GET /audio/collections
async fn list_collections(State(cache): State<Arc<AudioCache>>) -> Response {
    match cache.list_collections().await {
        Ok(collections) => Json(collections).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Cannot list collections",
        )
            .into_response(),
    }
}

#[cfg(feature = "pmoserver")]
impl AudioCacheExt for Server {
    async fn init_audio_cache(
        &mut self,
        cache_dir: &str,
        limit: usize,
    ) -> anyhow::Result<Arc<AudioCache>> {
        let cache = Arc::new(AudioCache::new(cache_dir, limit)?);

        // Routes pour servir les fichiers audio
        let tracks_router = Router::new()
            .route("/{pk}/stream", get(stream_audio))
            .route("/{pk}/metadata", get(get_metadata))
            .route("/{pk}/didl", get(get_didl))
            .route("/{pk}/status", get(get_status))
            .with_state(cache.clone());

        self.add_router("/audio/tracks", tracks_router).await;

        // Routes utilitaires
        self.add_handler_with_state("/audio/stats", get_audio_stats, cache.clone())
            .await;
        self.add_handler_with_state("/audio/collections", list_collections, cache.clone())
            .await;

        // Router API RESTful
        let api_router = Router::new()
            .route(
                "/",
                get(api::list_tracks)
                    .post(api::add_track)
                    .delete(api::purge_cache),
            )
            .route(
                "/{pk}",
                get(api::get_track_info).delete(api::delete_track),
            )
            .route("/{pk}/metadata", get(api::get_track_metadata))
            .route("/{pk}/didl", get(api::get_track_didl))
            .route("/consolidate", post(api::consolidate_cache))
            .with_state(cache.clone());

        // Documentation OpenAPI
        let openapi = crate::ApiDoc::openapi();

        // Enregistrer l'API avec Swagger UI
        self.add_openapi(api_router, openapi, "audio").await;

        info!(
            "Audio cache initialized at {} with limit {}",
            cache_dir, limit
        );

        Ok(cache)
    }

    async fn init_audio_cache_configured(&mut self) -> anyhow::Result<Arc<AudioCache>> {
        let config = pmoconfig::get_config();

        // TODO: Ajouter audio_cache dans la config
        let cache_dir = "./audio_cache"; // Placeholder
        let limit = 1000; // Placeholder

        info!(
            "Audio cache directory {}, size {}",
            cache_dir, limit
        );

        self.init_audio_cache(cache_dir, limit).await
    }
}
