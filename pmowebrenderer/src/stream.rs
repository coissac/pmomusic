//! Handler HTTP GET /api/webrenderer/{id}/stream
//!
//! Sert le flux FLAC d'une instance WebRenderer via DirectFlacSink.
//!
//! Reconnectable : appelé à chaque Play, crée un nouveau pipe + encodeur.

use axum::{
    body::Body,
    extract::{Path, State},
    http::{
        StatusCode,
        header::{CACHE_CONTROL, CONNECTION, CONTENT_TYPE},
    },
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use tokio_util::io::ReaderStream;
use tracing::info;

use crate::registry::RendererRegistry;

/// GET /api/webrenderer/{id}/stream
///
/// Crée un nouveau pipe FLAC à chaque connexion (chaque Play).
/// Le flux reste ouvert jusqu'à ce que le client se déconnecte (Stop).
/// Les morceaux s'enchaînent en gapless dans le même flux.
///
/// Safari envoie un Range header (bytes=0-) et exige Accept-Ranges: bytes.
/// On répond 206 Partial Content si un Range header est présent, sinon 200.
pub async fn stream_handler(
    State(registry): State<Arc<RendererRegistry>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    info!(instance_id = %instance_id, "FLAC stream client connecting");

    let handle = match registry.get_flac_handle(&instance_id) {
        Some(h) => h,
        None => {
            return (
                StatusCode::NOT_FOUND,
                format!("No WebRenderer instance for id={}", instance_id),
            )
                .into_response()
        }
    };

    let stream = handle.subscribe();

    info!(instance_id = %instance_id, "FLAC stream started");

    // Toujours 200 OK pour un flux live de taille inconnue.
    // Les navigateurs (Safari inclus) acceptent 200 pour l'audio streaming.
    // Un Content-Range invalide (bytes 0-*/*) ferait rejeter le flux.
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "audio/ogg; codecs=flac")
        .header(CACHE_CONTROL, "no-store, no-transform")
        .header(CONNECTION, "keep-alive")
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::from_stream(ReaderStream::new(stream)))
        .unwrap()
        .into_response()
}
