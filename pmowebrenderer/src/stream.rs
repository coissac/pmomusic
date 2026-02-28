//! Handler HTTP GET /api/webrenderer/{id}/stream
//!
//! Sert le flux OGG-FLAC d'une instance WebRenderer.
//!
//! Safari fait systématiquement une requête Range: bytes=0-1 avant de jouer.
//! On répond 206 avec 2 octets factices pour satisfaire la sonde,
//! puis la vraie requête (sans Range) reçoit le stream persistant.

use axum::{
    body::Body,
    extract::{Path, State},
    http::{
        HeaderMap, StatusCode,
        header::{CACHE_CONTROL, CONNECTION, CONTENT_TYPE, CONTENT_RANGE, CONTENT_LENGTH, ACCEPT_RANGES},
    },
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use tokio_util::io::ReaderStream;
use tracing::info;

use crate::registry::RendererRegistry;

/// GET /api/webrenderer/{id}/stream
pub async fn stream_handler(
    State(registry): State<Arc<RendererRegistry>>,
    Path(instance_id): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    info!(instance_id = %instance_id, "FLAC stream client connecting");

    // Détecter la sonde Range: bytes=0-1 de Safari
    if let Some(range) = headers.get("range") {
        if range.as_bytes() == b"bytes=0-1" {
            info!(instance_id = %instance_id, "Safari range probe — responding 206");
            return Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(CONTENT_TYPE, "audio/ogg; codecs=flac")
                .header(ACCEPT_RANGES, "bytes")
                .header(CONTENT_RANGE, "bytes 0-1/*")
                .header(CONTENT_LENGTH, "2")
                .body(Body::from(vec![0u8, 0u8]))
                .unwrap()
                .into_response();
        }
    }

    let stream = match registry.get_stream(&instance_id) {
        Some(s) => s,
        None => {
            return (
                StatusCode::NOT_FOUND,
                format!("No WebRenderer instance for id={}", instance_id),
            )
                .into_response()
        }
    };

    info!(instance_id = %instance_id, "FLAC stream started");

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "audio/ogg; codecs=flac")
        .header(CACHE_CONTROL, "no-store, no-transform")
        .header(CONNECTION, "keep-alive")
        .header(ACCEPT_RANGES, "bytes")
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::from_stream(ReaderStream::new(stream)))
        .unwrap()
        .into_response()
}
