//! Handler HTTP GET /api/webrenderer/{id}/stream
//!
//! Sert le flux OGG-FLAC d'une instance WebRenderer.
//!
//! Safari envoie parfois Range: bytes=0-N avant de jouer.
//! On ignore ce header et on répond toujours 200 chunked (flux live infini).

use axum::{
    body::Body,
    extract::{Path, State},
    http::{
        header::{CACHE_CONTROL, CONNECTION, CONTENT_TYPE, TRANSFER_ENCODING},
        HeaderMap, StatusCode,
    },
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use tokio_util::io::ReaderStream;
use tracing::{error, info};

use pmomediarenderer::MediaRendererRegistry;

/// GET /api/webrenderer/{id}/stream
pub async fn stream_handler(
    State(registry): State<Arc<MediaRendererRegistry>>,
    Path(instance_id): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    info!(instance_id = %instance_id, "FLAC stream client connecting");

    // Ignorer le header Range — flux live infini, non seekable.
    if let Some(range) = headers.get("range") {
        info!(instance_id = %instance_id, "Range header ignored: {:?}", range);
    }

    let stream = match registry.get_stream(&instance_id) {
        Some(s) => {
            info!(instance_id = %instance_id, "Found instance, getting stream");
            s
        }
        None => {
            error!(instance_id = %instance_id, "No WebRenderer instance found!");
            return (
                StatusCode::NOT_FOUND,
                format!("No WebRenderer instance for id={}", instance_id),
            )
                .into_response();
        }
    };

    info!(instance_id = %instance_id, "FLAC stream started - returning OGG-FLAC");

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "audio/ogg; codecs=flac")
        .header(CACHE_CONTROL, "no-store, no-transform")
        .header(CONNECTION, "keep-alive")
        .header(TRANSFER_ENCODING, "chunked")
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::from_stream(ReaderStream::new(stream)))
        .unwrap()
        .into_response()
}
