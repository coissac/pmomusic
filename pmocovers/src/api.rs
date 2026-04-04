//! API REST handlers spécifiques au cache de couvertures

use crate::cache;
use crate::Cache;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use pmocache::api::{AddItemRequest, AddItemResponse, ErrorResponse};
use pmocache::covers_route_for;
use serde::{Deserialize, Serialize};
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

// ============================================================================
// Proxy pour covers LAN externes
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CoverProxyParams {
    url: String,
}

#[derive(Debug, Serialize)]
pub struct CoverProxyResponse {
    pub cached_url: String,
    pub pk: String,
}

/// GET /covers/proxy?url=<encoded_url>
/// Proxy transparent qui :
/// 1. Détecte si l'URL est une URL LAN externe (pas déjà locale)
/// 2. Ajoute à cache via add_from_url (déduplication automatique)
/// 3. Retourne l'URL locale du cache
#[cfg(feature = "pmoserver")]
pub async fn cover_proxy_handler(
    Query(params): Query<CoverProxyParams>,
    State(cache): State<Arc<Cache>>,
    Extension(base_url): Extension<pmoserver::BaseUrl>,
) -> impl IntoResponse {
    let external_url = &params.url;

    // Ignorer si déjà une URL de NOTRE instance pmomusic (ne pas se cacher soi-même)
    if is_local_cover_url(external_url, &base_url) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "INVALID_REQUEST".to_string(),
                message: "URL is already a local cover from this instance".to_string(),
            }),
        )
            .into_response();
    }

    // Vérifier si c'est une URL LAN à proxyfier
    if !should_proxy_url(external_url) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "INVALID_REQUEST".to_string(),
                message: "URL is not a LAN URL requiring proxy".to_string(),
            }),
        )
            .into_response();
    }

    // Ajouter au cache (add_from_url gère la déduplication)
    match cache.add_from_url(external_url, Some("external-covers")).await {
        Ok(pk) => {
            // Retourner l'URL locale
            let local_url = base_url.url_for(&covers_route_for(&pk, None));
            (
                StatusCode::OK,
                Json(CoverProxyResponse {
                    cached_url: local_url,
                    pk,
                }),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse {
                error: "CACHE_ERROR".to_string(),
                message: format!("Failed to cache external cover: {}", e),
            }),
        )
            .into_response(),
    }
}

/// Vérifie si l'URL est déjà une cover locale de NOTRE instance pmomusic
/// Note: Les covers d'autres instances pmomusic sur le LAN DEVRAIENT être proxyfiées
fn is_local_cover_url(url: &str, base_url: &pmoserver::BaseUrl) -> bool {
    url.starts_with(&base_url.0)
}

/// Vérifie si l'URL doit être proxyfiée (URL LAN externe)
fn should_proxy_url(url: &str) -> bool {
    if let Ok(parsed) = url::Url::parse(url) {
        if let Some(host) = parsed.host_str() {
            // Proxy uniquement les URLs LAN (pas les URLs publiques)
            if let Ok(ip) = host.parse::<std::net::IpAddr>() {
                return match ip {
                    std::net::IpAddr::V4(ipv4) => ipv4.is_private() || ipv4.is_loopback(),
                    std::net::IpAddr::V6(ipv6) => ipv6.is_loopback(),
                };
            }
            // aussi les .local
            return host.ends_with(".local") || host == "localhost";
        }
    }
    false
}
