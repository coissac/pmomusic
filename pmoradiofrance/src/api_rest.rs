//! Endpoints API REST pour Radio France
//!
//! Ce module définit les handlers HTTP pour accéder aux stations Radio France,
//! leurs métadonnées live et les flux de streaming.

use crate::models::LiveResponse;
use crate::playlist::StationGroups;
use crate::pmoserver_ext::RadioFranceState;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use futures::StreamExt;
use serde_json;

// ============ Gestion des erreurs ============

struct AppError(String);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self.0.as_str() {
            "not_found" => (StatusCode::NOT_FOUND, self.0),
            "internal_error" => (StatusCode::INTERNAL_SERVER_ERROR, self.0),
            "bad_gateway" => (StatusCode::BAD_GATEWAY, self.0),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, self.0),
        };

        let body = Json(serde_json::json!({
            "error": message
        }));

        (status, body).into_response()
    }
}

impl From<String> for AppError {
    fn from(err: String) -> Self {
        Self(err)
    }
}

/// Crée le router pour l'API Radio France
pub fn create_router(state: RadioFranceState) -> Router {
    Router::new()
        .route("/stations", get(get_stations))
        .route("/{slug}/metadata", get(get_metadata))
        .route("/{slug}/stream", get(proxy_stream))
        .route("/default-logo", get(get_default_logo))
        .with_state(state)
}

// ============================================================================
// Route Handlers
// ============================================================================

/// GET /api/radiofrance/stations
/// Returns the grouped list of stations
#[axum::debug_handler]
async fn get_stations(
    State(state): State<RadioFranceState>,
) -> Result<Json<StationGroups>, AppError> {
    let stations = state
        .client
        .get_stations()
        .await
        .map_err(|e| AppError(e.to_string()))?;

    let groups = StationGroups::from_stations(stations);
    Ok(Json(groups))
}

/// GET /api/radiofrance/{slug}/metadata
/// Returns live metadata for a station (with caching)
async fn get_metadata(
    State(state): State<RadioFranceState>,
    Path(slug): Path<String>,
) -> Result<Json<LiveResponse>, AppError> {
    let metadata = state
        .client
        .get_live_metadata(&slug)
        .await
        .map_err(|e| AppError(e.to_string()))?;

    Ok(Json(metadata))
}

/// GET /api/radiofrance/{slug}/stream
/// Proxies the AAC stream from Radio France (passthrough, no transcoding)
async fn proxy_stream(
    State(state): State<RadioFranceState>,
    Path(slug): Path<String>,
) -> Result<Response, AppError> {
    // Get the stream URL
    let stream_url = state
        .client
        .get_stream_url(&slug)
        .await
        .map_err(|e| AppError(format!("Stream not found: {}", e)))?;

    // Connect to the Radio France stream
    let response = reqwest::get(&stream_url)
        .await
        .map_err(|e| AppError(format!("Failed to connect: {}", e)))?;

    if !response.status().is_success() {
        return Err(AppError(format!("Upstream returned {}", response.status())));
    }

    // Build response headers
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "audio/aac".parse().unwrap());
    headers.insert("cache-control", "no-cache".parse().unwrap());

    // Create streaming body
    let stream = response
        .bytes_stream()
        .map(|chunk| chunk.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)));

    let body = Body::from_stream(stream);

    Ok((headers, body).into_response())
}

/// GET /api/radiofrance/default-logo
/// Returns the default Radio France logo (embedded in binary)
async fn get_default_logo() -> impl IntoResponse {
    use crate::source::RADIOFRANCE_DEFAULT_IMAGE;

    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", "image/webp".parse().unwrap());
    headers.insert("Cache-Control", "public, max-age=86400".parse().unwrap());

    (headers, RADIOFRANCE_DEFAULT_IMAGE).into_response()
}
