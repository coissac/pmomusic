//! API REST pour la gestion des playlists.

use std::time::{Duration, SystemTime};

use axum::{
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::manager::{PlaylistOverview, PlaylistSnapshot, PlaylistTrackSnapshot};
use crate::PlaylistRole;

/// Router `/api/playlists` combinant les différents endpoints REST.
pub fn playlist_api_router() -> Router {
    Router::new()
        .route("/", get(list_playlists).post(create_playlist))
        .route(
            "/{playlist_id}",
            get(get_playlist)
                .patch(update_playlist)
                .delete(delete_playlist),
        )
        .route(
            "/{playlist_id}/tracks",
            post(add_tracks).delete(flush_tracks),
        )
        .route("/{playlist_id}/tracks/{cache_pk}", delete(remove_track))
}

/// Résumé d'une playlist (utilisé dans les listings).
#[derive(Debug, Serialize, ToSchema)]
pub struct PlaylistSummaryResponse {
    pub id: String,
    pub title: String,
    #[schema(value_type = String)]
    pub role: PlaylistRole,
    pub persistent: bool,
    pub track_count: usize,
    pub max_size: Option<usize>,
    pub default_ttl_secs: Option<u64>,
    pub last_change: DateTime<Utc>,
}

/// Réponse détaillée pour une playlist (inclut les tracks).
#[derive(Debug, Serialize, ToSchema)]
pub struct PlaylistDetailResponse {
    #[serde(flatten)]
    #[schema(inline)]
    pub summary: PlaylistSummaryResponse,
    pub tracks: Vec<PlaylistTrackResponse>,
}

/// Track référencé dans une playlist.
#[derive(Debug, Serialize, ToSchema)]
pub struct PlaylistTrackResponse {
    pub cache_pk: String,
    pub added_at: DateTime<Utc>,
    pub ttl_secs: Option<u64>,
}

/// Requête pour créer une playlist persistante/éphémère.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePlaylistRequest {
    pub id: String,
    pub title: Option<String>,
    #[schema(value_type = String)]
    pub role: Option<PlaylistRole>,
    #[schema(example = true)]
    pub persistent: Option<bool>,
    pub max_size: Option<usize>,
    pub default_ttl_secs: Option<u64>,
}

/// Requête pour mettre à jour les métadonnées/config d'une playlist.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdatePlaylistRequest {
    pub title: Option<String>,
    #[schema(value_type = String)]
    pub role: Option<PlaylistRole>,
    pub max_size: Option<Option<usize>>,
    pub default_ttl_secs: Option<Option<u64>>,
}

/// Requête pour ajouter des morceaux dans une playlist.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddTracksRequest {
    pub cache_pks: Vec<String>,
    #[schema(example = 3600)]
    pub ttl_secs: Option<u64>,
    #[schema(example = false)]
    pub lazy: Option<bool>,
}

/// Réponse d'erreur REST générique.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}

#[utoipa::path(
    get,
    path = "/api/playlists",
    tag = "playlists",
    responses(
        (status = 200, description = "Liste de toutes les playlists", body = [PlaylistSummaryResponse])
    )
)]
pub async fn list_playlists() -> Response {
    let manager = crate::manager::PlaylistManager();
    match manager.all_playlist_overviews().await {
        Ok(overviews) => {
            let payload: Vec<PlaylistSummaryResponse> = overviews
                .into_iter()
                .map(PlaylistSummaryResponse::from)
                .collect();
            (StatusCode::OK, Json(payload)).into_response()
        }
        Err(err) => map_error(err),
    }
}

