//! Documentation OpenAPI pour les endpoints playlists (SSE évènements).

#[cfg(feature = "pmoserver")]
use utoipa::OpenApi;

/// Documentation OpenAPI pour l'API playlist (flux SSE).
#[derive(OpenApi)]
#[openapi(
    paths(
        crate::api::list_playlists,
        crate::api::create_playlist,
        crate::api::get_playlist,
        crate::api::update_playlist,
        crate::api::delete_playlist,
        crate::api::add_tracks,
        crate::api::flush_tracks,
        crate::api::remove_track,
        crate::sse::playlist_events_sse,
    ),
    components(
        schemas(
            crate::api::PlaylistSummaryResponse,
            crate::api::PlaylistDetailResponse,
            crate::api::PlaylistTrackResponse,
            crate::api::CreatePlaylistRequest,
            crate::api::UpdatePlaylistRequest,
            crate::api::AddTracksRequest,
            crate::api::ErrorResponse,
            crate::sse::EventPayload,
            crate::sse::EventsQuery,
        )
    ),
    tags(
        (name = "playlists", description = "Suivi des playlists et des morceaux joués")
    ),
    info(
        title = "PMO Playlist API",
        version = "0.1.0",
        description = r#"
# Flux d'évènements playlists

Endpoint SSE pour suivre :
- les modifications de playlists (updated)
- les lectures de morceaux appartenant aux playlists (track_played)

Payload JSON par évènement :
- `playlist_id` : identifiant de la playlist
- `kind` : `updated` ou `track_played`
- `cache_pk` : pk du morceau (si track_played)
- `qualifier` : qualifier de diffusion (orig/stream/etc.)
- `timestamp` : horodatage UTC
- `source_client` : client à l'origine (optionnel)
        "#,
        license(
            name = "MIT",
        ),
    )
)]
pub struct ApiDoc;
