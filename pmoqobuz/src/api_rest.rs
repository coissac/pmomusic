//! Endpoints API REST pour Qobuz
//!
//! Ce module définit les handlers HTTP pour accéder aux fonctionnalités Qobuz.

#[cfg(feature = "pmoserver")]
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json, Router,
};

#[cfg(feature = "pmoserver")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "pmoserver")]
use std::sync::Arc;

#[cfg(feature = "pmoserver")]
use crate::{client::QobuzClient, error::QobuzError, models::*};

/// État partagé de l'application
#[cfg(feature = "pmoserver")]
#[derive(Clone)]
pub struct QobuzState {
    pub client: Arc<QobuzClient>,
    #[cfg(feature = "covers")]
    pub cover_cache: Option<Arc<pmocovers::Cache>>,
}

/// Paramètres de recherche
#[cfg(feature = "pmoserver")]
#[derive(Debug, Deserialize)]
pub struct SearchParams {
    /// Requête de recherche
    pub q: String,
    /// Type de recherche (albums, artists, tracks, playlists)
    #[serde(rename = "type")]
    pub search_type: Option<String>,
}

/// Paramètres pour featured albums
#[cfg(feature = "pmoserver")]
#[derive(Debug, Deserialize)]
pub struct FeaturedAlbumsParams {
    /// ID du genre (optionnel)
    pub genre_id: Option<String>,
    /// Type (new-releases, ideal-discography, etc.)
    #[serde(rename = "type", default = "default_featured_type")]
    pub type_: String,
}

#[cfg(feature = "pmoserver")]
fn default_featured_type() -> String {
    "new-releases".to_string()
}

/// Paramètres pour featured playlists
#[cfg(feature = "pmoserver")]
#[derive(Debug, Deserialize)]
pub struct FeaturedPlaylistsParams {
    /// ID du genre (optionnel)
    pub genre_id: Option<String>,
    /// Tags (optionnel)
    pub tags: Option<String>,
}

/// Crée le router Axum avec tous les endpoints Qobuz
#[cfg(feature = "pmoserver")]
pub fn create_router(state: QobuzState) -> Router {
    Router::new()
        // Albums
        .route("/albums/:id", axum::routing::get(get_album))
        .route("/albums/:id/tracks", axum::routing::get(get_album_tracks))
        // Tracks
        .route("/tracks/:id", axum::routing::get(get_track))
        .route("/tracks/:id/stream", axum::routing::get(get_stream_url))
        // Artists
        .route("/artists/:id/albums", axum::routing::get(get_artist_albums))
        .route(
            "/artists/:id/similar",
            axum::routing::get(get_similar_artists),
        )
        // Playlists
        .route("/playlists/:id", axum::routing::get(get_playlist))
        .route(
            "/playlists/:id/tracks",
            axum::routing::get(get_playlist_tracks),
        )
        // Recherche
        .route("/search", axum::routing::get(search))
        // Favoris
        .route("/favorites/albums", axum::routing::get(get_favorite_albums))
        .route(
            "/favorites/artists",
            axum::routing::get(get_favorite_artists),
        )
        .route("/favorites/tracks", axum::routing::get(get_favorite_tracks))
        .route(
            "/favorites/playlists",
            axum::routing::get(get_user_playlists),
        )
        // Catalogue
        .route("/genres", axum::routing::get(get_genres))
        .route("/featured/albums", axum::routing::get(get_featured_albums))
        .route(
            "/featured/playlists",
            axum::routing::get(get_featured_playlists),
        )
        // Cache
        .route("/cache/stats", axum::routing::get(get_cache_stats))
        .with_state(state)
}

// ============ Handlers ============

#[cfg(feature = "pmoserver")]
async fn get_album(
    State(state): State<QobuzState>,
    Path(id): Path<String>,
) -> Result<Json<Album>, AppError> {
    let mut album = state.client.get_album(&id).await?;

    #[cfg(feature = "covers")]
    if let Some(ref cover_cache) = state.cover_cache {
        album = cache_album_image(album, cover_cache).await;
    }

    Ok(Json(album))
}