#[utoipa::path(
    post,
    path = "/api/playlists",
    tag = "playlists",
    request_body = CreatePlaylistRequest,
    responses(
        (status = 201, description = "Playlist créée", body = PlaylistDetailResponse),
        (status = 400, description = "Requête invalide", body = ErrorResponse),
        (status = 409, description = "Playlist déjà existante", body = ErrorResponse)
    )
)]
pub async fn create_playlist(Json(req): Json<CreatePlaylistRequest>) -> Response {
    if req.id.trim().is_empty() {
        return map_status(
            StatusCode::BAD_REQUEST,
            "INVALID_ID",
            "Playlist id cannot be empty",
        );
    }

    let manager = crate::manager::PlaylistManager();
    let persistent = req.persistent.unwrap_or(true);
    let requested_role = req.role.clone();
    let role = requested_role.clone().unwrap_or_else(PlaylistRole::user);

    let result = async move {
        let id = req.id.clone();
        if persistent {
            let writer = manager
                .create_persistent_playlist_with_role(id.clone(), role)
                .await?;
            apply_metadata_updates(
                &writer,
                req.title.clone(),
                req.max_size,
                req.default_ttl_secs,
            )
            .await?;
        } else {
            let writer = manager.get_write_handle(id.clone()).await?;
            if let Some(role) = requested_role {
                writer.set_role(role).await?;
            }
            apply_metadata_updates(
                &writer,
                req.title.clone(),
                req.max_size,
                req.default_ttl_secs,
            )
            .await?;
        }

        manager.playlist_snapshot(&req.id).await
    }
    .await;

    match result {
        Ok(snapshot) => (
            StatusCode::CREATED,
            Json(PlaylistDetailResponse::from(snapshot)),
        )
            .into_response(),
        Err(err) => map_error(err),
    }
}

#[utoipa::path(
    get,
    path = "/api/playlists/{playlist_id}",
    tag = "playlists",
    params(
        ("playlist_id" = String, Path, description = "Identifiant de la playlist")
    ),
    responses(
        (status = 200, description = "Playlist détaillée", body = PlaylistDetailResponse),
        (status = 404, description = "Playlist introuvable", body = ErrorResponse)
    )
)]
pub async fn get_playlist(Path(playlist_id): Path<String>) -> Response {
    let manager = crate::manager::PlaylistManager();
    match manager.playlist_snapshot(&playlist_id).await {
        Ok(snapshot) => {
            (StatusCode::OK, Json(PlaylistDetailResponse::from(snapshot))).into_response()
        }
        Err(err) => map_error(err),
    }
}

#[utoipa::path(
    patch,
    path = "/api/playlists/{playlist_id}",
    tag = "playlists",
    params(
        ("playlist_id" = String, Path, description = "Identifiant de la playlist")
    ),
    request_body = UpdatePlaylistRequest,
    responses(
        (status = 200, description = "Playlist mise à jour", body = PlaylistDetailResponse),
        (status = 404, description = "Playlist introuvable", body = ErrorResponse)
    )
)]
pub async fn update_playlist(
    Path(playlist_id): Path<String>,
    Json(req): Json<UpdatePlaylistRequest>,
) -> Response {
    let UpdatePlaylistRequest {
        title,
        role,
        max_size,
        default_ttl_secs,
    } = req;

    let manager = crate::manager::PlaylistManager();
    let result = async move {
        // S'assurer que la playlist existe
        manager.get_read_handle(&playlist_id).await?;
        let writer = manager.get_write_handle(playlist_id.clone()).await?;

        if let Some(title) = title {
            writer.set_title(title).await?;
        }
        if let Some(role) = role {
            writer.set_role(role).await?;
        }
        if let Some(capacity) = max_size {
            writer.set_capacity(capacity).await?;
        }
        if let Some(ttl) = default_ttl_secs {
            writer.set_default_ttl(ttl.map(Duration::from_secs)).await?;
        }

        manager.playlist_snapshot(&playlist_id).await
    }
    .await;

    match result {
        Ok(snapshot) => {
            (StatusCode::OK, Json(PlaylistDetailResponse::from(snapshot))).into_response()
        }
        Err(err) => map_error(err),
    }
}

