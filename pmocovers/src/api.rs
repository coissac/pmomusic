//! API REST handlers spécifiques au cache de couvertures

use crate::cache;
use crate::Cache;
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use pmocache::api::{AddItemRequest, AddItemResponse, ErrorResponse};
use std::sync::Arc;

#[derive(Clone, Copy)]
enum AddSource<'a> {
    Url(&'a str),
    Local(&'a str),
}

/// Handler spécialisé pour l'ajout d'images dans le cache de couvertures.
///
/// Supporte l'ajout depuis une URL (avec conversion WebP) ou depuis un fichier local
/// (avec conversion ou passthrough selon le format).
pub async fn add_cover_item(
    State(cache): State<Arc<Cache>>,
    Json(req): Json<AddItemRequest>,
) -> impl IntoResponse {
    let mode = match (req.url.as_deref(), req.path.as_deref()) {
        (Some(url), None) if !url.is_empty() => AddSource::Url(url),
        (None, Some(path)) if !path.is_empty() => AddSource::Local(path),
        (Some(_), Some(_)) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "INVALID_REQUEST".to_string(),
                    message: "Provide either 'url' or 'path', not both".to_string(),
                }),
            )
                .into_response()
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "INVALID_REQUEST".to_string(),
                    message: "Either 'url' or 'path' must be provided".to_string(),
                }),
            )
                .into_response()
        }
    };

    let collection = req.collection.as_deref();
    let add_result = match mode {
        AddSource::Url(url) => cache.add_from_url(url, collection).await,
        AddSource::Local(path) => cache::add_local_file(&cache, path, collection).await,
    };

    match add_result {
        Ok(pk) => {
            let origin =
                cache
                    .db
                    .get_origin_url(&pk)
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| match mode {
                        AddSource::Url(url) => url.to_string(),
                        AddSource::Local(path) => format!("file://{}", path),
                    });

            (
                StatusCode::CREATED,
                Json(AddItemResponse {
                    pk,
                    url: origin,
                    message: "Image added successfully".to_string(),
                }),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "PROCESSING_ERROR".to_string(),
                message: format!("Cannot add image: {}", e),
            }),
        )
            .into_response(),
    }
}
