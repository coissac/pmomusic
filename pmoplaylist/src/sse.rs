//! SSE pour suivre les évènements de playlists (updates + morceaux joués).
//!
//! Route type : `GET /api/playlists/events?playlist_id=foo`

use crate::{subscribe_events, PlaylistEventKind};
#[cfg(feature = "pmoserver")]
use async_stream::stream;
use axum::{
    extract::Query,
    response::sse::{Event, KeepAlive, Sse},
    response::IntoResponse,
    Router,
};
use serde::{Deserialize, Serialize};
#[cfg(feature = "pmoserver")]
use tokio_stream::StreamExt;

#[derive(Debug, Default, Deserialize)]
#[cfg_attr(feature = "pmoserver", derive(utoipa::IntoParams, utoipa::ToSchema))]
pub struct EventsQuery {
    /// Filtrer sur une playlist précise (optionnel).
    #[serde(default)]
    pub playlist_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "pmoserver", derive(utoipa::ToSchema))]
pub struct EventPayload {
    pub playlist_id: String,
    pub kind: String,
    pub cache_pk: Option<String>,
    pub qualifier: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub source_client: Option<String>,
}

/// Handler SSE : diffuse les évènements playlist enrichis.
#[utoipa::path(
    get,
    path = "/api/playlists/events",
    tag = "playlists",
    params(EventsQuery),
    responses(
        (status = 200, description = "Flux SSE des évènements playlists (updated, track_played)", content_type = "text/event-stream")
    )
)]
pub async fn playlist_events_sse(Query(params): Query<EventsQuery>) -> impl IntoResponse {
    let mut rx = subscribe_events();

    let stream = stream! {
        while let Ok(envelope) = rx.recv().await {
            if let Some(filter) = &params.playlist_id {
                if &envelope.event.playlist_id != filter {
                    continue;
                }
            }

            let (kind, cache_pk, qualifier) = match &envelope.event.kind {
                PlaylistEventKind::Updated => ("updated", None, None),
                PlaylistEventKind::TrackPlayed { cache_pk, qualifier } => {
                    ("track_played", Some(cache_pk.as_str()), Some(qualifier.as_str()))
                }
            };

            let ts = chrono::DateTime::<chrono::Utc>::from(envelope.timestamp);
            let payload = EventPayload {
                playlist_id: envelope.event.playlist_id.clone(),
                kind: kind.to_string(),
                cache_pk: cache_pk.map(|s| s.to_string()),
                qualifier: qualifier.map(|s| s.to_string()),
                timestamp: ts,
                source_client: envelope.source_client.clone(),
            };

            if let Ok(json) = serde_json::to_string(&payload) {
                yield Ok::<_, axum::Error>(Event::default().event("playlist").data(json));
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Router prêt à être monté (ex: `/api/playlists/events`).
pub fn playlist_events_router() -> Router {
    use axum::routing::get;

    Router::new().route("/events", get(playlist_events_sse))
}