#[utoipa::path(
    delete,
    path = "/api/playlists/{playlist_id}",
    tag = "playlists",
    params(
        ("playlist_id" = String, Path, description = "Identifiant de la playlist")
    ),
    responses(
        (status = 204, description = "Playlist supprimée"),
        (status = 404, description = "Playlist introuvable", body = ErrorResponse)
    )
)]
pub async fn delete_playlist(Path(playlist_id): Path<String>) -> Response {
    let manager = crate::manager::PlaylistManager();
    match manager.delete_playlist(&playlist_id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => map_error(err),
    }
}

#[utoipa::path(
    post,
    path = "/api/playlists/{playlist_id}/tracks",
    tag = "playlists",
    params(
        ("playlist_id" = String, Path, description = "Identifiant de la playlist")
    ),
    request_body = AddTracksRequest,
    responses(
        (status = 200, description = "Morceaux ajoutés", body = PlaylistDetailResponse),
        (status = 400, description = "Requête invalide", body = ErrorResponse),
        (status = 404, description = "Playlist introuvable", body = ErrorResponse)
    )
)]
pub async fn add_tracks(
    Path(playlist_id): Path<String>,
    Json(req): Json<AddTracksRequest>,
) -> Response {
    if req.cache_pks.is_empty() {
        return map_status(
            StatusCode::BAD_REQUEST,
            "EMPTY_PAYLOAD",
            "cache_pks cannot be empty",
        );
    }

    let manager = crate::manager::PlaylistManager();
    let result = async {
        manager.get_read_handle(&playlist_id).await?;
        let writer = manager.get_write_handle(playlist_id.clone()).await?;
        let ttl = req.ttl_secs.map(Duration::from_secs);
        let use_lazy = req.lazy.unwrap_or(false);

        if use_lazy {
            if req.cache_pks.len() == 1 {
                writer.push_lazy(req.cache_pks[0].clone()).await?;
            } else {
                writer.push_lazy_batch(req.cache_pks.clone()).await?;
            }
        } else if let Some(ttl) = ttl {
            for pk in &req.cache_pks {
                writer.push_with_ttl(pk.clone(), ttl).await?;
            }
        } else if req.cache_pks.len() == 1 {
            writer.push(req.cache_pks[0].clone()).await?;
        } else {
            writer.push_set(req.cache_pks.clone()).await?;
        }

        manager.playlist_snapshot(&playlist_id).await
    }
    .await;

    match result {
        Ok(snapshot) => {
            (StatusCode::OK, Json(PlaylistDetailResponse::from(snapshot))).into_response()
        }
        Err(err) => map_error(err),
    }
}

#[utoipa::path(
    delete,
    path = "/api/playlists/{playlist_id}/tracks",
    tag = "playlists",
    params(
        ("playlist_id" = String, Path, description = "Identifiant de la playlist")
    ),
    responses(
        (status = 200, description = "Playlist vidée", body = PlaylistDetailResponse),
        (status = 404, description = "Playlist introuvable", body = ErrorResponse)
    )
)]
pub async fn flush_tracks(Path(playlist_id): Path<String>) -> Response {
    let manager = crate::manager::PlaylistManager();
    let result = async {
        manager.get_read_handle(&playlist_id).await?;
        let writer = manager.get_write_handle(playlist_id.clone()).await?;
        writer.flush().await?;
        manager.playlist_snapshot(&playlist_id).await
    }
    .await;

    match result {
        Ok(snapshot) => {
            (StatusCode::OK, Json(PlaylistDetailResponse::from(snapshot))).into_response()
        }
        Err(err) => map_error(err),
    }
}