#[cfg(feature = "pmoserver")]
async fn get_album_tracks(
    State(state): State<QobuzState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<Track>>, AppError> {
    let tracks = state.client.get_album_tracks(&id).await?;
    Ok(Json(tracks))
}

#[cfg(feature = "pmoserver")]
async fn get_track(
    State(state): State<QobuzState>,
    Path(id): Path<String>,
) -> Result<Json<Track>, AppError> {
    let track = state.client.get_track(&id).await?;
    Ok(Json(track))
}

#[cfg(feature = "pmoserver")]
async fn get_stream_url(
    State(state): State<QobuzState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let url = state.client.get_stream_url(&id).await?;
    Ok(Json(serde_json::json!({ "url": url })))
}

#[cfg(feature = "pmoserver")]
async fn get_artist_albums(
    State(state): State<QobuzState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<Album>>, AppError> {
    let mut albums = state.client.get_artist_albums(&id).await?;

    #[cfg(feature = "covers")]
    if let Some(ref cover_cache) = state.cover_cache {
        albums = cache_albums_images(albums, cover_cache).await;
    }

    Ok(Json(albums))
}

#[cfg(feature = "pmoserver")]
async fn get_similar_artists(
    State(state): State<QobuzState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<Artist>>, AppError> {
    let artists = state.client.get_similar_artists(&id).await?;
    Ok(Json(artists))
}

#[cfg(feature = "pmoserver")]
async fn get_playlist(
    State(state): State<QobuzState>,
    Path(id): Path<String>,
) -> Result<Json<Playlist>, AppError> {
    let playlist = state.client.get_playlist(&id).await?;
    Ok(Json(playlist))
}

#[cfg(feature = "pmoserver")]
async fn get_playlist_tracks(
    State(state): State<QobuzState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<Track>>, AppError> {
    let tracks = state.client.get_playlist_tracks(&id).await?;
    Ok(Json(tracks))
}

#[cfg(feature = "pmoserver")]
async fn search(
    State(state): State<QobuzState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchResult>, AppError> {
    let mut result = state
        .client
        .search(&params.q, params.search_type.as_deref())
        .await?;

    #[cfg(feature = "covers")]
    if let Some(ref cover_cache) = state.cover_cache {
        result.albums = cache_albums_images(result.albums, cover_cache).await;
    }

    Ok(Json(result))
}

#[cfg(feature = "pmoserver")]
async fn get_favorite_albums(
    State(state): State<QobuzState>,
) -> Result<Json<Vec<Album>>, AppError> {
    let mut albums = state.client.get_favorite_albums().await?;

    #[cfg(feature = "covers")]
    if let Some(ref cover_cache) = state.cover_cache {
        albums = cache_albums_images(albums, cover_cache).await;
    }

    Ok(Json(albums))
}

#[cfg(feature = "pmoserver")]
async fn get_favorite_artists(
    State(state): State<QobuzState>,
) -> Result<Json<Vec<Artist>>, AppError> {
    let artists = state.client.get_favorite_artists().await?;
    Ok(Json(artists))
}

#[cfg(feature = "pmoserver")]
async fn get_favorite_tracks(
    State(state): State<QobuzState>,
) -> Result<Json<Vec<Track>>, AppError> {
    let tracks = state.client.get_favorite_tracks().await?;
    Ok(Json(tracks))
}

#[cfg(feature = "pmoserver")]
async fn get_user_playlists(
    State(state): State<QobuzState>,
) -> Result<Json<Vec<Playlist>>, AppError> {
    let playlists = state.client.get_user_playlists().await?;
    Ok(Json(playlists))
}

#[cfg(feature = "pmoserver")]
async fn get_genres(State(state): State<QobuzState>) -> Result<Json<Vec<Genre>>, AppError> {
    let genres = state.client.get_genres().await?;
    Ok(Json(genres))
}

#[cfg(feature = "pmoserver")]
async fn get_featured_albums(
    State(state): State<QobuzState>,
    Query(params): Query<FeaturedAlbumsParams>,
) -> Result<Json<Vec<Album>>, AppError> {
    let mut albums = state
        .client
        .get_featured_albums(params.genre_id.as_deref(), &params.type_)
        .await?;

    #[cfg(feature = "covers")]
    if let Some(ref cover_cache) = state.cover_cache {
        albums = cache_albums_images(albums, cover_cache).await;
    }

    Ok(Json(albums))
}

#[cfg(feature = "pmoserver")]
async fn get_featured_playlists(
    State(state): State<QobuzState>,
    Query(params): Query<FeaturedPlaylistsParams>,
) -> Result<Json<Vec<Playlist>>, AppError> {
    let playlists = state
        .client
        .get_featured_playlists(params.genre_id.as_deref(), params.tags.as_deref())
        .await?;
    Ok(Json(playlists))
}

#[cfg(feature = "pmoserver")]
async fn get_cache_stats(
    State(state): State<QobuzState>,
) -> Result<Json<crate::cache::CacheStats>, AppError> {
    let stats = state.client.cache().stats().await;
    Ok(Json(stats))
}

// ============ Helpers pour le cache d'images ============

#[cfg(all(feature = "pmoserver", feature = "covers"))]
async fn cache_album_image(mut album: Album, cover_cache: &Arc<pmocovers::Cache>) -> Album {
    if let Some(ref image_url) = album.image {
        match cover_cache.add_from_url(image_url, None).await {
            Ok(pk) => {
                album.image_cached = Some(format!("/covers/images/{}", pk));
            }
            Err(e) => {
                tracing::warn!("Failed to cache album image: {}", e);
            }
        }
    }
    album
}

#[cfg(all(feature = "pmoserver", feature = "covers"))]
async fn cache_albums_images(
    albums: Vec<Album>,
    cover_cache: &Arc<pmocovers::Cache>,
) -> Vec<Album> {
    let mut cached_albums = Vec::with_capacity(albums.len());
    for album in albums {
        cached_albums.push(cache_album_image(album, cover_cache).await);
    }
    cached_albums
}

// ============ Gestion des erreurs ============

#[cfg(feature = "pmoserver")]
struct AppError(QobuzError);

#[cfg(feature = "pmoserver")]
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self.0 {
            QobuzError::Unauthorized(_) => (StatusCode::UNAUTHORIZED, self.0.to_string()),
            QobuzError::NotFound(_) => (StatusCode::NOT_FOUND, self.0.to_string()),
            QobuzError::RateLimitExceeded => (StatusCode::TOO_MANY_REQUESTS, self.0.to_string()),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, self.0.to_string()),
        };

        let body = Json(serde_json::json!({
            "error": message
        }));

        (status, body).into_response()
    }
}

#[cfg(feature = "pmoserver")]
impl<E> From<E> for AppError
where
    E: Into<QobuzError>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}
