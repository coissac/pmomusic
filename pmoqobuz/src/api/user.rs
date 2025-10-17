//! Module d'accès aux données utilisateur (favoris)

use super::catalog::{AlbumResponse, ArtistResponse, PlaylistResponse, TrackResponse};
use super::QobuzApi;
use crate::error::{QobuzError, Result};
use crate::models::*;
use serde::Deserialize;
use tracing::debug;

/// Réponse paginée
#[derive(Debug, Deserialize)]
struct PaginatedResponse<T> {
    items: Vec<T>,
}

/// Réponse de l'endpoint /favorite/getUserFavorites
#[derive(Debug, Deserialize)]
struct FavoritesResponse {
    #[serde(default)]
    albums: Option<PaginatedResponse<AlbumResponse>>,
    #[serde(default)]
    artists: Option<PaginatedResponse<ArtistResponse>>,
    #[serde(default)]
    tracks: Option<PaginatedResponse<TrackResponse>>,
}

/// Réponse de l'endpoint /playlist/getUserPlaylists
#[derive(Debug, Deserialize)]
struct UserPlaylistsResponse {
    playlists: PaginatedResponse<PlaylistResponse>,
}

impl QobuzApi {
    /// Vérifie que l'utilisateur est authentifié
    fn ensure_authenticated(&self) -> Result<&str> {
        self.user_id
            .as_deref()
            .ok_or_else(|| QobuzError::Unauthorized("Not authenticated".to_string()))
    }

    /// Récupère les albums favoris de l'utilisateur
    pub async fn get_favorite_albums(&self) -> Result<Vec<Album>> {
        let user_id = self.ensure_authenticated()?;
        debug!("Fetching favorite albums for user {}", user_id);

        let params = [("user_id", user_id), ("type", "albums"), ("limit", "1000")];

        let response: FavoritesResponse = self.get("/favorite/getUserFavorites", &params).await?;

        if let Some(albums) = response.albums {
            Ok(albums
                .items
                .into_iter()
                .map(QobuzApi::parse_album)
                .filter(|a| a.streamable)
                .collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Récupère les artistes favoris de l'utilisateur
    pub async fn get_favorite_artists(&self) -> Result<Vec<Artist>> {
        let user_id = self.ensure_authenticated()?;
        debug!("Fetching favorite artists for user {}", user_id);

        let params = [
            ("user_id", user_id),
            ("type", "artists"),
            ("limit", "1000"),
        ];

        let response: FavoritesResponse = self.get("/favorite/getUserFavorites", &params).await?;

        if let Some(artists) = response.artists {
            Ok(artists
                .items
                .into_iter()
                .map(QobuzApi::parse_artist)
                .collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Récupère les tracks favorites de l'utilisateur
    pub async fn get_favorite_tracks(&self) -> Result<Vec<Track>> {
        let user_id = self.ensure_authenticated()?;
        debug!("Fetching favorite tracks for user {}", user_id);

        let params = [("user_id", user_id), ("type", "tracks"), ("limit", "1000")];

        let response: FavoritesResponse = self.get("/favorite/getUserFavorites", &params).await?;

        if let Some(tracks) = response.tracks {
            Ok(tracks
                .items
                .into_iter()
                .map(|t| QobuzApi::parse_track(t, None))
                .filter(|t| t.streamable)
                .collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Récupère les playlists de l'utilisateur
    pub async fn get_user_playlists(&self) -> Result<Vec<Playlist>> {
        let user_id = self.ensure_authenticated()?;
        debug!("Fetching playlists for user {}", user_id);

        let params = [("user_id", user_id), ("limit", "1000")];

        let response: UserPlaylistsResponse =
            self.get("/playlist/getUserPlaylists", &params).await?;

        Ok(response
            .playlists
            .items
            .into_iter()
            .map(QobuzApi::parse_playlist)
            .collect())
    }

    /// Ajoute un album aux favoris
    pub async fn add_favorite_album(&self, album_id: &str) -> Result<()> {
        let user_id = self.ensure_authenticated()?;
        debug!("Adding album {} to favorites for user {}", album_id, user_id);

        let params = [
            ("album_id", album_id),
            ("user_id", user_id),
        ];

        self.get::<serde_json::Value>("/favorite/create", &params).await?;
        Ok(())
    }

    /// Supprime un album des favoris
    pub async fn remove_favorite_album(&self, album_id: &str) -> Result<()> {
        let user_id = self.ensure_authenticated()?;
        debug!("Removing album {} from favorites for user {}", album_id, user_id);

        let params = [
            ("album_ids", album_id),
            ("user_id", user_id),
        ];

        self.get::<serde_json::Value>("/favorite/delete", &params).await?;
        Ok(())
    }

    /// Ajoute un track aux favoris
    pub async fn add_favorite_track(&self, track_id: &str) -> Result<()> {
        let user_id = self.ensure_authenticated()?;
        debug!("Adding track {} to favorites for user {}", track_id, user_id);

        let params = [
            ("track_id", track_id),
            ("user_id", user_id),
        ];

        self.get::<serde_json::Value>("/favorite/create", &params).await?;
        Ok(())
    }

    /// Supprime un track des favoris
    pub async fn remove_favorite_track(&self, track_id: &str) -> Result<()> {
        let user_id = self.ensure_authenticated()?;
        debug!("Removing track {} from favorites for user {}", track_id, user_id);

        let params = [
            ("track_ids", track_id),
            ("user_id", user_id),
        ];

        self.get::<serde_json::Value>("/favorite/delete", &params).await?;
        Ok(())
    }

    /// Ajoute un track à une playlist
    pub async fn add_to_playlist(&self, playlist_id: &str, track_id: &str) -> Result<()> {
        let user_id = self.ensure_authenticated()?;
        debug!("Adding track {} to playlist {} for user {}", track_id, playlist_id, user_id);

        let params = [
            ("playlist_id", playlist_id),
            ("track_ids", track_id),
        ];

        self.get::<serde_json::Value>("/playlist/addTracks", &params).await?;
        Ok(())
    }
}