#[utoipa::path(
    delete,
    path = "/api/playlists/{playlist_id}/tracks/{cache_pk}",
    tag = "playlists",
    params(
        ("playlist_id" = String, Path, description = "Identifiant de la playlist"),
        ("cache_pk" = String, Path, description = "PK à retirer")
    ),
    responses(
        (status = 200, description = "Track retiré", body = PlaylistDetailResponse),
        (status = 404, description = "Playlist ou track introuvable", body = ErrorResponse)
    )
)]
pub async fn remove_track(Path((playlist_id, cache_pk)): Path<(String, String)>) -> Response {
    let manager = crate::manager::PlaylistManager();
    let result = async {
        manager.get_read_handle(&playlist_id).await?;
        let writer = manager.get_write_handle(playlist_id.clone()).await?;
        if !writer.remove_track(&cache_pk).await? {
            return Err(crate::Error::CacheEntryNotFound(cache_pk));
        }
        manager.playlist_snapshot(&playlist_id).await
    }
    .await;

    match result {
        Ok(snapshot) => {
            (StatusCode::OK, Json(PlaylistDetailResponse::from(snapshot))).into_response()
        }
        Err(crate::Error::CacheEntryNotFound(pk)) => map_status(
            StatusCode::NOT_FOUND,
            "TRACK_NOT_FOUND",
            &format!("Track '{}' not found in playlist", pk),
        ),
        Err(err) => map_error(err),
    }
}

fn apply_metadata_updates(
    writer: &crate::handle::WriteHandle,
    title: Option<String>,
    max_size: Option<usize>,
    default_ttl_secs: Option<u64>,
) -> impl std::future::Future<Output = Result<(), crate::Error>> + '_ {
    async move {
        if let Some(title) = title {
            writer.set_title(title).await?;
        }
        if let Some(max) = max_size {
            writer.set_capacity(Some(max)).await?;
        }
        if let Some(ttl) = default_ttl_secs {
            writer
                .set_default_ttl(Some(Duration::from_secs(ttl)))
                .await?;
        }
        Ok(())
    }
}

fn playlist_track_to_response(track: &PlaylistTrackSnapshot) -> PlaylistTrackResponse {
    PlaylistTrackResponse {
        cache_pk: track.cache_pk.clone(),
        added_at: system_time_to_datetime(track.added_at),
        ttl_secs: track.ttl.map(|ttl| ttl.as_secs()),
    }
}

impl From<PlaylistOverview> for PlaylistSummaryResponse {
    fn from(value: PlaylistOverview) -> Self {
        Self {
            id: value.id,
            title: value.title,
            role: value.role,
            persistent: value.persistent,
            track_count: value.track_count,
            max_size: value.max_size,
            default_ttl_secs: value.default_ttl.map(|ttl| ttl.as_secs()),
            last_change: system_time_to_datetime(value.last_change),
        }
    }
}

impl From<PlaylistSnapshot> for PlaylistDetailResponse {
    fn from(value: PlaylistSnapshot) -> Self {
        let summary = PlaylistSummaryResponse::from(value.overview);
        let tracks = value
            .tracks
            .iter()
            .map(playlist_track_to_response)
            .collect();
        Self { summary, tracks }
    }
}

fn system_time_to_datetime(time: SystemTime) -> DateTime<Utc> {
    DateTime::<Utc>::from(time)
}

fn map_status<S: Into<String>>(status: StatusCode, error: &str, message: S) -> Response {
    (
        status,
        Json(ErrorResponse {
            error: error.to_string(),
            message: message.into(),
        }),
    )
        .into_response()
}

fn map_error(error: crate::Error) -> Response {
    let status = match error {
        crate::Error::PlaylistNotFound(_) | crate::Error::PlaylistDeleted(_) => {
            StatusCode::NOT_FOUND
        }
        crate::Error::PlaylistAlreadyExists(_) | crate::Error::WriteLockHeld(_) => {
            StatusCode::CONFLICT
        }
        crate::Error::CacheEntryNotFound(_) => StatusCode::BAD_REQUEST,
        crate::Error::PlaylistNotPersistent(_) => StatusCode::BAD_REQUEST,
        crate::Error::CacheError(_)
        | crate::Error::PersistenceError(_)
        | crate::Error::ManagerNotInitialized
        | crate::Error::Other(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };

    (
        status,
        Json(ErrorResponse {
            error: format!("{:?}", error),
            message: error.to_string(),
        }),
    )
        .into_response()
}
